//! layer: link
//! depends-on: quantum
//!
//! `LinkClock` — the agogo-side handle on an Ableton Link session.
//!
//! Implements `agogo::chan::control::PhaseSourceImpl` over rusty_link: the
//! lifecycle surface (construct / enable / tempo / num_peers) from
//! Plan 07, plus the host-time bridge (Plan 08) that maps a
//! stream-global sample index to Link's host-time domain and returns
//! the session's beat-phase as an fxp `Phase`.
//!
//! RT-safety in brief: construction, enable, and disable are **not**
//! RT-safe — they open a UDP socket and spawn Link's internal
//! networking threads. The remaining read-only queries are RT-safe;
//! `tempo()` and `phase_at_sample()` capture audio session state via
//! `capture_audio_session_state` (lock-free on Link's C++ side);
//! `is_enabled()` and `num_peers()` call the matching `AblLink`
//! methods, which are documented by rusty_link as RT-safe atomic
//! reads on the Link C++ handle.

use std::num::NonZeroU32;

use agogo::chan::conn::fixed::Micro;
use agogo::chan::conn::float::F064FD06;
use agogo::chan::conn::float::{Extended, ExtendedFloat};
use agogo::chan::conn::float::{f64_phase_to_phase, tempo_to_f64_bpm};
use agogo::chan::conn::phase::Phase;
use agogo::chan::conn::tempo::Tempo;
use agogo::chan::control::PhaseSourceImpl;

use crate::quantum::Quantum;
use rusty_link::{AblLink, SessionState};

/// Static mapping from stream-global sample indices to Link's
/// host-time domain.
///
/// Plan 09 will replace this static form with a per-buffer
/// atomic-packed anchor updated from the audio callback (torn-read
/// safe via a generation counter). This sprint ships the static
/// shape — sufficient for the CLI probe and any pre-audio-callback
/// test path.
#[derive(Debug, Clone, Copy)]
pub struct HostTimeAnchor {
    /// Link host-time (microseconds, `i64`) corresponding to absolute
    /// sample index 0.
    pub host_origin_micros: i64,
    /// Sample rate used as the denominator in `n → host-µs`.
    /// `NonZeroU32` rather than `u32` so the division in
    /// [`LinkClock::phase_at_sample`] can't panic on a zero anchor —
    /// the invariant is enforced at the type level rather than via a
    /// runtime check on the RT-safe read path.
    pub sample_rate: NonZeroU32,
}

/// agogo's wrapper around an Ableton Link session.
pub struct LinkClock {
    link: AblLink,
    /// Scratch session-state buffer, reused across RT-safe reads to
    /// avoid allocating per call.
    session: SessionState,
    anchor: HostTimeAnchor,
}

impl LinkClock {
    /// Construct a Link session seeded at `initial_bpm` and anchored
    /// via `anchor`. Link's networking is **off** until
    /// [`LinkClock::enable`] is called; the session is a local-only
    /// timeline until then.
    ///
    /// # RT-safety
    ///
    /// Not RT-safe. Opens a UDP socket internally; run on the control
    /// thread only.
    pub fn new(initial_bpm: Tempo, anchor: HostTimeAnchor) -> Self {
        // Link FFI: AblLink's C++ constructor takes f64 BPM. Contain
        // the one-shot `Tempo → f64` cast to this line via the
        // lawful `tempo_to_f64_bpm` (F064FD06.inner under the hood);
        // downstream agogo never sees the f64.
        let initial_bpm_f64 = tempo_to_f64_bpm(initial_bpm);
        Self {
            link: AblLink::new(initial_bpm_f64),
            session: SessionState::new(),
            anchor,
        }
    }

