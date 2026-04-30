//! Decimal fixed-point ladder: `FD00 / FD01 / FD02 / FD03 / FD06 / FD09 / FD12`.
//!
//! Each type is an `i64` numerator with an implicit 10⁻ᵏ denominator:
//!
//! - `FD00(i) = i × 10⁰`   (1 s)
//! - `FD01(i) = i × 10⁻¹`  (100 ms)
//! - `FD02(i) = i × 10⁻²`  (10 ms)
//! - `FD03(i) = i × 10⁻³`  (1 ms)
//! - `FD06(i) = i × 10⁻⁶`  (1 µs)
//! - `FD09(i) = i × 10⁻⁹`  (1 ns)
//! - `FD12(i) = i × 10⁻¹²` (1 ps)
//!
//! For every ordered pair `(Fine, Coarse)` where `Fine`'s resolution is
//! strictly smaller, there is a [`Conn`]`<Fine, Coarse>` named
//! `FD<dd>FD<dd>`:
//!
//! - `ceil:  Fine → Coarse`  smallest `c` with `inner(c) ≥ f`
//! - `inner: Coarse → Fine`  exact embedding (`c × PREC_ratio`)
//! - `floor: Fine → Coarse`  largest `c` with `inner(c) ≤ f`
//!
//! Ported from `Data.Connection.Fixed` in
//! <https://github.com/cmk/connections> (Haskell).
//!
//! Both rounding functions use [`i64::div_euclid`] / [`i64::rem_euclid`]
//! so negative inputs round consistently toward −∞ for `floor` and
//! toward +∞ for `ceil`. This matches the documented adjoint-triple
//! semantics of [`Conn`]; the Haskell `fixfix` `h` used `div` with a
//! `j − 1` fixup on nonzero remainder, which does not satisfy the
//! standard lower-adjoint Galois law and is believed to be an
//! idiosyncrasy of the Haskell implementation rather than an
//! intentional different convention. (Haskell `ratfix`'s `h` is a
//! plain `div`, matching this port.)

use connections::conn::Conn;

macro_rules! def_fixed {
    ($name:ident, $prec:expr) => {
        #[repr(transparent)]
        #[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
        pub struct $name(pub i64);

        impl HasResolution for $name {
            const PREC: i64 = $prec;
        }

        impl $name {
            pub const ZERO: Self = Self(0);
        }
    };
}

/// Decimal precision marker: `PREC` is the denominator such that a
/// value `T(i)` represents `i × 10⁻ᵏ` with `PREC = 10ᵏ`.
pub trait HasResolution {
    const PREC: i64;
}

def_fixed!(FD00, 1);
def_fixed!(FD01, 10);
def_fixed!(FD02, 100);
def_fixed!(FD03, 1_000);
def_fixed!(FD06, 1_000_000);
def_fixed!(FD09, 1_000_000_000);
def_fixed!(FD12, 1_000_000_000_000);

// ────────────────────────────────────────────────────────────────────
// Domain aliases.
//
// Two time-unit words kept alongside the canonical FD06 / FD12
// names because they read more naturally at FFI seams (host-link
// session arming, channel::scheduler delay/offset arithmetic, the
// cpal seam, jitter math). Every other workspace site uses the
// canonical FDxx names directly. Moved here from fxp.rs (Plan
// 2026-04-28-03 T5) — the aliases live with the types they alias.
// ────────────────────────────────────────────────────────────────────

/// Domain alias for µs. Used at FFI seams: `Quantum(Micro)`,
/// `host-link::session`, `ChannelCommon::{delay, offset}`,
/// `channel::scheduler` arithmetic.
pub use FD06 as Micro;

/// Domain alias for ps. Used in `pico_to_samples`, the cpal seam,
/// `arb::pulse_train`, `sync::pll` jitter math.
pub use FD12 as Pico;

// ────────────────────────────────────────────────────────────────────
// Fine → Coarse connection constructors.
//
// For each pair, the ratio `prec = Fine::PREC / Coarse::PREC` is the
// multiplier `inner` applies to go Coarse → Fine (e.g. 1 FD00 = 1000
// FD03, so `FD03FD00`'s prec is 1000).
//
// The block-scoped `fn` items inside each `const` expression are
// monomorphic and get coerced to `fn(_) -> _` pointers at const-eval,
// which is all `Conn` asks for.
// ────────────────────────────────────────────────────────────────────

macro_rules! fix_fix {
    ($const_name:ident, $Fine:ident, $Coarse:ident, $prec:expr) => {
        pub const $const_name: Conn<$Fine, $Coarse> = {
            const PREC: i64 = $prec;
            fn ceil(x: $Fine) -> $Coarse {
                let q = x.0.div_euclid(PREC);
                if x.0.rem_euclid(PREC) != 0 {
                    $Coarse(q + 1)
                } else {
                    $Coarse(q)
                }
            }
            fn inner(x: $Coarse) -> $Fine {
                $Fine(x.0 * PREC)
            }
            fn floor(x: $Fine) -> $Coarse {
                $Coarse(x.0.div_euclid(PREC))
            }
            Conn::new(ceil, inner, floor)
        };
    };
}

