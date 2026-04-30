//! Time-tier proptest strategies vendored from `connections @ d1ac1ead`.
//!
//! These strategies were lifted into `connections::property::arb` by
//! `connections @ 04a12d2`, then deleted alongside the type families
//! they served:
//!
//! - `rate_*` and `pico_*` were removed by `connections @ a99a3ab` when
//!   `conn::sample` moved downstream.
//! - `extended_fdNN` and `fixed_*` were removed by `connections @
//!   6c88862` when `conn::std::i64::decimal` moved downstream.
//!
//! Vendoring them here keeps the staged `time::decimal` and
//! `time::sample` test batteries working with no upstream `Conn`
//! changes. Strategies are `#[cfg(test)]`-only — they exist solely to
//! drive the law-battery proptests inside `time::decimal::tests` and
//! `time::sample::tests`.

use connections::extended::Extended;
use proptest::prelude::*;

use crate::conn::fixed::{FD00, FD01, FD02, FD03, FD06, FD09, FD12, HasResolution};

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

// ── Sample-rate (Sxxx) strategies ────────────────────────────────
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
// by `time::decimal::tests`.