    /// Replace the host-time anchor. Plan 09's audio callback uses
    /// this per-buffer; this sprint treats the anchor as immutable
    /// after construction in practice, but the setter is already
    /// here so later tests + integration paths don't need an API
    /// change.
    ///
    /// RT-safety: the setter writes two scalar fields and does not
    /// block, but it requires exclusive `&mut self` access, so it is
    /// not concurrently callable with readers without external
    /// synchronization. Plan 09 will promote the anchor to an
    /// atomic-packed variant for torn-read safety on the audio
    /// thread.
    pub fn set_anchor(&mut self, anchor: HostTimeAnchor) {
        self.anchor = anchor;
    }

    /// Current anchor.
    pub fn anchor(&self) -> HostTimeAnchor {
        self.anchor
    }

    /// Grab the current Link host-time (µs since some arbitrary
    /// platform epoch). Useful for synthesising a fresh anchor.
    ///
    /// RT-safe.
    pub fn clock_micros(&self) -> i64 {
        self.link.clock_micros()
    }

    /// Toggle peer discovery + session joining.
    ///
    /// # RT-safety
    ///
    /// Not RT-safe. Wraps a mutex acquisition on Link's C++ side.
    pub fn enable(&self, on: bool) {
        self.link.enable(on);
    }

    /// Toggle Link's start-stop synchronization. **Required** for
    /// `set_is_playing` / `is_playing` to propagate across peers —
    /// off by default in Link. Agogo's `LinkSession` calls this
    /// through from `LinkWriteConfig.enable_start_stop_sync`. Not
    /// RT-safe.
    pub fn enable_start_stop_sync(&self, on: bool) {
        self.link.enable_start_stop_sync(on);
    }

    /// Whether peer discovery is currently on.
    ///
    /// RT-safe.
    pub fn is_enabled(&self) -> bool {
        self.link.is_enabled()
    }

    /// Current session tempo.
    ///
    /// RT-safe — captures the audio session state (lock-free) and
    /// reads the tempo field.
    pub fn tempo(&mut self) -> Tempo {
        self.link.capture_audio_session_state(&mut self.session);
        // Link FFI: AblLink returns BPM as f64. `f64_bpm_to_tempo`
        // handles the one-shot conversion to the `Tempo` newtype
        // (µBPM u32, saturating on out-of-range).
        agogo::chan::conn::float::f64_bpm_to_tempo(self.session.tempo())
    }

    /// Number of peers currently joined to the session.
    ///
    /// RT-safe.
    pub fn num_peers(&self) -> u64 {
        self.link.num_peers()
    }

    /// Push a new BPM to the Link session at the current host-time.
    /// Link smooths peer-side; no local ramp.
    ///
    /// # RT-safety
    ///
    /// Not RT-safe. `commit_audio_session_state` allocates internally
    /// in rusty_link. Run on the control thread, never the audio
    /// thread.
    pub fn push_tempo(&mut self, bpm: Tempo) {
        self.link.capture_audio_session_state(&mut self.session);
        // Link FFI — Tempo (µBPM) → f64 BPM at the set_tempo
        // boundary via the lawful `tempo_to_f64_bpm` (F064FD06.inner).
        let bpm_f64 = tempo_to_f64_bpm(bpm);
        self.session.set_tempo(bpm_f64, self.link.clock_micros());
        self.link.commit_audio_session_state(&self.session);
    }

    /// Observe the session's current `is_playing` flag. Refreshes the
    /// cached session state via `capture_audio_session_state` and
    /// reads the flag. RT-safe.
    pub fn is_playing_session(&mut self) -> bool {
        self.link.capture_audio_session_state(&mut self.session);
        self.session.is_playing()
    }

    /// One-shot publish of a transport state change to the Link
    /// session. `true` → all peers see `is_playing = true`; `false` →
    /// `is_playing = false`. Control-thread only — invokes
    /// `commit_audio_session_state`.
    pub fn publish_is_playing(&mut self, playing: bool) {
        self.link.capture_audio_session_state(&mut self.session);
        self.session
            .set_is_playing(playing, self.link.clock_micros());
        self.link.commit_audio_session_state(&self.session);
    }

