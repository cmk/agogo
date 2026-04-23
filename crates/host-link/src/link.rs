//! `LinkClock` — the agogo-side handle on an Ableton Link session.
//!
//! Scope in this sprint (Plan 07): lifecycle only — construct, enable,
//! read tempo, count peers. The `phase_at_sample` bridge lands in the
//! follow-up sprint after the fxp refactor sets the final sample /
//! phase types.
//!
//! RT-safety in brief: construction, enable, and disable are **not**
//! RT-safe — they open a UDP socket and spawn Link's internal
//! networking threads. The remaining read-only queries are RT-safe;
//! `tempo()` captures audio session state via
//! `capture_audio_session_state` (lock-free on Link's C++ side);
//! `is_enabled()` and `num_peers()` call the matching `AblLink`
//! methods, which are documented by rusty_link as RT-safe atomic
//! reads on the Link C++ handle.

use agogo_core::sync::PhaseSourceImpl;
use rusty_link::{AblLink, SessionState};

/// agogo's wrapper around an Ableton Link session.
pub struct LinkClock {
    link: AblLink,
    /// Scratch session-state buffer, reused across RT-safe reads to
    /// avoid allocating per call.
    session: SessionState,
}

impl LinkClock {
    /// Construct a Link session seeded at `initial_bpm`. Link's
    /// networking is **off** until [`LinkClock::enable`] is called;
    /// the session is a local-only timeline until then.
    ///
    /// # RT-safety
    ///
    /// Not RT-safe. Opens a UDP socket internally; run on the control
    /// thread only.
    pub fn new(initial_bpm: f64) -> Self {
        Self {
            link: AblLink::new(initial_bpm),
            session: SessionState::new(),
        }
    }

    /// Toggle peer discovery + session joining.
    ///
    /// # RT-safety
    ///
    /// Not RT-safe. Wraps a mutex acquisition on Link's C++ side.
    pub fn enable(&self, on: bool) {
        self.link.enable(on);
    }

    /// Whether peer discovery is currently on.
    ///
    /// RT-safe.
    pub fn is_enabled(&self) -> bool {
        self.link.is_enabled()
    }

    /// Current session tempo in BPM.
    ///
    /// RT-safe — captures the audio session state (lock-free) and
    /// reads the tempo field.
    pub fn tempo(&mut self) -> f64 {
        self.link.capture_audio_session_state(&mut self.session);
        self.session.tempo()
    }

    /// Number of peers currently joined to the session.
    ///
    /// RT-safe.
    pub fn num_peers(&self) -> u64 {
        self.link.num_peers()
    }
}

impl PhaseSourceImpl for LinkClock {
    /// **Deferred.** The Tick ↔ Host-Time bridge lands in the
    /// follow-up sprint after the in-flight fixed-point refactor
    /// (`plan/2026-04-23-03`) merges — see `doc/plans/plan-2026-04-23-04.md`
    /// §Deferred. Panics on call so misuse surfaces immediately
    /// rather than silently returning 0.0.
    fn phase_at_sample(&mut self, _n: u64) -> f32 {
        todo!(
            "LinkClock::phase_at_sample is deferred to Plan 07-b; \
             requires the fxp Phase / Sample types"
        )
    }

    /// Link derives tempo from network, not from audio input; this
    /// method is deliberately a no-op. Matches `PhaseSource::Internal`'s
    /// behaviour.
    fn feed_samples(&mut self, _samples: &[f32], _start: u64) {
        // no-op by design
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_does_not_panic_across_bpm_range() {
        // Construct at representative BPMs; drop without enabling so
        // peer discovery / multicast join never starts. (Link's C++
        // side still opens a UDP socket on construction; we just
        // don't announce presence to the LAN.)
        for bpm in [60.0, 90.0, 120.0, 137.0, 200.0] {
            let _c = LinkClock::new(bpm);
        }
    }

    #[test]
    fn tempo_reads_back_initial_bpm() {
        // Link internally clamps to [20, 999] — 137.0 passes through.
        let mut c = LinkClock::new(137.0);
        let t = c.tempo();
        assert!(
            (t - 137.0).abs() < 1e-9,
            "tempo {t} differs from initial 137.0"
        );
    }

    #[test]
    fn num_peers_zero_before_enable() {
        // Disabled session has no discovery running → no peers.
        let c = LinkClock::new(120.0);
        assert_eq!(c.num_peers(), 0);
    }

    #[test]
    fn is_enabled_tracks_enable_call() {
        // Touches the network: `enable(true)` opens Link's UDP
        // multicast listener. Fine on a dev box and GitHub-hosted
        // CI runners, but sandboxed / multicast-less environments
        // may fail. Plan 08 adds a `fixture_or_skip!`-style network
        // gate when the multicast-dependent integration tests land.
        let c = LinkClock::new(120.0);
        assert!(!c.is_enabled());
        c.enable(true);
        assert!(c.is_enabled());
        c.enable(false);
        assert!(!c.is_enabled());
    }

    #[test]
    #[should_panic(expected = "not yet implemented")]
    fn phase_at_sample_panics_until_plan_07b() {
        // `todo!()` panics with "not yet implemented" regardless of
        // the message argument (as of stable Rust). Our custom message
        // appears in the panic payload but not the `expected` prefix;
        // match on the stable prefix instead.
        let mut c = LinkClock::new(120.0);
        let _ = c.phase_at_sample(0);
    }

    #[test]
    fn feed_samples_is_noop_and_preserves_tempo() {
        let mut c = LinkClock::new(120.0);
        c.feed_samples(&[0.1, 0.2, 0.3], 0);
        // Tempo unchanged by audio input.
        assert!((c.tempo() - 120.0).abs() < 1e-9);
    }
}
