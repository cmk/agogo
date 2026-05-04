//! Proptest strategies for the `conn`-tier types and connections.
//!
//! Consolidated by Plan 2026-04-29-01 T4 from the per-module
//! `arb.rs` files (`fixed/arb.rs`, `sample/arb.rs`, `tempo/arb.rs`,
//! plus the FD-fixed strategies that lived in `time/arb.rs` before
//! the move). Same shape as the upstream
//! `Test/Data/Connection/{Float,Int,…}.hs` test layout in the
//! Haskell library — strategies grouped per top-level module rather
//! than per type.
//!
//! Gated on `cfg(any(test, feature = "testkit"))` so downstream
//! crates can pull these strategies into their own proptest blocks
//! without forcing the rest of the workspace to compile proptest.

use connections::extended::Extended;
use proptest::prelude::*;

use crate::conn::fixed::{FD00, FD01, FD02, FD03, FD06, FD09, FD12, HasResolution, Pico};
use crate::conn::tempo::Tempo;

// ── Tempo ─────────────────────────────────────────────────────────

/// BPM strategy as `Tempo` (BPM × 10⁶). Biased toward common
/// musical tempos with some boundary spice.
pub fn arb_bpm() -> impl Strategy<Value = Tempo> {
    prop_oneof![
        1 => Just(Tempo::from_bpm_integer(60)),
        1 => Just(Tempo::from_bpm_integer(120)),
        1 => Just(Tempo::from_bpm_integer(200)),
        5 => (60_000_000u32..200_000_000).prop_map(Tempo),
        1 => (30_000_000u32..400_000_000).prop_map(Tempo),
    ]
}

// ── Sample rate ───────────────────────────────────────────────────

/// Sample rate strategy: standard audio rates only. (`u32` so it can
/// be used by callers that pick a rate type at the callsite; the
/// typed variants `S044` / `S048` / … expose the same values via
/// [`SampleRate::HZ`](crate::conn::sample::SampleRate::HZ).)
pub fn arb_sample_rate() -> impl Strategy<Value = u32> {
    prop_oneof![
        Just(44_100u32),
        Just(48_000u32),
        Just(96_000u32),
        Just(192_000u32),
    ]
}

// ── Pico (jitter σ) ───────────────────────────────────────────────

/// Jitter σ as [`Pico`]. Heavy bias toward small values so the PLL
/// convergence properties usually fire on inputs they can lock to.
pub fn arb_jitter_sigma() -> impl Strategy<Value = Pico> {
    prop_oneof![
        1 => Just(Pico(0)),
        5 => (0i64..50_000_000).prop_map(Pico),          // 0..50 µs in ps
        2 => (50_000_000i64..200_000_000).prop_map(Pico),
        1 => (200_000_000i64..500_000_000).prop_map(Pico),
    ]
}

// ── Fixed-point ladder (FD12..FD00) strategies ───────────────────
//
// For each (Fine, Coarse) pair with ratio PREC, the `inner` call
// computes `coarse * PREC` which must fit i64. Strategies clamp the
// coarse-side input to `|x| < i64::MAX / PREC` to avoid overflow
// inside the connection itself. The fine-side input is bounded by
// i64 range naturally.

/// Coarse-side i64 strategy for Fine→Coarse with `inner` ratio
/// `PREC`. Clamped to `|x| ≤ i64::MAX / prec` so `c · prec` fits i64.
pub fn fixed_coarse(prec: i64) -> impl Strategy<Value = i64> {
    let limit = i64::MAX / prec;
    prop_oneof![
        1 => Just(0_i64),
        1 => Just(1_i64),
        1 => Just(-1_i64),
        1 => Just(limit),
        1 => Just(-limit),
        5 => -limit..=limit,
    ]
}

/// Fine-side i64 strategy for Fine→Coarse with `inner` ratio `PREC`.
/// Full i64 range with explicit boundary bias around `±prec` and
/// `i64::{MIN, MAX}`. Use for properties that don't round-trip
/// through `inner` (adjoint, monotone, kernel).
pub fn fixed_fine(prec: i64) -> impl Strategy<Value = i64> {
    prop_oneof![
        1 => Just(0_i64),
        1 => Just(prec),
        1 => Just(-prec),
        1 => Just(prec - 1),
        1 => Just(-(prec - 1)),
        1 => Just(prec + 1),
        1 => Just(-(prec + 1)),
        1 => Just(i64::MAX),
        1 => Just(i64::MIN + 1), // i64::MIN causes overflow under negation in some checks
        5 => any::<i64>(),
    ]
}