    /// Micro-seconds until the next `quantum`-boundary after `now`.
    /// Pure query — no publish, no commit. Returns `Micro::ZERO` if
    /// `quantum` is non-positive, or if the computed delta would be
    /// negative (paranoid guard; should not happen unless Link's
    /// session is wildly stale).
    ///
    /// Both agogo's `Micro` and Link's host-time domain are
    /// microseconds, so the result is already in the right lattice —
    /// no sample-rate conversion needed. The caller adds this `Micro`
    /// delta to `channel.offset`; the downstream
    /// `transform::micro_to_samples` then applies the sample rate.
    ///
    /// # RT-safety
    ///
    /// Not RT-safe — `capture_audio_session_state` refreshes the
    /// cached session. Run on the control thread.
    pub fn snap_offset_micro(&mut self, quantum: Quantum) -> Micro {
        // Quantum (Micro / microbeats) → f64 beats via the lawful
        // F064FD06 Conn inverse. The `10⁶` unit shift lives inside
        // `F064FD06`'s definition (`agogo::chan::conn::float`).
        // `Extended::Finite` lifts the `Micro` into the saturation
        // lattice F064FD06 operates on; `Bot`/`Top` are unreachable
        // for a finite `Quantum` but the match keeps the result total.
        let q_f64 = match F064FD06.inner(Extended::Finite(quantum.0)) {
            ExtendedFloat::Extend(b) => b,
            ExtendedFloat::Bot | ExtendedFloat::Top => return Micro::ZERO,
        };
        if q_f64 <= 0.0 || q_f64.is_nan() {
            return Micro::ZERO;
        }
        self.link.capture_audio_session_state(&mut self.session);
        let now = self.link.clock_micros();
        // Link FFI: beat_at_time / time_at_beat both take/return f64
        // beats. The arithmetic on f64 stays within 1–2 lines of each
        // FFI call (CLAUDE.md exception 5). Plan 2026-04-28-03 T10
        // inlined this from a helper to keep the f64 footprint
        // visible at the API boundary.
        let current_beat = self.session.beat_at_time(now, q_f64); // Link FFI
        let next_boundary = (current_beat / q_f64).ceil() * q_f64; // Link FFI (f64 next to call)
        let next_us = self.session.time_at_beat(next_boundary, q_f64); // Link FFI
        Micro(next_us.saturating_sub(now).max(0))
    }
}

