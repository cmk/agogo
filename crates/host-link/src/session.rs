//! `LinkSession` — thin orchestrator composing `LinkClock`, the
//! transport FSM, and the per-session quantum.
//!
//! Stubbed in T0; filled out in T4 once tempo-push (T1) and the FSM
//! (T2) + quantum snap (T3) are in place.

use agogo_core::channel::Channel;
use agogo_core::conn::fixed::Micro;
use agogo_core::conn::phase::Phase;
use agogo_core::conn::tempo::Tempo;
use agogo_core::machine::ChannelSpec;
// Required for `LinkClock::phase_at_sample` (trait-provided method
// called by the `phase_at_sample` shim below). Copilot flagged this
// as unused on PR #16 round 1 — false positive: removing it breaks
// `cargo build --features rusty-link`.
use agogo_core::sync::PhaseSourceImpl;

use crate::link::{HostTimeAnchor, LinkClock};
use crate::quantum::Quantum;
use crate::transport::{TransportEvent, TransportFsm, TransportOutput, TransportState};

// `LinkSession::quantum` was originally a T0 placeholder for a
// session-level default quantum. T3's per-channel snap_to_quantum
// field superseded it briefly; audit P2 (Plan 20) then dropped that
// field too. The session now exposes a stateless
// `snap_offset_for(Option<Quantum>)` helper; callers (orchestrator
// or tests) pull the snap intent from `ChannelSpec::snap_intent()`
// and apply the returned `Micro` delta to their own `offset`.
// `LinkWriteConfig::default_quantum` remains on `config` for
// session-level defaults independent of per-channel snaps.

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
    pub fn new(initial_bpm: Tempo, anchor: HostTimeAnchor, config: LinkWriteConfig) -> Self {
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

    /// Compute the `Micro` delta required to snap a channel's next
    /// tick onto the nearest upcoming `q`-boundary on the Link
    /// timeline. Returns `Micro::ZERO` when `snap` is `None` (no
    /// snap intent).
    ///
    /// Stateless w.r.t. the channel — callers fold the returned delta
    /// into their own `Channel.offset` if they want arming behaviour.
    /// `ChannelSpec::snap_intent()` returns `Option<Micro>` (microbeats
    /// in `core`'s vocabulary) which the caller wraps into the
    /// host-link-shaped `Option<Quantum>` via `.map(Quantum)`:
    ///
    /// ```ignore
    /// let delta = session.snap_offset_for(spec.snap_intent().map(Quantum));
    /// ch.offset = Micro(ch.offset.0.saturating_add(delta.0));
    /// ```
    ///
    /// Pre-P2 this was `arm_channel(&mut Channel)` which mutated the
    /// channel directly; that coupling is gone — `LinkSession` no
    /// longer touches `agogo_core::channel::Channel`.
    ///
    /// Link's host-time model and agogo's `Micro` lattice are both
    /// microseconds, so no `SampleTickConn` is needed here —
    /// `PicoSampleConn` is only required downstream in
    /// `transform::micro_to_samples` when applying the sample rate.
    pub fn snap_offset_for(&mut self, snap: Option<Quantum>) -> Micro {
        match snap {
            Some(q) => self.clock.snap_offset_micro(q),
            None => Micro::ZERO,
        }
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

/// Apply per-channel snap deltas in bulk: for each `(spec, channel)`
/// pair, fold the result of
/// `session.snap_offset_for(spec.snap_intent().map(Quantum))` into
/// `channel.common_mut().offset`. The `.map(Quantum)` step wraps
/// `core`'s `Option<Micro>` into host-link's `Option<Quantum>` —
/// `core` doesn't know about `Quantum` (host-link-shaped type) so
/// the wrap happens here, at the host-link boundary.
///
/// Replaces the manual loop the orchestrator would otherwise duplicate
/// (Plan 20 shipped `snap_offset_for` as a per-call helper but left
/// the walk for callers; production `cli/src/run.rs` skipped it
/// entirely — Plan 2026-04-28-09 T1 consolidates the walk on the
/// host-link side so cli code doesn't need to grow another
/// `LinkSession` call site).
///
/// `specs` and `channels` must have the same length and be in the
/// same order. Mismatch is a programming error and panics in both
/// `debug` and `release` builds via `assert_eq!` — silent
/// `min(specs.len(), channels.len())` partial-apply would leave some
/// channels un-snapped with no diagnostic, which is worse than a
/// fail-loud panic at a known orchestrator boundary.
///
/// Channels whose spec has `snap_intent() == None` are unchanged. The
/// session is mutated as a side effect of each `snap_offset_for` call
/// (Link's audio-session-state capture).
pub fn apply_snap_offsets(
    specs: &[ChannelSpec],
    session: &mut LinkSession,
    channels: &mut [Channel],
) {
    assert_eq!(
        specs.len(),
        channels.len(),
        "apply_snap_offsets: specs ({}) / channels ({}) arity mismatch — \
         caller must pass parallel slices",
        specs.len(),
        channels.len(),
    );
    for (spec, channel) in specs.iter().zip(channels.iter_mut()) {
        let snap = spec.snap_intent().map(Quantum);
        let delta = session.snap_offset_for(snap);
        if delta != Micro::ZERO {
            let common = channel.common_mut();
            common.offset = Micro(common.offset.0.saturating_add(delta.0));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroU32;

    fn anchor_48k() -> HostTimeAnchor {
        HostTimeAnchor {
            host_origin_micros: 0,
            sample_rate: NonZeroU32::new(48_000).expect("non-zero"),
        }
    }

    #[test]
    fn snap_offset_for_none_is_zero() {
        let mut s = LinkSession::new(
            Tempo::from_bpm_integer(120),
            anchor_48k(),
            LinkWriteConfig::default(),
        );
        assert_eq!(s.snap_offset_for(None), Micro::ZERO);
    }

    /// `quantum_snap_nonneg` at the LinkSession layer: the returned
    /// snap delta is never negative, so adding it to a non-negative
    /// `offset` never reduces it. (Snap goes forward, always.)
    #[test]
    fn snap_offset_for_some_is_nonneg() {
        let mut s = LinkSession::new(
            Tempo::from_bpm_integer(120),
            anchor_48k(),
            LinkWriteConfig::default(),
        );
        let delta = s.snap_offset_for(Some(Quantum::from_bars(4)));
        assert!(delta.0 >= 0, "snap delta is negative: {delta:?}",);
    }

    /// `quantum_snap_idempotent` — two consecutive calls with the
    /// same intent yield deltas within 1 ms of each other. Bit-exact
    /// equality isn't achievable because Link's session state drifts
    /// microseconds between two `capture_audio_session_state` calls
    /// (the network sync protocol runs continuously). 1 ms of
    /// tolerance catches any real-world divergence while still being
    /// tight enough to flag a regression.
    #[test]
    fn snap_offset_for_is_near_idempotent() {
        let mut s = LinkSession::new(
            Tempo::from_bpm_integer(120),
            anchor_48k(),
            LinkWriteConfig::default(),
        );
        let q = Some(Quantum::from_bars(4));
        let a = s.snap_offset_for(q);
        let b = s.snap_offset_for(q);
        let drift = (a.0 - b.0).abs();
        assert!(
            drift < 1_000,
            "two consecutive snap_offset_for calls diverged by {drift} µs (>1 ms)",
        );
    }

    // ── Plan 2026-04-28-09 T1 — `apply_snap_offsets` helper. ──

    /// Build `(specs, channels)` from a slice of `--ch` mini-language
    /// strings. The two vectors are parallel — `channels[i]` was built
    /// from `specs[i]`. Mirrors the orchestrator's pre-helper shape.
    fn build_pair(spec_strs: &[&str]) -> (Vec<ChannelSpec>, Vec<Channel>) {
        let owned: Vec<String> = spec_strs.iter().map(|s| (*s).to_string()).collect();
        let named = agogo_core::machine::parse_channels(&owned).expect("parse spec");
        let specs: Vec<ChannelSpec> = named.iter().map(|(_, s)| s.clone()).collect();
        let channels: Vec<Channel> = specs
            .iter()
            .cloned()
            .map(|s| s.into_channel().expect("into_channel"))
            .collect();
        (specs, channels)
    }

    /// Empty inputs are a no-op — the helper handles a zero-channel
    /// list without panic and leaves the session quiescent. Catches a
    /// regression where the implementation might index unconditionally.
    #[test]
    fn apply_snap_offsets_no_panic_on_empty() {
        let mut s = LinkSession::new(
            Tempo::from_bpm_integer(120),
            anchor_48k(),
            LinkWriteConfig::default(),
        );
        let specs: Vec<ChannelSpec> = Vec::new();
        let mut channels: Vec<Channel> = Vec::new();
        apply_snap_offsets(&specs, &mut s, &mut channels);
        // No assertion beyond "didn't panic" — the empty case is the
        // edge we're pinning. `channels` stays empty by definition.
    }

    /// Two helper invocations against fresh-but-equivalent state
    /// produce per-channel offsets that agree within drift tolerance.
    /// The helper is a wrapper around N `snap_offset_for` calls, each
    /// of which has the ~1 ms drift bound — this property pins that
    /// the helper doesn't *amplify* that drift beyond the per-call
    /// tolerance × channel count.
    #[test]
    fn apply_snap_offsets_idempotent_per_channel() {
        let spec_strs = &[
            "dev=midi,grid=t4,snap-quantum-us=4000000",
            "dev=midi,grid=t8,snap-quantum-us=2000000",
        ];

        let mut session = LinkSession::new(
            Tempo::from_bpm_integer(120),
            anchor_48k(),
            LinkWriteConfig::default(),
        );

        // First invocation against fresh channels.
        let (specs1, mut channels1) = build_pair(spec_strs);
        apply_snap_offsets(&specs1, &mut session, &mut channels1);
        let offsets1: Vec<i64> = channels1.iter().map(|c| c.common().offset.0).collect();

        // Second invocation against fresh channels (same starting state).
        let (specs2, mut channels2) = build_pair(spec_strs);
        apply_snap_offsets(&specs2, &mut session, &mut channels2);
        let offsets2: Vec<i64> = channels2.iter().map(|c| c.common().offset.0).collect();

        // Liveness guard — both spec_strs entries are snap-armed, so
        // BOTH invocations must produce non-zero offsets. Without
        // this, a no-op `apply_snap_offsets` would leave offsets1 ==
        // offsets2 == [0, 0] and the `drift < 4_000` checks below
        // would pass trivially.
        assert!(
            offsets1.iter().any(|o| *o != 0),
            "invocation 1: expected at least one snap-armed offset to be non-zero \
             (got {offsets1:?})",
        );
        assert!(
            offsets2.iter().any(|o| *o != 0),
            "invocation 2: expected at least one snap-armed offset to be non-zero \
             (got {offsets2:?})",
        );

        for (i, (o1, o2)) in offsets1.iter().zip(offsets2.iter()).enumerate() {
            let drift = (o1 - o2).abs();
            assert!(
                drift < 4_000,
                "channel {i}: invocation 1 vs 2 diverged by {drift} µs (>4 ms): \
                 first={o1} second={o2}",
            );
        }
    }
}
