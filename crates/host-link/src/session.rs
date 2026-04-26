//! `LinkSession` — thin orchestrator composing `LinkClock`, the
//! transport FSM, and the per-session quantum.
//!
//! Stubbed in T0; filled out in T4 once tempo-push (T1) and the FSM
//! (T2) + quantum snap (T3) are in place.

use agogo_core::channel::Channel;
use agogo_core::fxp::{Micro, Phase, Quantum, Tempo};
// Required for `LinkClock::phase_at_sample` (trait-provided method
// called by the `phase_at_sample` shim below). Copilot flagged this
// as unused on PR #16 round 1 — false positive: removing it breaks
// `cargo build --features rusty-link`.
use agogo_core::sync::PhaseSourceImpl;

use crate::link::{HostTimeAnchor, LinkClock};
use crate::transport::{TransportEvent, TransportFsm, TransportOutput, TransportState};

// `LinkSession::quantum` was originally a T0 placeholder for a
// session-level default quantum. T3's per-channel
// `ch.snap_to_quantum` field superseded it: `arm_channel` reads
// `ch.snap_to_quantum` directly, with `LinkWriteConfig::default_quantum`
// held on `config` as the authoritative default. Removed to avoid
// duplicating state that the v0.5 Sprint 01 developer might mistake
// for load-bearing.

/// Write-path configuration for `LinkSession`. Tunes which classes
/// of transport / tempo events agogo emits vs. only observes.
#[derive(Debug, Clone, Copy)]
pub struct LinkWriteConfig {
    /// When `true`, `poll_transport` drives the FSM with
    /// `LinkReports*` events that mirror the network transport.
    pub enable_start_stop_sync: bool,
    /// When `true`, `user_start` / `user_stop` publish one-shot to
    /// the Link session via `set_is_playing`. When `false`, the FSM
    /// state still flips but no network write happens.
    pub enable_start_stop: bool,
    /// Default quantum used when channels don't override it.
    pub default_quantum: Quantum,
    /// When `true`, `set_tempo` on this session pushes the new tempo
    /// to the Link network. When `false`, only the local session's
    /// view of tempo changes (useful for hold-and-preview UI flows).
    pub push_tempo_on_change: bool,
}

impl Default for LinkWriteConfig {
    fn default() -> Self {
        Self {
            enable_start_stop_sync: true,
            enable_start_stop: true,
            default_quantum: Quantum::from_bars(4),
            push_tempo_on_change: true,
        }
    }
}

/// Thin orchestrator — owns a `LinkClock`, the transport FSM, and
/// the write-path config. Plan 06's `Machine` eventually absorbs
/// this.
pub struct LinkSession {
    clock: LinkClock,
    transport: TransportFsm,
    config: LinkWriteConfig,
}

impl LinkSession {
    /// Construct a session. The underlying `LinkClock` is constructed
    /// but not enabled — call `enable` explicitly.
    pub fn new(
        initial_bpm: Tempo,
        anchor: HostTimeAnchor,
        config: LinkWriteConfig,
    ) -> Self {
        Self {
            clock: LinkClock::new(initial_bpm, anchor),
            transport: rust_fsm::StateMachine::new(),
            config,
        }
    }

    /// Toggle peer discovery + session joining. When
    /// `enable_start_stop_sync` is configured, the underlying Link
    /// instance's start-stop-sync flag follows `on` symmetrically so
    /// `is_playing` propagates across peers (off by default in Link)
    /// while the session is active, and is cleared on disable so
    /// peers stop receiving publishes from this instance.
    pub fn enable(&self, on: bool) {
        self.clock.enable(on);
        if self.config.enable_start_stop_sync {
            self.clock.enable_start_stop_sync(on);
        }
    }

    /// Number of peers currently joined.
    pub fn num_peers(&self) -> u64 {
        self.clock.num_peers()
    }

    /// Current tempo. RT-safe (delegates to `LinkClock::tempo`).
    pub fn tempo(&mut self) -> Tempo {
        self.clock.tempo()
    }

    /// Beat-phase ([0, 1) cycles) at the given absolute sample
    /// index. RT-safe — delegates to [`LinkClock::phase_at_sample`].
    /// Plan 14's `LinkPhaseSource` adapter routes through this so
    /// the audio thread can read phase via the same `Arc<Mutex<_>>`
    /// the control thread holds.
    pub fn phase_at_sample(&mut self, n: u64) -> Phase {
        self.clock.phase_at_sample(n)
    }

    /// Snapshot of the current transport state.
    pub fn transport_state(&self) -> &TransportState {
        self.transport.state()
    }

    /// Whether the FSM currently reports Playing. Convenience wrapper
    /// around `transport_state`.
    pub fn is_playing(&self) -> bool {
        matches!(self.transport.state(), TransportState::Playing)
    }

    /// Drive the FSM with `UserStart`. When `enable_start_stop` is
    /// on and the transition emits `PublishPlaying`, this calls
    /// through to T1's push-path (filled out once T1 lands).
    ///
    /// Control-thread only.
    pub fn user_start(&mut self) {
        self.drive_and_publish(TransportEvent::UserStart);
    }

    /// Drive the FSM with `UserStop`.
    ///
    /// Control-thread only.
    pub fn user_stop(&mut self) {
        self.drive_and_publish(TransportEvent::UserStop);
    }