/// Fine-side strategy restricted to values whose `inner(ceil(_))`
/// round-trip does not overflow.
///
/// `inner(c) = c * PREC`, so the safe Fine range is
/// `|fine| ≤ (i64::MAX / PREC) * PREC` — every Fine value that
/// `ceil` can map without pushing the resulting Coarse past
/// `i64::MAX / PREC`. Use for properties that round-trip through
/// `inner` (closure, idempotent).
pub fn fixed_safe_fine(prec: i64) -> impl Strategy<Value = i64> {
    let limit = (i64::MAX / prec) * prec;
    prop_oneof![
        1 => Just(0_i64),
        1 => Just(prec),
        1 => Just(-prec),
        1 => Just(prec - 1),
        1 => Just(-(prec - 1)),
        1 => Just(prec + 1),
        1 => Just(-(prec + 1)),
        1 => Just(limit),
        1 => Just(-limit),
        5 => -limit..=limit,
    ]
}

// ── Extended<FDxx> strategies ────────────────────────────────────
//
// `any::<f64>()` shrinks bit-by-bit through the mantissa and
// dominates runtime without finding structural bugs; bounded ranges
// plus explicit boundaries give wide enough adjoint-law coverage.

/// `Extended<FD00>` over `NegInf`, `PosInf`, and finite FD00 (1 s)
/// values across the full `i64` backing range. FD00::PREC is 1, so
/// `inner(c) = c · 1` doesn't overflow on any i64.
pub fn extended_fd00() -> impl Strategy<Value = Extended<FD00>> {
    prop_oneof![
        1 => Just(Extended::NegInf),
        1 => Just(Extended::PosInf),
        1 => Just(Extended::Finite(FD00(0))),
        1 => Just(Extended::Finite(FD00(i64::MAX))),
        1 => Just(Extended::Finite(FD00(i64::MIN))),
        8 => any::<i64>().prop_map(|x| Extended::Finite(FD00(x))),
    ]
}

/// `Extended<FD01>` over `NegInf`, `PosInf`, and finite FD01 (100 ms)
/// values bounded by `i64::MAX / FD01::PREC` (plus i64-edge `Just`s).
pub fn extended_fd01() -> impl Strategy<Value = Extended<FD01>> {
    let limit = i64::MAX / FD01::PREC;
    prop_oneof![
        1 => Just(Extended::NegInf),
        1 => Just(Extended::PosInf),
        1 => Just(Extended::Finite(FD01(i64::MAX))),
        1 => Just(Extended::Finite(FD01(i64::MIN))),
        8 => (-limit..=limit).prop_map(|x| Extended::Finite(FD01(x))),
    ]
}

/// `Extended<FD02>` over `NegInf`, `PosInf`, and finite FD02 (10 ms)
/// values bounded by `i64::MAX / FD02::PREC`.
pub fn extended_fd02() -> impl Strategy<Value = Extended<FD02>> {
    let limit = i64::MAX / FD02::PREC;
    prop_oneof![
        1 => Just(Extended::NegInf),
        1 => Just(Extended::PosInf),
        1 => Just(Extended::Finite(FD02(i64::MAX))),
        1 => Just(Extended::Finite(FD02(i64::MIN))),
        8 => (-limit..=limit).prop_map(|x| Extended::Finite(FD02(x))),
    ]
}

/// `Extended<FD03>` over `NegInf`, `PosInf`, and finite FD03 (1 ms)
/// values bounded by `i64::MAX / FD03::PREC`.
pub fn extended_fd03() -> impl Strategy<Value = Extended<FD03>> {
    let limit = i64::MAX / FD03::PREC;
    prop_oneof![
        1 => Just(Extended::NegInf),
        1 => Just(Extended::PosInf),
        1 => Just(Extended::Finite(FD03(i64::MAX))),
        1 => Just(Extended::Finite(FD03(i64::MIN))),
        8 => (-limit..=limit).prop_map(|x| Extended::Finite(FD03(x))),
    ]
}