// Adjacent (one step on the ladder).
fix_fix!(FD01FD00, FD01, FD00, 10);
fix_fix!(FD02FD01, FD02, FD01, 10);
fix_fix!(FD03FD02, FD03, FD02, 10);
fix_fix!(FD06FD03, FD06, FD03, 1_000);
fix_fix!(FD09FD06, FD09, FD06, 1_000);
fix_fix!(FD12FD09, FD12, FD09, 1_000);

// Non-adjacent (direct shortcut; matches Haskell verbatim).
fix_fix!(FD02FD00, FD02, FD00, 100);
fix_fix!(FD03FD00, FD03, FD00, 1_000);
fix_fix!(FD03FD01, FD03, FD01, 100);
fix_fix!(FD06FD00, FD06, FD00, 1_000_000);
fix_fix!(FD06FD01, FD06, FD01, 100_000);
fix_fix!(FD06FD02, FD06, FD02, 10_000);
fix_fix!(FD09FD00, FD09, FD00, 1_000_000_000);
fix_fix!(FD09FD01, FD09, FD01, 100_000_000);
fix_fix!(FD09FD02, FD09, FD02, 10_000_000);
fix_fix!(FD09FD03, FD09, FD03, 1_000_000);
fix_fix!(FD12FD00, FD12, FD00, 1_000_000_000_000);
fix_fix!(FD12FD01, FD12, FD01, 100_000_000_000);
fix_fix!(FD12FD02, FD12, FD02, 10_000_000_000);
fix_fix!(FD12FD03, FD12, FD03, 1_000_000_000);
fix_fix!(FD12FD06, FD12, FD06, 1_000_000);

