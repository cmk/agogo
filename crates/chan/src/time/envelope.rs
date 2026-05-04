//! Opening / closing / Hermite-smoothstep envelopes on `Tick` ranges.
//!
//! Port of `opening`, `closing`, `sCurveEnv` from the Haskell. Each
//! envelope maps `(t, n) ↦ u8` where `n` is the envelope length and
//! `t` is the position; output saturates at the endpoints.
//!
//! **Plan argument-order note.** The plan specifies `fn opening(t, n)`
//! — `t` first, `n` second — which is the reverse of Haskell's
//! `opening n t`. We follow the plan (Rust convention: position-then-
//! length reads naturally as `opening(at: t, over: n)`).
//!
//! **Rounding.** All arithmetic is integer: the `linear_u8` and
//! `smoothstep_u8` primitives below carry bit-exact rounding semantics
//! (ties round half-away-from-zero via the shared Q0.24 / u128 path)
//! without any floating-point. Edge cases match the Haskell: `n == 0`
//! collapses to the envelope's fully-open value (255 for `opening` /
//! `s_curve`, 0 for `closing`).

use crate::time::tick::Tick;
use connections::fixed::u8::U128U008;

// ────────────────────────────────────────────────────────────────────
// Integer ramp + smoothstep primitives.
//
// Both take integer inputs `t <= n` (with `n > 0`) and return u8.
// Bit-exact, no float. Moved here from `crate::fxp` (Plan
// 2026-04-28-03 T5): these are envelope curves, not arithmetic
// primitives — they belong with the envelope module.
// ────────────────────────────────────────────────────────────────────

/// Linear ramp `t/n` rendered as `u8`. Endpoints: `linear_u8(0, n) = 0`,
/// `linear_u8(n, n) = 255`. Degenerate `n = 0` returns 255 (treat
/// "no span" as fully open — matches `opening(0, 0) = 255`).
pub fn linear_u8(t: u64, n: u64) -> u8 {
    if n == 0 {
        return 255;
    }
    if t >= n {
        return 255;
    }
    // round-nearest: (t * 255 + n/2) / n. Widen to u128 so `t * 255`
    // can't overflow at the top of `Tick`'s u64 range.
    let num = u128::from(t) * 255 + u128::from(n) / 2;
    U128U008.ceil(num / u128::from(n))
}

/// Hermite smoothstep `3x² − 2x³` rendered as `u8` with
/// `x = t/n ∈ [0, 1]`. Endpoints: `smoothstep_u8(0, n) = 0`,
/// `smoothstep_u8(n, n) = 255`. Degenerate `n = 0` returns 255.
pub fn smoothstep_u8(t: u64, n: u64) -> u8 {
    if n == 0 {
        return 255;
    }
    if t >= n {
        return 255;
    }
    if t == 0 {
        return 0;
    }
    // x as Q0.24 (guaranteed < 1 here because t < n).
    //   x = t · 2^24 / n
    // y = 3x² − 2x³, with x in Q0.24:
    //   x² in Q0.48, x³ in Q0.72. Work in u128.
    let x: u128 = (u128::from(t) << 24) / u128::from(n);
    let x2: u128 = x * x; // Q0.48
    let x3: u128 = x2 * x; // Q0.72
    // y = 3·x² − 2·x³, both terms scaled to Q0.48 then combined.
    //   3·x² is already Q0.48.
    //   2·x³ in Q0.72 becomes (2·x³) >> 24 in Q0.48 (with rounding).
    let term_a = 3u128 * x2;
    let rounding = 1u128 << 23;
    let term_b = (2u128 * x3 + rounding) >> 24;
    let y: u128 = term_a - term_b; // Q0.48, always ≤ 2^48
    // scale to u8: (y * 255 + 2^47) >> 48
    let scaled = (y * 255u128 + (1u128 << 47)) >> 48;
    U128U008.ceil(scaled)
}

// ────────────────────────────────────────────────────────────────────
// Envelopes proper.
// ────────────────────────────────────────────────────────────────────

/// Linear opening envelope: 0 at `t = 0`, 255 at `t = n`.
pub fn opening(t: Tick, n: Tick) -> u8 {
    linear_u8(t.0, n.0)
}