/// `Extended<FD06>` over `NegInf`, `PosInf`, and finite FD06
/// values bounded by `i64::MAX / FD06::PREC` (plus i64-edge
/// `Just`s).
pub fn extended_fd06() -> impl Strategy<Value = Extended<FD06>> {
    let limit = i64::MAX / FD06::PREC;
    prop_oneof![
        1 => Just(Extended::NegInf),
        1 => Just(Extended::PosInf),
        1 => Just(Extended::Finite(FD06(i64::MAX))),
        1 => Just(Extended::Finite(FD06(i64::MIN))),
        8 => (-limit..=limit).prop_map(|x| Extended::Finite(FD06(x))),
    ]
}

/// `Extended<FD09>` over `NegInf`, `PosInf`, and finite FD09 (1ns)
/// values across the full `i64` backing range. FD09's `inner` does
/// not multiply through PREC (it's Duration's natural resolution),
/// so the full i64 range is safe.
pub fn extended_fd09() -> impl Strategy<Value = Extended<FD09>> {
    prop_oneof![
        1 => Just(Extended::NegInf),
        1 => Just(Extended::PosInf),
        1 => Just(Extended::Finite(FD09(0))),
        1 => Just(Extended::Finite(FD09(i64::MAX))),
        1 => Just(Extended::Finite(FD09(i64::MIN))),
        1 => Just(Extended::Finite(FD09(1_000_000_000))),  // 1 second
        1 => Just(Extended::Finite(FD09(-1_000_000_000))), // -1 second
        8 => any::<i64>().prop_map(|x| Extended::Finite(FD09(x))),
    ]
}

/// `Extended<FD12>` over `NegInf`, `PosInf`, and finite FD12 values
/// bounded by `i64::MAX / FD12::PREC` (plus i64-edge `Just`s).
pub fn extended_fd12() -> impl Strategy<Value = Extended<FD12>> {
    let limit = i64::MAX / FD12::PREC;
    prop_oneof![
        1 => Just(Extended::NegInf),
        1 => Just(Extended::PosInf),
        1 => Just(Extended::Finite(FD12(i64::MAX))),
        1 => Just(Extended::Finite(FD12(i64::MIN))),
        8 => (-limit..=limit).prop_map(|x| Extended::Finite(FD12(x))),
    ]
}

// ── Sample-rate (Sxxx) Conn strategies ───────────────────────────
//
// For Conn<S_a, S_b> with rational ratio `num/den`, the `inner`
// computation does `c · num / den` and casts to i64; the safe
// Coarse range is `|c| ≤ i64::MAX / num`.

/// Coarse-side i64 strategy for rate↔rate Conn with rational ratio
/// `num/den`. Clamped to `|x| ≤ i64::MAX / num`.
pub fn rate_coarse(num: i128) -> impl Strategy<Value = i64> {
    let limit = (i64::MAX as i128 / num.max(1)) as i64;
    let limit = limit.max(1);
    prop_oneof![
        1 => Just(0_i64),
        1 => Just(1_i64),
        1 => Just(-1_i64),
        1 => Just(limit),
        1 => Just(-limit),
        5 => -limit..=limit,
    ]
}

/// Fine-side i64 strategy for rate↔rate Conn with rational ratio
/// `num/den`. ceil/floor compute `x · den ± (den-1)` as i128, never
/// overflowing for `den ≤ 1e7` and `x ∈ i64`; the i64 cast of the
/// result fits because dividing by `num ≥ den` shrinks magnitude.
/// Bias to boundaries around `±(i64::MAX − den − 1)`.
///
/// **Precondition:** `num ≥ den`. The strategy's range depends only
/// on `den`, but the safety argument relies on `num ≥ den` to ensure
/// the final i64 cast doesn't overflow. Asserted at the top of the
/// function so a caller mismatch fails loudly during property-test
/// setup rather than as a silent overflow inside a generated case.
pub fn rate_fine(den: i128, num: i128) -> impl Strategy<Value = i64> {
    assert!(
        num >= den,
        "rate_fine precondition violated: num ({num}) < den ({den}); \
         the strategy's range relies on num ≥ den to keep the i64 \
         cast in range",
    );
    let near_max = i64::MAX - den as i64 - 1;
    prop_oneof![
        1 => Just(0_i64),
        1 => Just(1_i64),
        1 => Just(-1_i64),
        1 => Just(near_max),
        1 => Just(-near_max),
        5 => -near_max..=near_max,
    ]
}

