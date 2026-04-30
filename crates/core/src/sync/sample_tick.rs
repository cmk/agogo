//! Sample ↔ Tick bridge — runtime-parameterised on `(sr, bpm, ppqn)`.
//!
//! `connections::Conn<A, B>` uses bare `fn` pointers that cannot close
//! over runtime state, so the natural `Conn<Sample, Tick>` parameterised
//! on `(sr, bpm, ppqn)` is not expressible today. The pragmatic
//! workaround is a struct mirroring `Conn`'s `(ceil, inner, floor)`
//! shape with the same adjoint-law guarantees, all integer-valued.
//!
//! Moved from `crate::time::conn` (Plan 2026-04-28-03 T3) — `time/` is
//! supposed to be tempo-INDEPENDENT (per the module-level doc on
//! `crate::time`), and `SampleTickConn` reads `Tempo`. It belongs in
//! `sync` alongside the other tempo-coupled state.

use crate::conn::tempo::Tempo;
use crate::time::tick::Tick;

/// Sample ↔ Tick bridge parameterised by sample rate, tempo, and PPQN.
///
/// Mirrors `connections::Conn<Sample, Tick>`'s `(ceil, inner, floor)`
/// triple. The laws — round-trip on aligned inputs, monotonicity — are
/// verified by proptest. Not a real `Conn` because its conversion
/// depends on runtime `(sr, bpm, ppqn)` — would require a closure-
/// capturing variant upstream.
///
/// All arithmetic is integer (`Tempo` for tempo, `u128` intermediate).
/// No floating-point.
#[derive(Copy, Clone, Debug)]
pub struct SampleTickConn {
    sr: u32,
    bpm: Tempo,
    ppqn: u32,
}

impl SampleTickConn {
    /// # Panics
    ///
    /// Panics if `sr == 0`, `ppqn == 0`, or `bpm.0 == 0`. These are
    /// programming errors — every call site either ships fixed
    /// constants or validates at a CLI/config boundary.
    pub fn new(sr: u32, bpm: Tempo, ppqn: u32) -> Self {
        assert!(sr > 0, "sample rate must be positive");
        assert!(ppqn > 0, "ppqn must be positive");
        assert!(bpm.0 > 0, "bpm must be positive, got {:?}", bpm);
        Self { sr, bpm, ppqn }
    }

    pub fn sr(&self) -> u32 {
        self.sr
    }
    pub fn bpm(&self) -> Tempo {
        self.bpm
    }
    pub fn ppqn(&self) -> u32 {
        self.ppqn
    }

    /// Tick → Sample. Exact when `tick × sr × 60 × 10⁶` is divisible
    /// by `bpm_µ × ppqn` (e.g. 48 kHz / 120 BPM / 960 PPQN is exact);
    /// otherwise rounded to the nearest `u64` (half-away-from-zero —
    /// both quantities are non-negative). Saturates to `u64::MAX`
    /// for pathological inputs whose quotient exceeds `u64::MAX`
    /// (e.g. `Tick(u32::MAX)` with `bpm_µ = 1`, `ppqn = 1`); mirrors
    /// the `to_tick` clamp on the inverse direction. The wrap was
    /// flagged on PR #35; saturation closes it.
    pub fn inner(&self, tick: Tick) -> u64 {
        // sample = tick · sr · 60 · 10⁶ / (bpm_µ · ppqn)
        let num = u128::from(tick.0) * u128::from(self.sr) * 60 * 1_000_000;
        let denom = u128::from(self.bpm.0) * u128::from(self.ppqn);
        // Round to nearest: (num + denom/2) / denom. Half-up because
        // both num and denom are non-negative.
        let q = (num + denom / 2) / denom;
        q.min(u128::from(u64::MAX)) as u64
    }

    /// Sample → Tick, rounding down (latest tick at-or-before `sample`).
    pub fn floor(&self, sample: u64) -> Tick {
        // tick = sample · bpm_µ · ppqn / (sr · 60 · 10⁶)   (floor)
        let num = u128::from(sample) * u128::from(self.bpm.0) * u128::from(self.ppqn);
        let denom = u128::from(self.sr) * 60 * 1_000_000;
        Self::to_tick(num / denom)
    }