/// Linear closing envelope: 255 at `t = 0`, 0 at `t = n`.
pub fn closing(t: Tick, n: Tick) -> u8 {
    if n.0 == 0 {
        return 0;
    }
    255 - linear_u8(t.0.min(n.0), n.0)
}

/// Hermite smoothstep envelope: `3x² - 2x³` scaled to `0..=255`.
/// Monotonically non-decreasing on `[0, n]`.
pub fn s_curve(t: Tick, n: Tick) -> u8 {
    smoothstep_u8(t.0, n.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    // ── Spot checks ──────────────────────────────────────────────

    #[test]
    fn opening_endpoints() {
        assert_eq!(opening(Tick(0), Tick(100)), 0);
        assert_eq!(opening(Tick(100), Tick(100)), 255);
    }

    #[test]
    fn closing_endpoints() {
        assert_eq!(closing(Tick(0), Tick(100)), 255);
        assert_eq!(closing(Tick(100), Tick(100)), 0);
    }

    #[test]
    fn s_curve_endpoints() {
        assert_eq!(s_curve(Tick(0), Tick(100)), 0);
        assert_eq!(s_curve(Tick(100), Tick(100)), 255);
    }

    #[test]
    fn opening_midpoint_128() {
        // 5/10 * 255 = 127.5 → round to 128 (half-away-from-zero).
        assert_eq!(opening(Tick(5), Tick(10)), 128);
    }

    #[test]
    fn s_curve_midpoint_128() {
        // x = 0.5: 3*0.25 - 2*0.125 = 0.75 - 0.25 = 0.5. 0.5 * 255 = 127.5 → 128.
        assert_eq!(s_curve(Tick(5), Tick(10)), 128);
    }

    #[test]
    fn opening_saturates_past_n() {
        assert_eq!(opening(Tick(200), Tick(100)), 255);
    }

    #[test]
    fn closing_saturates_past_n() {
        assert_eq!(closing(Tick(200), Tick(100)), 0);
    }

    #[test]
    fn n_zero_edge_cases() {
        // n=0 is a degenerate envelope; saturate to the fully-open value.
        assert_eq!(opening(Tick(0), Tick(0)), 255);
        assert_eq!(closing(Tick(0), Tick(0)), 0);
        assert_eq!(s_curve(Tick(0), Tick(0)), 255);
    }

    // ── Shared proptest strategy ─────────────────────────────────

    fn arb_env_range() -> impl Strategy<Value = (Tick, Tick)> {
        // Equal-sized ranges keep interior (`t < n`), boundary (`t == n`),
        // and saturation (`t > n`) each well-represented, roughly 50/ε/50.
        (1u64..=10_000, 0u64..=10_000).prop_map(|(n, t)| (Tick(t), Tick(n)))
    }

    // ── Property tests ───────────────────────────────────────────

    proptest! {
        /// Plan property `envelope_endpoint`: endpoints always saturate
        /// correctly. Covers all three envelopes in one sweep.
        #[test]
        fn envelope_endpoint(n in 1u64..=10_000) {
            prop_assert_eq!(opening(Tick(0), Tick(n)), 0);
            prop_assert_eq!(opening(Tick(n), Tick(n)), 255);
            prop_assert_eq!(closing(Tick(0), Tick(n)), 255);
            prop_assert_eq!(closing(Tick(n), Tick(n)), 0);
            prop_assert_eq!(s_curve(Tick(0), Tick(n)), 0);
            prop_assert_eq!(s_curve(Tick(n), Tick(n)), 255);
        }

        /// Plan property `s_curve_monotone`: strictly non-decreasing
        /// (i.e. `t1 ≤ t2 ⟹ s_curve(t1) ≤ s_curve(t2)`) on `[0, n]`.
        /// With u8 output there can be flat plateaus near the curve's
        /// inflection points; the property is non-strict.
        #[test]
        fn s_curve_monotone(
            (t1, n) in arb_env_range(),
            (t2, _) in arb_env_range(),
        ) {
            if t1.0 <= t2.0 {
                prop_assert!(s_curve(t1, n) <= s_curve(t2, n));
            }
        }

        /// `opening` is non-decreasing.
        #[test]
        fn opening_monotone(
            (t1, n) in arb_env_range(),
            (t2, _) in arb_env_range(),
        ) {
            if t1.0 <= t2.0 {
                prop_assert!(opening(t1, n) <= opening(t2, n));
            }
        }

        /// `closing` is non-increasing.
        #[test]
        fn closing_monotone(
            (t1, n) in arb_env_range(),
            (t2, _) in arb_env_range(),
        ) {
            if t1.0 <= t2.0 {
                prop_assert!(closing(t1, n) >= closing(t2, n));
            }
        }

        /// All three envelopes clamp within `0..=255` (tautological
        /// for `u8`, so the real check is that no panic escapes from
        /// the float arithmetic: the `clamp + as u8` should absorb
        /// any FP edge case).
        #[test]
        fn envelopes_never_panic((t, n) in arb_env_range()) {
            let _ = opening(t, n);
            let _ = closing(t, n);
            let _ = s_curve(t, n);
        }

        /// `s_curve` hits exact endpoints on both sides. (`u8` bounds
        /// the range automatically.)
        #[test]
        fn s_curve_bounded((t, n) in arb_env_range()) {
            let v = s_curve(t, n);
            if t.0 == 0 { prop_assert_eq!(v, 0); }
            if t.0 >= n.0 { prop_assert_eq!(v, 255); }
        }
    }

    // ── linear_u8 / smoothstep_u8 primitives (moved from fxp T5) ─────

    #[test]
    fn smoothstep_endpoints_explicit() {
        assert_eq!(smoothstep_u8(0, 10), 0);
        assert_eq!(smoothstep_u8(10, 10), 255);
        assert_eq!(smoothstep_u8(5, 10), 128); // Hermite midpoint
    }

    #[test]
    fn linear_endpoints_explicit() {
        assert_eq!(linear_u8(0, 10), 0);
        assert_eq!(linear_u8(10, 10), 255);
        assert_eq!(linear_u8(5, 10), 128);
    }

    #[test]
    fn degenerate_n_zero() {
        assert_eq!(smoothstep_u8(0, 0), 255);
        assert_eq!(linear_u8(0, 0), 255);
    }

    proptest! {
        #[test]
        fn smoothstep_u8_endpoints(n in 1u64..u64::MAX) {
            prop_assert_eq!(smoothstep_u8(0, n), 0);
            prop_assert_eq!(smoothstep_u8(n, n), 255);
        }

        #[test]
        fn smoothstep_u8_monotone(t1 in 0u64..=1_000_000, n in 1u64..=1_000_000) {
            let t2 = t1.saturating_add(1);
            let (t1, t2) = if t1 <= n && t2 <= n { (t1, t2) } else { (0, 1.min(n)) };
            prop_assert!(smoothstep_u8(t1, n) <= smoothstep_u8(t2, n));
        }

        #[test]
        fn smoothstep_u8_symmetric(t in 0u64..=10_000, n_extra in 0u64..=10_000) {
            let n = t + n_extra;
            if n == 0 { return Ok(()); }
            let a = u32::from(smoothstep_u8(t, n));
            let b = u32::from(smoothstep_u8(n - t, n));
            // Hermite is symmetric around x=0.5, so s(t) + s(n-t) = 255,
            // modulo ±1 ULP rounding.
            let sum = a + b;
            prop_assert!(
                (254..=256).contains(&sum),
                "sum={} for t={} n={}",
                sum,
                t,
                n
            );
        }

        #[test]
        fn linear_u8_endpoints(n in 1u64..u64::MAX) {
            prop_assert_eq!(linear_u8(0, n), 0);
            prop_assert_eq!(linear_u8(n, n), 255);
        }

        #[test]
        fn linear_u8_monotone(t1 in 0u64..=1_000_000, n in 1u64..=1_000_000) {
            let t2 = t1.saturating_add(1);
            let (t1, t2) = if t1 <= n && t2 <= n { (t1, t2) } else { (0, 1.min(n)) };
            prop_assert!(linear_u8(t1, n) <= linear_u8(t2, n));
        }
    }
}