/// Fine-side strategy for rate↔rate Conn with `inner(ceil(_))`
/// round-trip safety: `|x| ≤ i64::MAX − num`. Use for closure and
/// idempotent properties.
pub fn rate_safe_fine(num: i128) -> impl Strategy<Value = i64> {
    let guard: i64 = i64::try_from(num).expect("num fits i64");
    let limit = i64::MAX - guard;
    prop_oneof![
        1 => Just(0_i64),
        1 => Just(1_i64),
        1 => Just(-1_i64),
        1 => Just(limit),
        1 => Just(-limit),
        5 => -limit..=limit,
    ]
}

// ── FD12↔Sample-rate strategies ──────────────────────────────────
//
// For `Conn<FD12, S_xxx>` (cross-tier between decimal SI time and
// sample-indexed time at a specific rate), FD12-side is full i64,
// Sample-side is bounded by the rate ratio.

/// FD12-side i64 strategy for FD12↔Sample Conn. Full range with
/// boundary bias.
pub fn pico_fine() -> impl Strategy<Value = i64> {
    prop_oneof![
        1 => Just(0_i64),
        1 => Just(1_i64),
        1 => Just(-1_i64),
        1 => Just(i64::MAX),
        1 => Just(i64::MIN + 1),
        5 => any::<i64>(),
    ]
}

/// Sample-side i64 strategy for FD12↔Sample Conn with rational
/// ratio `num/den`. Clamped to `|bits · num / den| < i64::MAX`,
/// i.e. `|bits| < i64::MAX · den / num`.
///
/// `i64::MAX · den` stays in i128 trivially for the rate ratios
/// shipped today (`den ≤ 113_000`); the `saturating_mul` is a
/// belt-and-suspenders for a future `den` past `i128::MAX / i64::MAX`
/// (≈ `1.84e19`), which would silently clamp rather than overflow.
/// No call site approaches that bound.
pub fn pico_coarse(num: i128, den: i128) -> impl Strategy<Value = i64> {
    let limit = ((i64::MAX as i128).saturating_mul(den) / num.max(1)) as i64;
    let limit = limit.max(1);
    prop_oneof![
        1 => Just(0_i64),
        1 => Just(1_i64),
        1 => Just(-1_i64),
        1 => Just(limit),
        1 => Just(-limit),
        5 => -limit..=limit,
    ]
}

/// FD12-side strategy for FD12↔Sample with round-trip safety:
/// `|p| ≤ i64::MAX − num`.
pub fn pico_safe(num: i128) -> impl Strategy<Value = i64> {
    let guard: i64 = i64::try_from(num).expect("num fits i64");
    let limit = i64::MAX - guard;
    prop_oneof![
        1 => Just(0_i64),
        1 => Just(1_i64),
        1 => Just(-1_i64),
        1 => Just(limit),
        1 => Just(-limit),
        5 => -limit..=limit,
    ]
}

// `extended_float_f64` is intentionally NOT vendored — it still
// ships in `connections::property::arb` and is imported directly
// by `conn::float::tests` / `conn::sample::tests`.

#[cfg(test)]
mod tests {
    use super::*;

    proptest! {
        #[test]
        fn arb_jitter_in_range(j in arb_jitter_sigma()) {
            prop_assert!((0..=500_000_000).contains(&j.0));
        }

        #[test]
        fn arb_bpm_in_range(bpm in arb_bpm()) {
            prop_assert!((30_000_000..=400_000_000).contains(&bpm.0));
        }

        #[test]
        fn arb_sample_rate_is_standard(sr in arb_sample_rate()) {
            prop_assert!(matches!(sr, 44_100 | 48_000 | 96_000 | 192_000));
        }
    }
}