    /// Poll Link's `is_playing` flag and drive the FSM with the
    /// corresponding `LinkReports*` event. No-op when
    /// `enable_start_stop_sync` is off. Control-thread only.
    pub fn poll_transport(&mut self) {
        if !self.config.enable_start_stop_sync {
            return;
        }
        let playing = self.clock.is_playing_session();
        let ev = if playing {
            TransportEvent::LinkReportsPlaying
        } else {
            TransportEvent::LinkReportsStopped
        };
        self.drive_and_publish(ev);
    }

    /// Control-thread: push a new BPM through the underlying
    /// `LinkClock`. When `push_tempo_on_change` is off this is a
    /// no-op (useful for UI hold-and-preview flows).
    pub fn set_tempo(&mut self, bpm: Tempo) {
        if self.config.push_tempo_on_change {
            self.clock.push_tempo(bpm);
        }
    }

    /// Control-thread: if `ch.snap_to_quantum` is `Some(q)`, compute
    /// the micro-offset that places the channel's next tick on the
    /// nearest upcoming `q`-boundary and add it into `ch.offset`. No-op
    /// when `snap_to_quantum` is `None`.
    ///
    /// Deviation from the plan draft: no `stc: &SampleTickConn` arg
    /// needed. Link's host-time model and agogo's `Micro` lattice are
    /// both microseconds, so the snap delta is already in the target
    /// type — `PicoSampleConn` is only needed downstream in
    /// `transform::micro_to_samples` when applying the sample rate.
    /// Documented in Plan 09 §Review.
    pub fn arm_channel(&mut self, ch: &mut Channel) {
        let q = match ch.snap_to_quantum {
            Some(q) => q,
            None => return,
        };
        let delta = self.clock.snap_offset_micro(q);
        ch.offset = Micro(ch.offset.0.saturating_add(delta.0));
    }

    fn drive_and_publish(&mut self, ev: TransportEvent) {
        let out = self
            .transport
            .consume(&ev)
            .expect("FSM declares all (state × event) combos");
        if let Some(o) = out {
            if self.config.enable_start_stop {
                match o {
                    TransportOutput::PublishPlaying => self.clock.publish_is_playing(true),
                    TransportOutput::PublishStopped => self.clock.publish_is_playing(false),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agogo_core::channel::{Channel, ChannelMode, MAX_DELAY};
    use agogo_core::time::grid::Grid;
    use agogo_core::time::swing::SwingConfig;
    use agogo_core::time::tbase::TBase;
    use std::num::NonZeroU32;

    fn anchor_48k() -> HostTimeAnchor {
        HostTimeAnchor {
            host_origin_micros: 0,
            sample_rate: NonZeroU32::new(48_000).expect("non-zero"),
        }
    }

    fn channel_with_snap(q: Option<Quantum>) -> Channel {
        Channel {
            mode: ChannelMode::MidiClock,
            divider: Grid::T4,
            shuffle: SwingConfig {
                resolution: TBase::T16,
                amount: 0,
            },
            delay: Micro::ZERO,
            offset: Micro::ZERO,
            snap_to_quantum: q,
            bar_multiplier: None,
        }
    }

    #[test]
    fn arm_channel_noop_when_snap_is_none() {
        let mut s = LinkSession::new(
            Tempo::from_bpm_integer(120),
            anchor_48k(),
            LinkWriteConfig::default(),
        );
        let mut ch = channel_with_snap(None);
        let offset_before = ch.offset;
        s.arm_channel(&mut ch);
        assert_eq!(ch.offset, offset_before);
    }

    /// `quantum_snap_nonneg` at the LinkSession layer: arm_channel
    /// never reduces `offset` below its starting value when the
    /// starting value is ≥ 0. (Snap goes forward, always.)
    #[test]
    fn arm_channel_never_reduces_offset() {
        let mut s = LinkSession::new(
            Tempo::from_bpm_integer(120),
            anchor_48k(),
            LinkWriteConfig::default(),
        );
        for start in [Micro::ZERO, Micro(1_000), Micro(MAX_DELAY.0 / 2)] {
            let mut ch = channel_with_snap(Some(Quantum::from_bars(4)));
            ch.offset = start;
            s.arm_channel(&mut ch);
            assert!(
                ch.offset.0 >= start.0,
                "snap reduced offset from {:?} to {:?}",
                start, ch.offset
            );
        }
    }

    /// `quantum_snap_idempotent` — calling `arm_channel` twice in
    /// quick succession on two identical channel copies yields
    /// offsets within 1 ms of each other. "Bit-exact" isn't
    /// achievable because Link's session state drifts microseconds
    /// between two `capture_audio_session_state` calls (the network
    /// sync protocol runs continuously). 1 ms of tolerance catches
    /// any real-world divergence while still being tight enough to
    /// flag a regression.
    #[test]
    fn arm_channel_is_near_idempotent() {
        let mut s = LinkSession::new(
            Tempo::from_bpm_integer(120),
            anchor_48k(),
            LinkWriteConfig::default(),
        );
        let q = Some(Quantum::from_bars(4));
        let mut ch_a = channel_with_snap(q);
        let mut ch_b = channel_with_snap(q);
        s.arm_channel(&mut ch_a);
        s.arm_channel(&mut ch_b);
        let drift = (ch_a.offset.0 - ch_b.offset.0).abs();
        assert!(
            drift < 1_000,
            "two consecutive arm_channel calls diverged by {} µs (>1 ms)",
            drift
        );
    }
}