    /// Sample → Tick, rounding up (next tick at-or-after `sample`).
    pub fn ceil(&self, sample: u64) -> Tick {
        let num = u128::from(sample) * u128::from(self.bpm.0) * u128::from(self.ppqn);
        let denom = u128::from(self.sr) * 60 * 1_000_000;
        Self::to_tick(num.div_ceil(denom))
    }

    fn to_tick(x: u128) -> Tick {
        Tick(x.min(u128::from(u64::MAX)) as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conn::fixed::Pico;
    use proptest::prelude::*;

    fn mbpm(b: u32) -> Tempo {
        Tempo::from_bpm_integer(b)
    }

    #[test]
    fn sample_tick_inner_120bpm_48k_one_beat_at_960ppqn() {
        // 120 BPM, 960 PPQN, 48 kHz: one quarter note (tick 960) is
        // 0.5 s = 24 000 samples.
        let stc = SampleTickConn::new(48_000, mbpm(120), 960);
        assert_eq!(stc.inner(Tick(960)), 24_000);
    }

    #[test]
    fn sample_tick_floor_and_ceil_bracket_inner() {
        let stc = SampleTickConn::new(48_000, mbpm(120), 960);
        // 24 000 samples is exactly tick 960. Floor and ceil both
        // 960; one sample later (24 001) → floor still 960, ceil 961.
        assert_eq!(stc.floor(24_000), Tick(960));
        assert_eq!(stc.ceil(24_000), Tick(960));
        assert_eq!(stc.floor(24_001), Tick(960));
        assert_eq!(stc.ceil(24_001), Tick(961));
    }

    #[test]
    fn sample_tick_zero_is_zero() {
        let stc = SampleTickConn::new(48_000, mbpm(120), 960);
        assert_eq!(stc.inner(Tick(0)), 0);
        assert_eq!(stc.floor(0), Tick(0));
        assert_eq!(stc.ceil(0), Tick(0));
    }

    /// Sample-rate / BPM / PPQN combinations that keep integer
    /// samples-per-tick exact (`sr · 60 · 10⁶` divisible by
    /// `bpm_µ · ppqn`), needed for the round-trip property.
    fn arb_integer_stc() -> impl Strategy<Value = SampleTickConn> {
        prop_oneof![
            // 960 PPQN: at 48 kHz, BPMs that divide 60·48000/960 = 3000
            // are integer-exact. 60, 120, 240, 300 all qualify.
            Just(SampleTickConn::new(48_000, mbpm(60), 960)),
            Just(SampleTickConn::new(48_000, mbpm(120), 960)),
            Just(SampleTickConn::new(48_000, mbpm(240), 960)),
            Just(SampleTickConn::new(48_000, mbpm(300), 960)),
            // 96 kHz: 60·96000/960 = 6000.
            Just(SampleTickConn::new(96_000, mbpm(120), 960)),
            // 192 kHz: 60·192000/960 = 12000.
            Just(SampleTickConn::new(192_000, mbpm(120), 960)),
            // Lower-PPQN sanity (24 PPQ MIDI clock cadence).
            Just(SampleTickConn::new(48_000, mbpm(120), 24)),
        ]
    }

    proptest! {
        #[test]
        fn sample_tick_round_trip(
            stc in arb_integer_stc(),
            t in 0u64..=1_000_000,
        ) {
            let tick = Tick(t);
            let sample = stc.inner(tick);
            prop_assert_eq!(stc.floor(sample), tick);
        }

        #[test]
        fn sample_tick_monotonic(
            stc in arb_integer_stc(),
            s1 in 0u64..=10_000_000,
            s2 in 0u64..=10_000_000,
        ) {
            let (lo, hi) = if s1 <= s2 { (s1, s2) } else { (s2, s1) };
            prop_assert!(stc.floor(lo).0 <= stc.floor(hi).0);
        }

        #[test]
        fn sample_tick_ceil_ge_floor(
            stc in arb_integer_stc(),
            s in 0u64..=10_000_000,
        ) {
            prop_assert!(stc.floor(s).0 <= stc.ceil(s).0);
        }

        /// Pathological inputs deliberately fish for the u128→u64
        /// narrow inside `SampleTickConn::inner`. With `Tick(u32::MAX)`
        /// (≈ 4.3×10⁹), `sr = 192 kHz`, and tiny `bpm_µ` / `ppqn`,
        /// `num = tick × sr × 60×10⁶ ≈ 5×10²²`. Even at the largest
        /// `bpm_µ × ppqn = 800` denominator, the exact quotient
        /// (~6×10¹⁹) exceeds `u64::MAX` (~1.84×10¹⁹), so every
        /// sampled point hits the saturation branch — without the
        /// clamp the u128→u64 narrow would wrap modulo 2⁶⁴ and
        /// return garbage. Per CLAUDE.md the anti-pattern is bounding
        /// to *avoid* boundaries; here the bounds are set to *reach*
        /// the wrap, which is the legitimate inverse of the rule.
        ///
        /// The realistic-input region is covered by the
        /// `arb_integer_stc()`-driven proptests above. The Tick u64
        /// horizon (values past `u32::MAX`) is exercised by the
        /// `sample_tick_inner_saturates_at_u64_horizon` spot check.
        #[test]
        fn sample_tick_inner_saturates_on_overflow(
            tick in u64::from(u32::MAX / 2)..=u64::from(u32::MAX),
            bpm_u in 1u32..=100,
            ppqn in 1u32..=8,
        ) {
            // Highest-rate sr maximises the numerator and so the
            // wrap region.
            let sr = 192_000u32;
            let stc = SampleTickConn::new(sr, Tempo(bpm_u), ppqn);
            let result = stc.inner(Tick(tick));
            // Independent reference: the exact (un-narrowed)
            // quotient in u128, saturated to u64::MAX.
            let num = u128::from(tick) * u128::from(sr) * 60 * 1_000_000;
            let denom = u128::from(bpm_u) * u128::from(ppqn);
            let exact = (num + denom / 2) / denom;
            let expected = exact.min(u128::from(u64::MAX)) as u64;
            // For this input region, the exact quotient strictly
            // exceeds u64::MAX, so every case must saturate.
            prop_assert_eq!(result, u64::MAX);
            prop_assert_eq!(result, expected);
        }
    }

    /// Tick widening to u64 (Plan 2026-04-28-07 T1) opened a new
    /// horizon above `u32::MAX`. Spot-check that `inner` saturates
    /// cleanly there — without the clamp, `Tick(u64::MAX)` would
    /// wrap modulo 2⁶⁴ inside the u128→u64 narrow.
    #[test]
    fn sample_tick_inner_saturates_at_u64_horizon() {
        let stc = SampleTickConn::new(192_000, Tempo(1), 1);
        // Tick(u64::MAX) × 192_000 × 60 × 1e6 / 1 vastly exceeds
        // u64::MAX after the u128 multiply — saturation is mandatory.
        assert_eq!(stc.inner(Tick(u64::MAX)), u64::MAX);
        // Realistic-tempo case at the same tick — still saturates,
        // but the math is closer to the boundary so a regression
        // narrowing too aggressively would be caught here.
        let stc_120 = SampleTickConn::new(48_000, Tempo::from_bpm_integer(120), 960);
        assert_eq!(stc_120.inner(Tick(u64::MAX)), u64::MAX);
    }

    // ── Pico ↔ Sample agreement with SampleTickConn ──────────────

    #[test]
    fn sample_tick_and_pico_to_samples_agree_at_120bpm_48k() {
        // 120 BPM / ppq=960 / 48 kHz: each quarter note = 0.5 s =
        // 24 000 samples = 5×10¹¹ pico. At tick 960 (one beat):
        let stc = SampleTickConn::new(48_000, mbpm(120), 960);

        let via_tick: u64 = stc.inner(Tick(960));
        let pico_at_one_beat = Pico(500_000_000_000);
        let via_pico: i64 = crate::conn::boundary::pico_to_samples(pico_at_one_beat, 48_000)
            .expect("48 kHz is supported");
        assert_eq!(via_tick, 24_000);
        assert_eq!(via_pico, 24_000);
        assert_eq!(via_tick as i64, via_pico);

        // And at tick 1920 (two beats = 1 s = 48 000 samples = 10¹² pico):
        assert_eq!(stc.inner(Tick(1920)), 48_000);
        assert_eq!(
            crate::conn::boundary::pico_to_samples(Pico(1_000_000_000_000), 48_000),
            Some(48_000)
        );
    }

    #[test]
    fn pico_to_samples_rejects_unsupported_rate() {
        assert_eq!(
            crate::conn::boundary::pico_to_samples(Pico(0), 22_050),
            None
        );
        assert_eq!(crate::conn::boundary::pico_to_samples(Pico(0), 0), None);
        assert_eq!(
            crate::conn::boundary::pico_to_samples(Pico(1_000_000_000_000), 48_000),
            Some(48_000)
        );
    }

    // ──────────────────────────────────────────────────────────────
    // Integer-exactness at 960 PPQN / 48k / 96k for divisor BPMs.
    //
    // Absorbed from `time/exact_rates.rs` (Plan 2026-04-28-03 T3) —
    // that file was 200 lines of tests for `SampleTickConn` mis-located
    // under `time/`, with a misleading name suggesting audio-rate
    // integer-exactness in general. The content is specifically about
    // `SampleTickConn::inner` rounding-free at common DAW configs.
    // ──────────────────────────────────────────────────────────────
    mod exactness {
        use super::*;
        use crate::time::tick::PPQN;

        /// Divisors of `n`, sorted ascending. Used to enumerate the
        /// integer-exact BPMs at a given sample rate.
        fn divisors(n: u32) -> Vec<u32> {
            let mut out = Vec::new();
            let mut k = 1u32;
            while k * k <= n {
                if n % k == 0 {
                    out.push(k);
                    if k != n / k {
                        out.push(n / k);
                    }
                }
                k += 1;
            }
            out.sort_unstable();
            out
        }

        /// `Tempo::from_bpm_integer` caps at `u32::MAX / 1_000_000 = 4294`
        /// because tempo storage is `bpm × 10⁶`. We filter the strategies
        /// to that bound; in practice every musically-meaningful BPM (≤ a
        /// few hundred) falls well below it.
        const BPM_MAX: u32 = 4294;

        #[test]
        fn divisors_of_3000_includes_common_bpms() {
            let d = divisors(3000);
            assert!(d.contains(&60));
            assert!(d.contains(&120));
            assert!(d.contains(&125));
            assert!(d.contains(&250));
            // 137 doesn't divide 3000.
            assert!(!d.contains(&137));
            // 240 does NOT divide 3000 (3000 = 2³·3·5³, lacks 2⁴).
            assert!(!d.contains(&240));
        }

        /// Strategy yielding `(sr, bpm)` pairs where `tick_to_sample` is
        /// integer-exact at 960 PPQN. At 48 kHz that's BPMs dividing 3000;
        /// at 96 kHz, divisors of 6000. Filtered to `bpm ∈ (0, 4294]` to
        /// stay inside `Tempo`'s storage range.
        fn arb_exact_sr_bpm() -> impl Strategy<Value = (u32, u32)> {
            let d48: Vec<u32> = divisors(3000)
                .into_iter()
                .filter(|&b| b > 0 && b <= BPM_MAX)
                .collect();
            let d96: Vec<u32> = divisors(6000)
                .into_iter()
                .filter(|&b| b > 0 && b <= BPM_MAX)
                .collect();
            prop_oneof![
                prop::sample::select(d48).prop_map(|bpm| (48_000u32, bpm)),
                prop::sample::select(d96).prop_map(|bpm| (96_000u32, bpm)),
            ]
        }

        /// Strategy yielding only `(48_000, bpm)` exact pairs.
        fn arb_exact_48k_bpm() -> impl Strategy<Value = u32> {
            let d: Vec<u32> = divisors(3000)
                .into_iter()
                .filter(|&b| b > 0 && b <= BPM_MAX)
                .collect();
            prop::sample::select(d)
        }

        /// Strategy yielding only `(96_000, bpm)` exact pairs.
        fn arb_exact_96k_bpm() -> impl Strategy<Value = u32> {
            let d: Vec<u32> = divisors(6000)
                .into_iter()
                .filter(|&b| b > 0 && b <= BPM_MAX)
                .collect();
            prop::sample::select(d)
        }

        /// "Exact" at the SampleTickConn level means
        /// `inner(tick) * bpm_µ * ppqn == tick * sr * 60 · 10⁶` — i.e.
        /// the half-up rounding step in `SampleTickConn::inner` collapses
        /// to a no-op.
        fn assert_exact(stc: &SampleTickConn, tick: Tick) {
            let sample = stc.inner(tick);
            let lhs = u128::from(sample) * u128::from(stc.bpm().0) * u128::from(stc.ppqn());
            let rhs = u128::from(tick.0) * u128::from(stc.sr()) * 60 * 1_000_000;
            assert_eq!(
                lhs,
                rhs,
                "tick {} not exact at sr={} bpm_µ={} ppqn={} (sample={})",
                tick.0,
                stc.sr(),
                stc.bpm().0,
                stc.ppqn(),
                sample,
            );
        }

        proptest! {
            /// Plan property `stc_samples_per_tick_is_exact_at_48k`: at
            /// `sr=48_000, ppqn=960`, integer BPMs that divide 3000 yield
            /// rounding-free `tick → sample` for any tick.
            #[test]
            fn stc_samples_per_tick_is_exact_at_48k(
                bpm in arb_exact_48k_bpm(),
                t in 0u64..=10_000_000,
            ) {
                let stc = SampleTickConn::new(48_000, Tempo::from_bpm_integer(bpm), PPQN);
                assert_exact(&stc, Tick(t));
            }

            /// Plan property `stc_samples_per_tick_is_exact_at_96k`: same
            /// at 96 kHz, BPMs dividing 6000.
            #[test]
            fn stc_samples_per_tick_is_exact_at_96k(
                bpm in arb_exact_96k_bpm(),
                t in 0u64..=10_000_000,
            ) {
                let stc = SampleTickConn::new(96_000, Tempo::from_bpm_integer(bpm), PPQN);
                assert_exact(&stc, Tick(t));
            }

            /// Plan property `stc_round_trip_identity_48k_96k`: at any
            /// integer-exact `(sr, bpm)` pair, the `Tick → Sample → Tick`
            /// round trip is the identity (both `floor` and `ceil` agree
            /// with `inner`'s exact result on every tick).
            #[test]
            fn stc_round_trip_identity_48k_96k(
                (sr, bpm) in arb_exact_sr_bpm(),
                t in 0u64..=1_000_000,
            ) {
                let stc = SampleTickConn::new(sr, Tempo::from_bpm_integer(bpm), PPQN);
                let tick = Tick(t);
                let sample = stc.inner(tick);
                prop_assert_eq!(stc.floor(sample), tick);
                prop_assert_eq!(stc.ceil(sample), tick);
            }
        }

        // ── Spot checks ──────────────────────────────────────────────

        #[test]
        fn exact_at_120bpm_48k_960ppqn() {
            // 120 BPM × 960 PPQN at 48 kHz: tick 1 = 25 samples.
            let stc = SampleTickConn::new(48_000, Tempo::from_bpm_integer(120), PPQN);
            assert_eq!(stc.inner(Tick(1)), 25);
            assert_exact(&stc, Tick(1));
            assert_exact(&stc, Tick(960));
            assert_exact(&stc, Tick(3840));
        }

        #[test]
        fn exact_at_125bpm_48k_960ppqn() {
            // 125 divides 3000 → exact at 48k. 1 tick = (48000·60·10⁶) /
            // (125·10⁶·960) = 24 samples.
            let stc = SampleTickConn::new(48_000, Tempo::from_bpm_integer(125), PPQN);
            assert_eq!(stc.inner(Tick(1)), 24);
            assert_exact(&stc, Tick(1));
        }

        #[test]
        fn exact_at_60bpm_96k_960ppqn() {
            // 60 divides 6000. 1 tick at 96k / 60 BPM = 100 samples.
            let stc = SampleTickConn::new(96_000, Tempo::from_bpm_integer(60), PPQN);
            assert_eq!(stc.inner(Tick(1)), 100);
            assert_exact(&stc, Tick(1));
        }

        /// Sanity: a non-divisor BPM is *not* exact — the `assert_exact`
        /// helper must be discriminating, not vacuously true.
        #[test]
        fn non_divisor_bpm_is_not_exact() {
            // 137 doesn't divide 3000. tick=1 → 48000·60·10⁶ / (137·10⁶·960)
            // = 21.897… samples; rounded to 22 → not exact.
            let stc = SampleTickConn::new(48_000, Tempo::from_bpm_integer(137), PPQN);
            let sample = stc.inner(Tick(1));
            let lhs = u128::from(sample) * u128::from(stc.bpm().0) * u128::from(stc.ppqn());
            let rhs = u128::from(1u32) * u128::from(stc.sr()) * 60 * 1_000_000;
            assert_ne!(lhs, rhs, "137 BPM should not be integer-exact at 48k/960");
        }
    }
}
