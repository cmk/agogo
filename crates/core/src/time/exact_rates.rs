//! Integer-exactness proptests for `SampleTickConn` at 960 PPQN / 48k
//! / 96k.
//!
//! At 960 PPQN the master tick is 1/3840 of a bar. The `Tick → Sample`
//! conversion `sample = tick · sr · 60 · 10⁶ / (bpm_µ · 960)` is
//! integer-exact when `sr · 60 · 10⁶ / 960 = sr / 16 · 10⁶` is
//! divisible by `bpm_µ`. For an integer BPM, `bpm_µ = bpm · 10⁶`, so
//! the divisor reduces to `sr / 16` — i.e. exact iff `bpm` divides
//! `sr / 16` (= 3000 at 48 kHz, 6000 at 96 kHz).
//!
//! v0.2 — Slot 03 of the version-0.2 plan (carried forward from
//! agogo.md §6's "BPM auto-snap" deferred). Three properties:
//!
//! - `stc_samples_per_tick_is_exact_at_48k` — no rounding inside
//!   `SampleTickConn::inner` when bpm divides 3000.
//! - `stc_samples_per_tick_is_exact_at_96k` — same at 96 kHz, divides
//!   6000.
//! - `stc_round_trip_identity_48k_96k` — strengthens the existing
//!   `sample_tick_round_trip` proptest from ceil-bound to equality:
//!   `floor(inner(tick)) == tick` AND `ceil(inner(tick)) == tick` for
//!   every tick on every integer-exact `(sr, bpm)` pair.

#[cfg(test)]
mod tests {
    use crate::fxp::Tempo;
    use crate::time::conn::SampleTickConn;
    use crate::time::tick::{PPQN, Tick};
    use proptest::prelude::*;

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
            t in 0u32..=10_000_000,
        ) {
            let stc = SampleTickConn::new(48_000, Tempo::from_bpm_integer(bpm), PPQN);
            assert_exact(&stc, Tick(t));
        }

        /// Plan property `stc_samples_per_tick_is_exact_at_96k`: same
        /// at 96 kHz, BPMs dividing 6000.
        #[test]
        fn stc_samples_per_tick_is_exact_at_96k(
            bpm in arb_exact_96k_bpm(),
            t in 0u32..=10_000_000,
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
            t in 0u32..=1_000_000,
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