impl PhaseSourceImpl for LinkClock {
    /// Sample → host-time → Link session phase (`[0, 1)`, beat-phase).
    ///
    /// Bridge math keeps `n × 10⁶` in `i128` so multi-day sample
    /// streams don't overflow the multiplication. The Link side
    /// returns `f64`; `f64_phase_to_phase` handles the `rem_euclid`
    /// and the "rounds to `2^32`" edge case, so the output is always
    /// a valid `Phase` in `[0, 2^32)`.
    ///
    /// RT-safe. `capture_audio_session_state` is lock-free per
    /// rusty_link's docs; the rest is arithmetic on owned state.
    fn phase_at_sample(&mut self, n: u64) -> Phase {
        let offset = (i128::from(n) * 1_000_000) / i128::from(self.anchor.sample_rate.get());
        let host_micros = (i128::from(self.anchor.host_origin_micros) + offset)
            .clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64;
        self.link.capture_audio_session_state(&mut self.session);
        let p = self.session.phase_at_time(host_micros, 1.0);
        f64_phase_to_phase(p)
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

    /// 48 kHz / `host_origin_micros = 0` — convenient for tests that
    /// don't care about wall-clock alignment.
    fn sr_48k() -> NonZeroU32 {
        NonZeroU32::new(48_000).expect("48 000 is non-zero")
    }

    fn zero_anchor_48k() -> HostTimeAnchor {
        HostTimeAnchor {
            host_origin_micros: 0,
            sample_rate: sr_48k(),
        }
    }

    #[test]
    fn new_does_not_panic_across_bpm_range() {
        // Construct at representative BPMs; drop without enabling so
        // peer discovery / multicast join never starts. (Link's C++
        // side still opens a UDP socket on construction; we just
        // don't announce presence to the LAN.)
        for bpm in [60u32, 90, 120, 137, 200] {
            let _c = LinkClock::new(Tempo::from_bpm_integer(bpm), zero_anchor_48k());
        }
    }

    #[test]
    fn tempo_reads_back_initial_bpm() {
        // Link internally clamps to [20, 999] — 137 BPM passes through.
        let mut c = LinkClock::new(Tempo::from_bpm_integer(137), zero_anchor_48k());
        assert_eq!(
            c.tempo(),
            Tempo::from_bpm_integer(137),
            "tempo differs from initial 137 BPM"
        );
    }

    #[test]
    fn num_peers_zero_before_enable() {
        // Disabled session has no discovery running → no peers.
        let c = LinkClock::new(Tempo::from_bpm_integer(120), zero_anchor_48k());
        assert_eq!(c.num_peers(), 0);
    }

    #[test]
    fn is_enabled_tracks_enable_call() {
        // Touches the network: `enable(true)` opens Link's UDP
        // multicast listener. Fine on a dev box and GitHub-hosted
        // CI runners, but sandboxed / multicast-less environments
        // may fail. Plan 09 adds a `fixture_or_skip!`-style network
        // gate when the multicast-dependent integration tests land.
        let c = LinkClock::new(Tempo::from_bpm_integer(120), zero_anchor_48k());
        assert!(!c.is_enabled());
        c.enable(true);
        assert!(c.is_enabled());
        c.enable(false);
        assert!(!c.is_enabled());
    }

    #[test]
    fn feed_samples_is_noop_and_preserves_tempo() {
        let mut c = LinkClock::new(Tempo::from_bpm_integer(120), zero_anchor_48k());
        c.feed_samples(&[0.1, 0.2, 0.3], 0);
        // Tempo unchanged by audio input.
        assert_eq!(c.tempo(), Tempo::from_bpm_integer(120));
    }

    // ── Phase bridge ──────────────────────────────────────────────

    #[test]
    fn phase_at_sample_returns_valid_phase() {
        // Bridge must always return a valid Phase (< 2^32); no
        // NaN/inf can sneak through `f64_phase_to_phase`.
        let mut c = LinkClock::new(Tempo::from_bpm_integer(120), zero_anchor_48k());
        for n in [0u64, 1, 48_000, 1_000_000, 10_000_000_000] {
            let _ = c.phase_at_sample(n);
        }
    }

    /// Smallest wrap-around distance between two Q0.32 phases. Works
    /// in `u32` arithmetic without overflow: the two candidates are
    /// `diff = b - a (mod 2^32)` and its `0 - diff (mod 2^32)`
    /// complement; the minimum is the shortest signed circular
    /// distance. (0 - 0 = 0 so zero-distance is handled cleanly.)
    fn phase_circular_ulps(a: Phase, b: Phase) -> u32 {
        let diff = b.0.wrapping_sub(a.0);
        diff.min(0u32.wrapping_sub(diff))
    }

    /// A full beat at 120 BPM = 500 ms = 24 000 samples at 48 kHz.
    /// Link's session returns beat-phase that wraps once per beat, so
    /// querying sample 0 and sample 24 000 should yield phases that
    /// are near-equal after one wrap.
    #[test]
    fn phase_wraps_once_per_beat_at_120bpm_48k() {
        let mut c = LinkClock::new(Tempo::from_bpm_integer(120), zero_anchor_48k());
        c.set_anchor(HostTimeAnchor {
            host_origin_micros: c.clock_micros(),
            sample_rate: sr_48k(),
        });

        let p0 = c.phase_at_sample(0);
        let p24k = c.phase_at_sample(24_000);
        // One full cycle later → same beat-phase modulo the full 2^32
        // range. Allow a generous threshold because Link's session
        // can drift microseconds between the two captures (fresh
        // `capture_audio_session_state` each call) — 2^22 ≈ 0.1% of
        // the cycle is more than enough headroom.
        let ulp = phase_circular_ulps(p0, p24k);
        assert!(
            ulp < (1u32 << 22),
            "p0={}, p24k={}, circular ULPs = {}",
            p0.0,
            p24k.0,
            ulp
        );
    }

    /// `f64_phase_to_phase` maps `1.0` (and anything ≥ 1.0 after
    /// `rem_euclid`) back to `0`, so the raw integer `u32::MAX`
    /// cannot appear at the boundary — but be paranoid: sweep a
    /// wide range of `n` and BPMs and assert every returned phase is
    /// strictly less than `u32::MAX`.
    #[test]
    fn phase_never_returns_exact_u32_max() {
        for bpm in [30_u32, 120, 200, 999] {
            let mut c = LinkClock::new(Tempo::from_bpm_integer(bpm), zero_anchor_48k());
            c.set_anchor(HostTimeAnchor {
                host_origin_micros: c.clock_micros(),
                sample_rate: sr_48k(),
            });
            for n in (0u64..100_000).step_by(37) {
                let p = c.phase_at_sample(n);
                assert!(p.0 < u32::MAX, "saw u32::MAX at n={n}, bpm={bpm}");
            }
        }
    }

    // `set_anchor` shifts the sample-index → host-time mapping. Same
    // `LinkClock` (same Link session), two anchors differing by
    // `Δ_us`, queries offset by `Δ_us × sr / 10⁶` samples should hit
    // the same host-time and therefore return near-equal phases.
    //
    // Using one clock is load-bearing: two independent `AblLink`
    // instances share the platform monotonic clock but have
    // independent session states (tempo, beat origin), so their
    // `phase_at_time(t, 1.0)` values diverge.
    proptest::proptest! {
        /// The per-stride phase delta must match the tempo-driven
        /// expectation (stride samples at `sample_rate` → `stride × bpm
        /// / 60 / sample_rate` cycles), regardless of the anchor's
        /// host-origin. Anchor shifts change absolute phase but not
        /// the rate of phase advance.
        ///
        /// BPM is sampled as µBPM across Link's `[20, 999]` BPM range
        /// so the test doesn't fix the tempo at 120 — the invariant's
        /// "tempo-driven" claim has to hold across Link's range.
        #[test]
        fn phase_delta_matches_tempo(
            host_origin in -1_000_000i64..=1_000_000i64,
            stride in 100u64..=10_000u64,
            bpm_mbpm in 20_000_000u32..=999_000_000u32,
        ) {
            // `bpm_mbpm` is already µBPM — construct Tempo directly, no
            // float round-trip.
            let tempo = Tempo(bpm_mbpm);
            let mut c = LinkClock::new(
                tempo,
                HostTimeAnchor { host_origin_micros: host_origin, sample_rate: sr_48k() },
            );
            let p0 = c.phase_at_sample(0);
            let p1 = c.phase_at_sample(stride);
            let diff = p1.0.wrapping_sub(p0.0);

            // Expected Q0.32 ULPs per stride:
            //   cycles_per_stride = stride · (µBPM / 10⁶) / 60 / sample_rate
            //                     = stride · µBPM / (60 · 10⁶ · sample_rate)
            //   ULPs = cycles · 2^32 rounded down.
            // Keep the whole numerator in u128 (stride · µBPM · 2³² is
            // at most ~10⁴ · 10⁹ · 4.3·10⁹ ≈ 4.3·10²², fits in 128 bits).
            let expected = (u128::from(stride)
                * u128::from(bpm_mbpm)
                * (1u128 << 32)
                / (60u128 * 1_000_000u128 * 48_000u128)) as u32;

            let err = phase_circular_ulps(Phase(diff), Phase(expected));
            // Generous tolerance: Link's session state can drift a
            // few microseconds between the two captures; 2²² ULPs
            // ≈ 0.1% of a full cycle is well above that noise floor.
            proptest::prop_assert!(
                err < (1u32 << 22),
                "delta != expected: diff={}, expected={}, err_ulp={}, bpm_µ={}",
                diff, expected, err, bpm_mbpm
            );
        }
    }

    // ── Snap-offset ────────────────────────────────────────────────

    /// `quantum_snap_nonneg`: the snap offset for any positive
    /// quantum is always ≥ `Micro::ZERO` — snap moves forward to the
    /// next boundary, never backward.
    #[test]
    fn quantum_snap_nonneg_at_representative_quanta() {
        let mut c = LinkClock::new(Tempo::from_bpm_integer(120), zero_anchor_48k());
        for bars in [1u32, 2, 4, 7, 16] {
            let q = Quantum::from_bars(bars);
            let delta = c.snap_offset_micro(q);
            assert!(
                delta.0 >= 0,
                "snap_offset_micro({:?}) returned negative: {:?}",
                q,
                delta
            );
        }
    }

    /// `snap_offset_micro` on `Quantum::ZERO` (or negative) returns
    /// `Micro::ZERO` — the guard against `q_f64 <= 0.0`. Divide-by-zero
    /// inside the math would otherwise panic in debug / produce inf
    /// in release.
    #[test]
    fn snap_offset_zero_quantum_is_zero() {
        let mut c = LinkClock::new(Tempo::from_bpm_integer(120), zero_anchor_48k());
        assert_eq!(c.snap_offset_micro(Quantum::ZERO), Micro::ZERO);
    }

    /// Snap offset bounds: for `Quantum::from_bars(n)` — which is
    /// `n` microbeats-million = `n` beats — at BPM B, the time
    /// until the next n-beat boundary can never exceed the time of
    /// one full n-beat span — i.e. `delta < n × 60 / B × 10⁶` µs.
    /// At 120 BPM, one beat = 500 000 µs, so an n-beat quantum
    /// spans `n × 500 000` µs. Verified on a representative set
    /// rather than as a full proptest because the Link session's
    /// beat-origin is machine-local and varies between runs; the
    /// bound holds unconditionally.
    #[test]
    fn snap_offset_bounded_by_one_quantum_span() {
        let mut c = LinkClock::new(Tempo::from_bpm_integer(120), zero_anchor_48k());
        for beats in [1u32, 4, 16] {
            let q = Quantum::from_bars(beats);
            let delta = c.snap_offset_micro(q);
            // At 120 BPM, one beat is 500_000 µs, so an n-beat
            // quantum spans `n * 500_000` µs.
            let one_quantum_span_us: i64 = (beats as i64) * 500_000;
            // Plus one µs of slack for floor/ceil rounding on `ceil()`.
            let bound = one_quantum_span_us + 1;
            assert!(
                delta.0 <= bound,
                "snap {:?} at {} beats exceeds one-quantum span bound {}",
                delta,
                beats,
                bound
            );
        }
    }

    #[test]
    fn set_anchor_shifts_the_sample_mapping() {
        let mut c = LinkClock::new(Tempo::from_bpm_integer(120), zero_anchor_48k());
        let now = c.clock_micros();

        // Query phase at n = 48 000 with anchor at `now` — this
        // probes host-time (now + 48 000 * 10⁶ / 48 000) = now + 10⁶.
        c.set_anchor(HostTimeAnchor {
            host_origin_micros: now,
            sample_rate: sr_48k(),
        });
        let a = c.phase_at_sample(48_000);

        // Shift anchor forward by 10⁶ µs (one second) and query at
        // n = 0 — this probes host-time now + 10⁶, the same moment.
        c.set_anchor(HostTimeAnchor {
            host_origin_micros: now + 1_000_000,
            sample_rate: sr_48k(),
        });
        let b = c.phase_at_sample(0);

        let ulp = phase_circular_ulps(a, b);
        assert!(
            ulp < (1u32 << 18),
            "expected near-equal phases at matched host times: a={}, b={}, diff_ulp={}",
            a.0,
            b.0,
            ulp
        );
    }
}
