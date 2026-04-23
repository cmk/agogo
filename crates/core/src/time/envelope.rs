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
//! **Rounding.** All arithmetic is integer: delegation to
//! `fxp::linear_u8` and `fxp::smoothstep_u8` keeps the old bit-exact
//! rounding semantics (ties round half-away-from-zero via the shared
//! Q0.24 / u128 path) without any floating-point. Edge cases match
//! the Haskell: `n == 0` collapses to the envelope's fully-open value
//! (255 for `opening` / `s_curve`, 0 for `closing`).

use crate::fxp;
use crate::time::tick::Tick;

/// Linear opening envelope: 0 at `t = 0`, 255 at `t = n`.
pub fn opening(t: Tick, n: Tick) -> u8 {
    fxp::linear_u8(t.0, n.0)
}

/// Linear closing envelope: 255 at `t = 0`, 0 at `t = n`.
pub fn closing(t: Tick, n: Tick) -> u8 {
    if n.0 == 0 {
        return 0;
    }
    255 - fxp::linear_u8(t.0.min(n.0), n.0)
}

/// Hermite smoothstep envelope: `3x² - 2x³` scaled to `0..=255`.
/// Monotonically non-decreasing on `[0, n]`.
pub fn s_curve(t: Tick, n: Tick) -> u8 {
    fxp::smoothstep_u8(t.0, n.0)
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
        (1u32..=10_000, 0u32..=10_000).prop_map(|(n, t)| (Tick(t), Tick(n)))
    }

    // ── Property tests ───────────────────────────────────────────

    proptest! {
        /// Plan property `envelope_endpoint`: endpoints always saturate
        /// correctly. Covers all three envelopes in one sweep.
        #[test]
        fn envelope_endpoint(n in 1u32..=10_000) {
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
}
