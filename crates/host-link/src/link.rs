//! `LinkClock` — the agogo-side handle on an Ableton Link session.
//!
//! Implements `agogo_core::sync::PhaseSourceImpl` over rusty_link: the
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

use agogo_core::fxp::{Phase, f64_phase_to_phase};
use agogo_core::sync::PhaseSourceImpl;
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
    /// Sample rate used as the denominator in `n → host-µs`. `u32`
    /// because Link's time domain is microseconds; the rate is just
    /// a scalar denominator here, not a typed sample-rate.
    pub sample_rate: u32,
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
    pub fn new(initial_bpm: f64, anchor: HostTimeAnchor) -> Self {
        Self {
            link: AblLink::new(initial_bpm),
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
    /// RT-safety: the setter writes two scalar fields; safe to call
    /// from any thread. Plan 09 will promote the anchor to an
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
        let offset = (i128::from(n) * 1_000_000) / i128::from(self.anchor.sample_rate);
        let host_micros = (i128::from(self.anchor.host_origin_micros) + offset)
            .clamp(i128::from(i64::MIN), i128::from(i64::MAX))
            as i64;
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
    fn zero_anchor_48k() -> HostTimeAnchor {
        HostTimeAnchor {
            host_origin_micros: 0,
            sample_rate: 48_000,
        }
    }

    #[test]
    fn new_does_not_panic_across_bpm_range() {
        // Construct at representative BPMs; drop without enabling so
        // peer discovery / multicast join never starts. (Link's C++
        // side still opens a UDP socket on construction; we just
        // don't announce presence to the LAN.)
        for bpm in [60.0, 90.0, 120.0, 137.0, 200.0] {
            let _c = LinkClock::new(bpm, zero_anchor_48k());
        }
    }

    #[test]
    fn tempo_reads_back_initial_bpm() {
        // Link internally clamps to [20, 999] — 137.0 passes through.
        let mut c = LinkClock::new(137.0, zero_anchor_48k());
        let t = c.tempo();
        assert!(
            (t - 137.0).abs() < 1e-9,
            "tempo {t} differs from initial 137.0"
        );
    }

    #[test]
    fn num_peers_zero_before_enable() {
        // Disabled session has no discovery running → no peers.
        let c = LinkClock::new(120.0, zero_anchor_48k());
        assert_eq!(c.num_peers(), 0);
    }

    #[test]
    fn is_enabled_tracks_enable_call() {
        // Touches the network: `enable(true)` opens Link's UDP
        // multicast listener. Fine on a dev box and GitHub-hosted
        // CI runners, but sandboxed / multicast-less environments
        // may fail. Plan 09 adds a `fixture_or_skip!`-style network
        // gate when the multicast-dependent integration tests land.
        let c = LinkClock::new(120.0, zero_anchor_48k());
        assert!(!c.is_enabled());
        c.enable(true);
        assert!(c.is_enabled());
        c.enable(false);
        assert!(!c.is_enabled());
    }

    #[test]
    fn feed_samples_is_noop_and_preserves_tempo() {
        let mut c = LinkClock::new(120.0, zero_anchor_48k());
        c.feed_samples(&[0.1, 0.2, 0.3], 0);
        // Tempo unchanged by audio input.
        assert!((c.tempo() - 120.0).abs() < 1e-9);
    }

    // ── Phase bridge ──────────────────────────────────────────────

    #[test]
    fn phase_at_sample_returns_valid_phase() {
        // Bridge must always return a valid Phase (< 2^32); no
        // NaN/inf can sneak through `f64_phase_to_phase`.
        let mut c = LinkClock::new(120.0, zero_anchor_48k());
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
        let mut c = LinkClock::new(120.0, zero_anchor_48k());
        c.set_anchor(HostTimeAnchor {
            host_origin_micros: c.clock_micros(),
            sample_rate: 48_000,
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
            p0.0, p24k.0, ulp
        );
    }

    /// `f64_phase_to_phase` maps `1.0` (and anything ≥ 1.0 after
    /// `rem_euclid`) back to `0`, so the raw integer `u32::MAX`
    /// cannot appear at the boundary — but be paranoid: sweep a
    /// wide range of `n` and BPMs and assert every returned phase is
    /// strictly less than `u32::MAX`.
    #[test]
    fn phase_never_returns_exact_u32_max() {
        for bpm in [30.0, 120.0, 200.0, 999.0] {
            let mut c = LinkClock::new(bpm, zero_anchor_48k());
            c.set_anchor(HostTimeAnchor {
                host_origin_micros: c.clock_micros(),
                sample_rate: 48_000,
            });
            for n in (0u64..100_000).step_by(37) {
                let p = c.phase_at_sample(n);
                assert!(p.0 < u32::MAX, "saw u32::MAX at n={n}, bpm={bpm}");
            }
        }
    }

    /// `set_anchor` shifts the sample-index → host-time mapping. Same
    /// `LinkClock` (same Link session), two anchors differing by
    /// `Δ_us`, queries offset by `Δ_us × sr / 10⁶` samples should hit
    /// the same host-time and therefore return near-equal phases.
    ///
    /// Using one clock is load-bearing: two independent `AblLink`
    /// instances share the platform monotonic clock but have
    /// independent session states (tempo, beat origin), so their
    /// `phase_at_time(t, 1.0)` values diverge.
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
            let bpm = f64::from(bpm_mbpm) / 1_000_000.0;
            let mut c = LinkClock::new(
                bpm,
                HostTimeAnchor { host_origin_micros: host_origin, sample_rate: 48_000 },
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

    #[test]
    fn set_anchor_shifts_the_sample_mapping() {
        let mut c = LinkClock::new(120.0, zero_anchor_48k());
        let now = c.clock_micros();

        // Query phase at n = 48 000 with anchor at `now` — this
        // probes host-time (now + 48 000 * 10⁶ / 48 000) = now + 10⁶.
        c.set_anchor(HostTimeAnchor {
            host_origin_micros: now,
            sample_rate: 48_000,
        });
        let a = c.phase_at_sample(48_000);

        // Shift anchor forward by 10⁶ µs (one second) and query at
        // n = 0 — this probes host-time now + 10⁶, the same moment.
        c.set_anchor(HostTimeAnchor {
            host_origin_micros: now + 1_000_000,
            sample_rate: 48_000,
        });
        let b = c.phase_at_sample(0);

        let ulp = phase_circular_ulps(a, b);
        assert!(
            ulp < (1u32 << 18),
            "expected near-equal phases at matched host times: a={}, b={}, diff_ulp={}",
            a.0, b.0, ulp
        );
    }
}