// `ExtendedFloat<f64>` ↔ `Extended<FDxx>` Conns (`F064FDxx`) live in
// `crate::conn::float` (Plan 2026-04-28-03 T1). Qualitatively different
// shape (correction loops, NaN/saturation handling) from the
// integer-tier `fix_fix!` Conns above.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conn::arb::{fixed_coarse, fixed_fine, fixed_safe_fine};
    use proptest::prelude::*;

    // Sanity spot checks (hand-computed).

    #[test]
    fn spot_fd03_fd00_positive() {
        assert_eq!(FD03FD00.ceil(FD03(5)), FD00(1));
        assert_eq!(FD03FD00.floor(FD03(5)), FD00(0));
        assert_eq!(FD03FD00.ceil(FD03(1_000)), FD00(1));
        assert_eq!(FD03FD00.floor(FD03(1_000)), FD00(1));
        assert_eq!(FD03FD00.ceil(FD03(999)), FD00(1));
        assert_eq!(FD03FD00.floor(FD03(999)), FD00(0));
    }

    #[test]
    fn spot_fd03_fd00_negative() {
        // div_euclid rounds toward −∞; rem_euclid is non-negative.
        // -5 / 1000: div_euclid = -1, rem_euclid = 995.
        assert_eq!(FD03FD00.ceil(FD03(-5)), FD00(0));
        assert_eq!(FD03FD00.floor(FD03(-5)), FD00(-1));
    }

    #[test]
    fn spot_fd12_fd00_exact_boundary() {
        assert_eq!(FD12FD00.ceil(FD12(1_000_000_000_000)), FD00(1));
        assert_eq!(FD12FD00.floor(FD12(1_000_000_000_000)), FD00(1));
        assert_eq!(FD12FD00.inner(FD00(1)), FD12(1_000_000_000_000));
    }

    #[test]
    fn spot_inner_roundtrip_across_ladder() {
        // inner is exact — multiplying up and down gets us back.
        assert_eq!(FD03FD00.ceil(FD03FD00.inner(FD00(42))), FD00(42));
        assert_eq!(FD03FD00.floor(FD03FD00.inner(FD00(-42))), FD00(-42));
        assert_eq!(FD12FD06.ceil(FD12FD06.inner(FD06(987))), FD06(987));
    }

    // Each test is written for one Conn and one (Fine, Coarse) pair,
    // then expanded via macro across the 21 connections.

    macro_rules! props_for_pair {
        ($mod:ident, $conn:ident, $Fine:ident, $Coarse:ident, $prec:expr) => {
            mod $mod {
                use super::*;
                use connections::prop::conn as laws;

                proptest! {
                    #[test]
                    fn roundtrip_ceil(c in fixed_coarse($prec)) {
                        prop_assert!(laws::conn_roundtrip_ceil(&$conn, $Coarse(c)));
                    }

                    #[test]
                    fn roundtrip_floor(c in fixed_coarse($prec)) {
                        prop_assert!(laws::conn_roundtrip_floor(&$conn, $Coarse(c)));
                    }

                    #[test]
                    fn monotone_l(x in fixed_fine($prec), y in fixed_fine($prec)) {
                        prop_assert!(laws::conn_monotone_l(&$conn, $Fine(x), $Fine(y)));
                    }

                    #[test]
                    fn floor_le_ceil(x in fixed_fine($prec)) {
                        let a = $Fine(x);
                        prop_assert!(laws::conn_floor_le_ceil(&$conn, a));
                        // Stronger: fixed-ladder ULP bound (ceil − floor ≤ 1).
                        prop_assert!(laws::conn_ulp_bound(&$conn, a, |b| b.0));
                    }

                    #[test]
                    fn galois_l(
                        x in fixed_fine($prec),
                        c in fixed_coarse($prec),
                    ) {
                        prop_assert!(laws::conn_galois_l(&$conn, $Fine(x), $Coarse(c)));
                    }

                    #[test]
                    fn galois_r(
                        x in fixed_fine($prec),
                        c in fixed_coarse($prec),
                    ) {
                        prop_assert!(laws::conn_galois_r(&$conn, $Fine(x), $Coarse(c)));
                    }

                    // Closure laws use fixed_safe_fine because the
                    // round-trip through inner multiplies by PREC and
                    // must fit i64; see fixed_safe_fine docs in
                    // crate::conn::arb.
                    #[test]
                    fn closure_l(x in fixed_safe_fine($prec)) {
                        prop_assert!(laws::conn_closure_l(&$conn, $Fine(x)));
                    }

                    #[test]
                    fn closure_r(x in fixed_safe_fine($prec)) {
                        prop_assert!(laws::conn_closure_r(&$conn, $Fine(x)));
                    }

                    #[test]
                    fn idempotent(x in fixed_safe_fine($prec)) {
                        prop_assert!(laws::conn_idempotent(&$conn, $Fine(x)));
                    }
                }
            }
        };
    }

    // Adjacent pairs.
    props_for_pair!(p_fd01_fd00, FD01FD00, FD01, FD00, 10);
    props_for_pair!(p_fd02_fd01, FD02FD01, FD02, FD01, 10);
    props_for_pair!(p_fd03_fd02, FD03FD02, FD03, FD02, 10);
    props_for_pair!(p_fd06_fd03, FD06FD03, FD06, FD03, 1_000);
    props_for_pair!(p_fd09_fd06, FD09FD06, FD09, FD06, 1_000);
    props_for_pair!(p_fd12_fd09, FD12FD09, FD12, FD09, 1_000);

    // Non-adjacent pairs.
    props_for_pair!(p_fd02_fd00, FD02FD00, FD02, FD00, 100);
    props_for_pair!(p_fd03_fd00, FD03FD00, FD03, FD00, 1_000);
    props_for_pair!(p_fd03_fd01, FD03FD01, FD03, FD01, 100);
    props_for_pair!(p_fd06_fd00, FD06FD00, FD06, FD00, 1_000_000);
    props_for_pair!(p_fd06_fd01, FD06FD01, FD06, FD01, 100_000);
    props_for_pair!(p_fd06_fd02, FD06FD02, FD06, FD02, 10_000);
    props_for_pair!(p_fd09_fd00, FD09FD00, FD09, FD00, 1_000_000_000);
    props_for_pair!(p_fd09_fd01, FD09FD01, FD09, FD01, 100_000_000);
    props_for_pair!(p_fd09_fd02, FD09FD02, FD09, FD02, 10_000_000);
    props_for_pair!(p_fd09_fd03, FD09FD03, FD09, FD03, 1_000_000);
    props_for_pair!(p_fd12_fd00, FD12FD00, FD12, FD00, 1_000_000_000_000);
    props_for_pair!(p_fd12_fd01, FD12FD01, FD12, FD01, 100_000_000_000);
    props_for_pair!(p_fd12_fd02, FD12FD02, FD12, FD02, 10_000_000_000);
    props_for_pair!(p_fd12_fd03, FD12FD03, FD12, FD03, 1_000_000_000);
    props_for_pair!(p_fd12_fd06, FD12FD06, FD12, FD06, 1_000_000);

    // `compose!`-macro tests live in `crate::conn`'s test module,
    // alongside the macro itself. Composition is a property of the
    // `Conn` abstraction, not of any one ladder pair; keeping the
    // tests there preserves the invariant that `conn::fixed` only
    // owns laws that are specific to fixed-point connections.

    // Negative-value regression: every Galois property holds for i < 0.
    // (Covered inside each pair's proptest since bounded_fine / bounded_coarse
    //  emit negatives; this stand-alone confirms the exact boundary FD00(-5).)
    #[test]
    fn regression_negative_fd03_fd00() {
        assert_eq!(FD03FD00.ceil(FD03(-1_000)), FD00(-1));
        assert_eq!(FD03FD00.floor(FD03(-1_000)), FD00(-1));
        assert_eq!(FD03FD00.ceil(FD03(-1_001)), FD00(-1));
        assert_eq!(FD03FD00.floor(FD03(-1_001)), FD00(-2));
    }

    // ── ExtendedFloat<f??> → Extended<Rung> tests live in
    // `crate::conn::float::tests` (Plan 2026-04-28-03 T1).
}
