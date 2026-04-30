//! `LinkPhaseSource` — adapter wrapping [`LinkSession`] for use as
//! [`agogo_core::control::sync::PhaseSource::Custom`].
//!
//! The audio callback needs `phase_at_sample(n)`; the control thread
//! needs `is_playing()`, `poll_transport()`, `set_tempo()`, etc. Both
//! sides talk to the same `LinkSession` via an `Arc<Mutex<_>>`. The
//! lock is uncontended on the audio thread because the control thread
//! only writes at human-pace boundaries (Ctrl-C, `--push-tempo`); the
//! audio-thread read is sub-µs and bounded by Link's own
//! `capture_audio_session_state`. v0.5 Sprint 02 swaps this for a
//! seqlock pattern per `link.md:23-29` once the precision matters.
//!
//! Plan 14 wires this in at the CLI boundary when the user passes
//! `--source link`.

use std::sync::{Arc, Mutex};

use agogo_core::conn::phase::Phase;
use agogo_core::conn::tempo::Tempo;
use agogo_core::control::sync::PhaseSourceImpl;

use crate::session::LinkSession;

/// Audio-thread side of the Link adapter. Implements
/// [`PhaseSourceImpl`] over a shared `Arc<Mutex<LinkSession>>`.
///
/// Drop into `PhaseSource::Custom(Box::new(link_phase_source))`. The
/// `feed_samples` impl is a no-op (Link derives tempo from the
/// network, not from the audio callback's PCM input).
pub struct LinkPhaseSource {
    inner: Arc<Mutex<LinkSession>>,
}

impl LinkPhaseSource {
    /// Build a [`LinkPhaseSource`] from an owned [`LinkSession`].
    /// Returns the source plus a [`LinkSessionHandle`] for the
    /// control thread.
    pub fn new(session: LinkSession) -> (Self, LinkSessionHandle) {
        let inner = Arc::new(Mutex::new(session));
        (
            Self {
                inner: Arc::clone(&inner),
            },
            LinkSessionHandle { inner },
        )
    }

    /// Mint another control-thread handle. Useful when more than one
    /// thread (e.g. Ctrl-C handler + main loop) needs to interact
    /// with the session.
    pub fn handle(&self) -> LinkSessionHandle {
        LinkSessionHandle {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl PhaseSourceImpl for LinkPhaseSource {
    fn phase_at_sample(&mut self, n: u64) -> Phase {
        // Lock acquisition is uncontended on the audio thread because
        // the control thread writes at human pace. v0.5 Sprint 02
        // swaps this for a seqlock-packed anchor when the per-buffer
        // re-anchoring lands.
        self.inner
            .lock()
            .expect("LinkSession Mutex poisoned")
            .phase_at_sample(n)
    }

    fn feed_samples(&mut self, _samples: &[f32], _start: u64) {
        // PCM ABI: `&[f32]` is the cpal audio I/O slice shape; the
        // parameter is required by the `PhaseSourceImpl` trait but
        // ignored here because Link derives tempo from the network,
        // not the audio stream. No PCM coupling.
    }
}

/// Control-thread handle for a [`LinkSession`] wrapped behind a
/// [`LinkPhaseSource`]. Cheaply cloneable; multiple callers can
/// share one (e.g. the Ctrl-C handler + the main poll loop).
#[derive(Clone)]
pub struct LinkSessionHandle {
    inner: Arc<Mutex<LinkSession>>,
}

impl LinkSessionHandle {
    /// Whether the FSM currently reports Playing.
    pub fn is_playing(&self) -> bool {
        self.inner
            .lock()
            .expect("LinkSession Mutex poisoned")
            .is_playing()
    }

    /// Drive the FSM by polling Link's `is_playing` flag. Call this
    /// from the control thread (Plan 14's main loop park-and-poll
    /// pattern).
    pub fn poll_transport(&self) {
        self.inner
            .lock()
            .expect("LinkSession Mutex poisoned")
            .poll_transport();
    }

    /// Drive the FSM with `UserStart`.
    pub fn user_start(&self) {
        self.inner
            .lock()
            .expect("LinkSession Mutex poisoned")
            .user_start();
    }

    /// Drive the FSM with `UserStop`. The Ctrl-C handler in Plan 14's
    /// `agogo run` calls this to mirror the local Stop into the Link
    /// session.
    pub fn user_stop(&self) {
        self.inner
            .lock()
            .expect("LinkSession Mutex poisoned")
            .user_stop();
    }

    /// Push a new tempo to the Link network.
    pub fn set_tempo(&self, bpm: Tempo) {
        self.inner
            .lock()
            .expect("LinkSession Mutex poisoned")
            .set_tempo(bpm);
    }

    /// Read current tempo. Locks; `is_playing` / `set_tempo` /
    /// `tempo` are equivalently lightweight at the lock layer.
    pub fn tempo(&self) -> Tempo {
        self.inner
            .lock()
            .expect("LinkSession Mutex poisoned")
            .tempo()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::link::HostTimeAnchor;
    use crate::session::LinkWriteConfig;
    use std::num::NonZeroU32;

    fn anchor_48k() -> HostTimeAnchor {
        HostTimeAnchor {
            host_origin_micros: 0,
            sample_rate: NonZeroU32::new(48_000).expect("non-zero"),
        }
    }

    /// Smoke: construct + read phase + read tempo. Validates the
    /// Arc<Mutex<_>> wiring compiles and locks symmetrically.
    #[test]
    fn link_phase_source_round_trip() {
        let session = LinkSession::new(
            Tempo::from_bpm_integer(120),
            anchor_48k(),
            LinkWriteConfig::default(),
        );
        let (mut src, handle) = LinkPhaseSource::new(session);

        // Audio-thread style read.
        let _phase = src.phase_at_sample(0);

        // Control-thread style reads + writes.
        assert!(!handle.is_playing());
        let _t = handle.tempo();
        handle.set_tempo(Tempo::from_bpm_integer(140));
        assert_eq!(handle.tempo(), Tempo::from_bpm_integer(140));

        // Phase read still works after the tempo push.
        let _phase = src.phase_at_sample(48_000);
    }

    /// Plan 14 property `link_phase_source_no_deadlock`: interleaved
    /// `phase_at_sample` (audio-thread proxy) and `set_tempo` /
    /// `is_playing` (control-thread proxy) for ~100 ms must not
    /// deadlock or panic. Lock-contention sanity, not a performance
    /// bound.
    #[test]
    fn link_phase_source_no_deadlock() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::time::{Duration, Instant};

        let session = LinkSession::new(
            Tempo::from_bpm_integer(120),
            anchor_48k(),
            LinkWriteConfig::default(),
        );
        let (mut src, handle) = LinkPhaseSource::new(session);
        let stop = Arc::new(AtomicBool::new(false));
        let h_stop = Arc::clone(&stop);

        let writer = std::thread::spawn(move || {
            let start = Instant::now();
            let mut bpm = 120u32;
            while start.elapsed() < Duration::from_millis(100) && !h_stop.load(Ordering::Acquire) {
                handle.set_tempo(Tempo::from_bpm_integer(bpm));
                let _ = handle.is_playing();
                bpm = if bpm >= 200 { 60 } else { bpm + 1 };
                std::thread::yield_now();
            }
        });

        let start = Instant::now();
        let mut n = 0u64;
        while start.elapsed() < Duration::from_millis(100) {
            let _phase = src.phase_at_sample(n);
            n = n.wrapping_add(48);
        }
        stop.store(true, Ordering::Release);
        writer.join().expect("writer thread panicked");
    }
}
