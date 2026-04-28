//! `ExtendedFloat<f64>` ↔ `Extended<FDxx>` Galois connections.
//!
//! Seven `Conn` constants — `F064FD00 / F064FD01 / F064FD02 / F064FD03 /
//! F064FD06 / F064FD09 / F064FD12` — bridging the f64 boundary into the
//! agogo decimal fixed-point ladder defined in [`crate::time::decimal`].
//!
//! Split out of `time/decimal.rs` (Plan 2026-04-28-03 T1): integer-tier
//! `FDxxFDyy` Conns and the f64-boundary `F064FDxx` Conns are
//! qualitatively different (correction loops, NaN/saturation handling,
//! float-domain proof obligations), so they live in separate files.

use connections::conn::Conn;
use connections::extended::Extended;
use connections::float::ExtendedFloat;

use super::decimal::{FD00, FD01, FD02, FD03, FD06, FD09, FD12};

// ExtendedFloat<f??> → Extended<Rung>. Lawful under `PartialOrd` on both
// sides.
//
// Source lattice (`ExtendedFloat<T>`):
//   `Bot` < `Extend(-∞)` < `Extend(finite)` < `Extend(+∞)` < `Top`,
//   with `Extend(NaN)` reflexive and incomparable with every other
//   `Extend(_)`.
//
// Target lattice (`Extended<Rung>`):
//   `NegInf` < `Finite(Rung(i64::MIN))` < … < `Finite(Rung(i64::MAX))`
//   < `PosInf`.
//
// `inner` embeds the target into the source: `NegInf → Bot`,
// `PosInf → Top`, `Finite(r) → Extend(r/PREC)`. The adjoint laws then
// fix the saturation behaviour of `ceil` and `floor`:
//
// | source input          | ceil                 | floor                 |
// |-----------------------|----------------------|-----------------------|
// | `Bot`                 | `NegInf`             | `NegInf`              |
// | `Top`                 | `PosInf`             | `PosInf`              |
// | `Extend(NaN)`         | `PosInf`             | `NegInf`              |
// | `Extend(-∞)`          | `Finite(Rung::MIN)`  | `NegInf`              |
// | `Extend(+∞)`          | `PosInf`             | `Finite(Rung::MAX)`   |
// | finite < inner(MIN)   | `Finite(Rung::MIN)`  | `NegInf`              |
// | finite > inner(MAX)   | `PosInf`             | `Finite(Rung::MAX)`   |
// | finite in range       | round-up rung        | round-down rung       |
//
// Note the asymmetry: source ±∞ maps to `Finite(Rung::MIN/MAX)` under
// the "inward" adjoint and to `±Inf` under the "outward" one. That
// falls directly out of the Galois law — target ±Inf is above/below
// every Finite, and inner(±Inf) = Bot/Top sit strictly outside the
// source's ±∞.
macro_rules! float_conn {
    ($const_name:ident, $float:ty, $Rung:ident, $prec:expr) => {
        pub const $const_name: Conn<ExtendedFloat<$float>, Extended<$Rung>> = {
            const PREC: i64 = $prec;
            const PREC_F: f64 = PREC as f64;

            // `inner(Rung)` reinterpreted as f64 for the correction-loop
            // comparisons. The `as $float as f64` round-trip is a no-op
            // for f64 (the only shipped instantiation); it's kept so a
            // future F032FD?? instantiation compiles — but F032FD?? is
            // deferred precisely because that round-trip creates plateau
            // collapse classes that turn the correction loops into
            // O(plateau) walks. See §Deferred in plan-2026-04-24-01.
            fn inner_as_f64(c: i64) -> f64 {
                ((c as f64) / PREC_F) as $float as f64
            }

            fn ceil(x: ExtendedFloat<$float>) -> Extended<$Rung> {
                let f = match x {
                    ExtendedFloat::Bot => return Extended::NegInf,
                    ExtendedFloat::Top => return Extended::PosInf,
                    ExtendedFloat::Extend(v) => v,
                };
                if f.is_nan() {
                    return Extended::PosInf;
                }
                if f == <$float>::INFINITY {
                    return Extended::PosInf;
                }
                if f == <$float>::NEG_INFINITY {
                    return Extended::Finite($Rung(i64::MIN));
                }
                let xf = f as f64;
                let scaled = xf * PREC_F;
                // `i64::MAX as f64` rounds to 2^63 (i64::MAX itself isn't
                // representable); the guard is therefore `scaled > 2^63`.
                // Values equal to 2^63 fall through to the correction
                // path, where `scaled.ceil() as i64` saturates to
                // `i64::MAX` — the right answer, just reached via the
                // loop rather than the early return.
                if scaled > i64::MAX as f64 {
                    return Extended::PosInf;
                }
                // `i64::MIN as f64` is exact (-2^63 is representable).
                if scaled < i64::MIN as f64 {
                    return Extended::Finite($Rung(i64::MIN));
                }
                // Initial estimate from f64 math; correct for drift
                // (bounded by ±1 ULP of the scaled product) so the
                // Galois law holds exactly.
                let mut c = scaled.ceil() as i64;
                while c < i64::MAX && inner_as_f64(c) < xf {
                    c += 1;
                }
                // `c > i64::MIN + 1` (not `> i64::MIN`) because the
                // next iteration reads `inner_as_f64(c - 1)`. Any input
                // that would legitimately round to `c = i64::MIN` was
                // already caught by the `scaled < i64::MIN as f64`
                // early return above.
                while c > i64::MIN + 1 && inner_as_f64(c - 1) >= xf {
                    c -= 1;
                }
                Extended::Finite($Rung(c))
            }

            fn inner(b: Extended<$Rung>) -> ExtendedFloat<$float> {
                match b {
                    Extended::NegInf => ExtendedFloat::Bot,
                    Extended::PosInf => ExtendedFloat::Top,
                    Extended::Finite(r) => ExtendedFloat::Extend(((r.0 as f64) / PREC_F) as $float),
                }
            }

            fn floor(x: ExtendedFloat<$float>) -> Extended<$Rung> {
                let f = match x {
                    ExtendedFloat::Bot => return Extended::NegInf,
                    ExtendedFloat::Top => return Extended::PosInf,
                    ExtendedFloat::Extend(v) => v,
                };
                if f.is_nan() {
                    return Extended::NegInf;
                }
                if f == <$float>::INFINITY {
                    return Extended::Finite($Rung(i64::MAX));
                }
                if f == <$float>::NEG_INFINITY {
                    return Extended::NegInf;
                }
                let xf = f as f64;
                let scaled = xf * PREC_F;
                // Saturation bounds mirror `ceil` — see that function
                // for why `i64::MAX as f64` is +2^63 (not exact) but
                // `i64::MIN as f64` is exact.
                if scaled > i64::MAX as f64 {
                    return Extended::Finite($Rung(i64::MAX));
                }
                if scaled < i64::MIN as f64 {
                    return Extended::NegInf;
                }
                let mut c = scaled.floor() as i64;
                while c > i64::MIN + 1 && inner_as_f64(c) > xf {
                    c -= 1;
                }
                while c < i64::MAX && inner_as_f64(c + 1) <= xf {
                    c += 1;
                }
                Extended::Finite($Rung(c))
            }

            Conn::new(ceil, inner, floor)
        };
    };
}

// F064FD?? only. An f32 version (`F032FD??`) would have the same shape
// but inner would narrow i64 → f32, which can collapse up to ~120k
// consecutive Rung values onto the same f32 (for FD12 scale). The
// adjoint law still holds, but ceil/floor would have to walk the full
// collapse class to find the smallest lawful c — O(plateau size) per
// call.
//
// f32 callers widen losslessly at the boundary —
// `F064FD06.ceil(ExtendedFloat::Extend(arg_f32 as f64))` — and get the
// same adjoint answer as native f64 input.
float_conn!(F064FD00, f64, FD00, 1);
float_conn!(F064FD01, f64, FD01, 10);
float_conn!(F064FD02, f64, FD02, 100);
float_conn!(F064FD03, f64, FD03, 1_000);
float_conn!(F064FD06, f64, FD06, 1_000_000);
float_conn!(F064FD09, f64, FD09, 1_000_000_000);
float_conn!(F064FD12, f64, FD12, 1_000_000_000_000);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::arb::{
        extended_fd00, extended_fd01, extended_fd02, extended_fd03, extended_fd06, extended_fd09,
        extended_fd12,
    };
    use connections::prop::arb::extended_float_f64;
    use proptest::prelude::*;

    // ── ExtendedFloat<f??> → Extended<Rung> connections ─────────────────
    //
    // Strategies (`extended_float_f64`, `extended_fd06`,
    // `extended_fd12`) live in `crate::time::arb`; see the doc
    // there for the `arb_f64_bounded` rationale on why a bounded
    // range beats `any::<f64>()` for these saturation-prone inputs.

    // Spot checks with exactly-representable f64 values.
    #[test]
    fn float_conn_spot() {
        let half = ExtendedFloat::Extend(0.5_f64);
        assert_eq!(F064FD03.ceil(half), Extended::Finite(FD03(500)));
        assert_eq!(F064FD03.floor(half), Extended::Finite(FD03(500)));
        assert_eq!(F064FD06.ceil(half), Extended::Finite(FD06(500_000)));
        assert_eq!(
            F064FD12.ceil(ExtendedFloat::Extend(0.25_f64)),
            Extended::Finite(FD12(250_000_000_000))
        );

        let one_and_half = ExtendedFloat::Extend(1.5_f64);
        assert_eq!(
            F064FD06.ceil(one_and_half),
            Extended::Finite(FD06(1_500_000))
        );
        assert_eq!(
            F064FD06.floor(one_and_half),
            Extended::Finite(FD06(1_500_000))
        );
        assert_eq!(
            F064FD06.inner(Extended::Finite(FD06(1_500_000))),
            ExtendedFloat::Extend(1.5_f64)
        );

        // Saturation map (matches the table in the macro doc comment).
        assert_eq!(F064FD06.ceil(ExtendedFloat::Bot), Extended::NegInf);
        assert_eq!(F064FD06.floor(ExtendedFloat::Bot), Extended::NegInf);
        assert_eq!(F064FD06.ceil(ExtendedFloat::Top), Extended::PosInf);
        assert_eq!(F064FD06.floor(ExtendedFloat::Top), Extended::PosInf);

        let nan: ExtendedFloat<f64> = ExtendedFloat::Extend(f64::NAN);
        assert_eq!(F064FD06.ceil(nan), Extended::PosInf);
        assert_eq!(F064FD06.floor(nan), Extended::NegInf);

        let pos_inf: ExtendedFloat<f64> = ExtendedFloat::Extend(f64::INFINITY);
        assert_eq!(F064FD06.ceil(pos_inf), Extended::PosInf);
        assert_eq!(F064FD06.floor(pos_inf), Extended::Finite(FD06(i64::MAX)));

        let neg_inf: ExtendedFloat<f64> = ExtendedFloat::Extend(f64::NEG_INFINITY);
        assert_eq!(F064FD06.ceil(neg_inf), Extended::Finite(FD06(i64::MIN)));
        assert_eq!(F064FD06.floor(neg_inf), Extended::NegInf);

        // Inner maps target ±Inf to ExtendedFloat's Top/Bot (synthetic
        // bounds outside the float range), NOT to Extend(±f64::INFINITY).
        assert_eq!(F064FD06.inner(Extended::PosInf), ExtendedFloat::Top);
        assert_eq!(F064FD06.inner(Extended::NegInf), ExtendedFloat::Bot);
    }

    // F064FD00 (PREC=1) is the identity-like regime — `inner(c) = c as f64`
    // with no division, so the correction loop is vacuous and saturation
    // is the only behaviour that can go wrong. Covered separately because
    // the proptest battery below exercises F064FD06 and F064FD12 only.
    #[test]
    fn f064fd00_saturation() {
        assert_eq!(F064FD00.ceil(ExtendedFloat::Bot), Extended::NegInf);
        assert_eq!(F064FD00.floor(ExtendedFloat::Bot), Extended::NegInf);
        assert_eq!(F064FD00.ceil(ExtendedFloat::Top), Extended::PosInf);
        assert_eq!(F064FD00.floor(ExtendedFloat::Top), Extended::PosInf);

        let nan: ExtendedFloat<f64> = ExtendedFloat::Extend(f64::NAN);
        assert_eq!(F064FD00.ceil(nan), Extended::PosInf);
        assert_eq!(F064FD00.floor(nan), Extended::NegInf);

        let pos_inf: ExtendedFloat<f64> = ExtendedFloat::Extend(f64::INFINITY);
        assert_eq!(F064FD00.ceil(pos_inf), Extended::PosInf);
        assert_eq!(F064FD00.floor(pos_inf), Extended::Finite(FD00(i64::MAX)));

        let neg_inf: ExtendedFloat<f64> = ExtendedFloat::Extend(f64::NEG_INFINITY);
        assert_eq!(F064FD00.ceil(neg_inf), Extended::Finite(FD00(i64::MIN)));
        assert_eq!(F064FD00.floor(neg_inf), Extended::NegInf);

        // Identity on exact integers.
        assert_eq!(
            F064FD00.ceil(ExtendedFloat::Extend(42.0)),
            Extended::Finite(FD00(42))
        );
        assert_eq!(
            F064FD00.floor(ExtendedFloat::Extend(42.0)),
            Extended::Finite(FD00(42))
        );
        assert_eq!(
            F064FD00.ceil(ExtendedFloat::Extend(-42.0)),
            Extended::Finite(FD00(-42))
        );
        assert_eq!(
            F064FD00.floor(ExtendedFloat::Extend(-42.0)),
            Extended::Finite(FD00(-42))
        );

        // Non-integer: ceil up, floor down.
        assert_eq!(
            F064FD00.ceil(ExtendedFloat::Extend(0.25)),
            Extended::Finite(FD00(1))
        );
        assert_eq!(
            F064FD00.floor(ExtendedFloat::Extend(0.25)),
            Extended::Finite(FD00(0))
        );
        assert_eq!(
            F064FD00.ceil(ExtendedFloat::Extend(-0.25)),
            Extended::Finite(FD00(0))
        );
        assert_eq!(
            F064FD00.floor(ExtendedFloat::Extend(-0.25)),
            Extended::Finite(FD00(-1))
        );

        // Very large finite: saturating, into Finite at the ceil/floor
        // boundary rather than flowing to ±Inf on the target.
        let huge = ExtendedFloat::Extend(2.0_f64.powi(70)); // > i64::MAX
        assert_eq!(F064FD00.ceil(huge), Extended::PosInf);
        assert_eq!(F064FD00.floor(huge), Extended::Finite(FD00(i64::MAX)));

        let tiny = ExtendedFloat::Extend(-2.0_f64.powi(70)); // < i64::MIN
        assert_eq!(F064FD00.ceil(tiny), Extended::Finite(FD00(i64::MIN)));
        assert_eq!(F064FD00.floor(tiny), Extended::NegInf);
    }

    // Adjoint + closure + kernel + monotonicity, stated over
    // PartialOrd on both sides. Mirrors `doc/design.md §Testing`.
    macro_rules! float_conn_props {
        ($mod:ident, $conn:ident, $Rung:ident, $arb_src:ident, $arb_tgt:ident) => {
            mod $mod {
                use super::*;
                use connections::prop::conn as laws;

                proptest! {
                    // Float-Conn shrinks on this domain are expensive:
                    // cap cases at 64 and shrink iters at 512 (default
                    // ~1M is ruinous and finds no bugs the first few
                    // hundred miss).
                    #![proptest_config(ProptestConfig {
                        cases: 64,
                        max_shrink_iters: 512,
                        .. ProptestConfig::default()
                    })]

                    #[test]
                    fn galois_l(a in $arb_src(), b in $arb_tgt()) {
                        prop_assert!(laws::conn_galois_l(&$conn, a, b));
                    }

                    #[test]
                    fn galois_r(a in $arb_src(), b in $arb_tgt()) {
                        prop_assert!(laws::conn_galois_r(&$conn, a, b));
                    }

                    #[test]
                    fn closure_l(a in $arb_src()) {
                        prop_assert!(laws::conn_closure_l(&$conn, a));
                    }

                    #[test]
                    fn closure_r(a in $arb_src()) {
                        prop_assert!(laws::conn_closure_r(&$conn, a));
                    }

                    #[test]
                    fn kernel_l(b in $arb_tgt()) {
                        prop_assert!(laws::conn_kernel_l(&$conn, b));
                    }

                    #[test]
                    fn kernel_r(b in $arb_tgt()) {
                        prop_assert!(laws::conn_kernel_r(&$conn, b));
                    }

                    #[test]
                    fn monotone_l(a1 in $arb_src(), a2 in $arb_src()) {
                        prop_assert!(laws::conn_monotone_l(&$conn, a1, a2));
                    }

                    // Idempotence: inner∘ceil is idempotent on its
                    // image. ExtendedFloat<f64>'s PartialEq treats
                    // Extend(NaN) == Extend(NaN) as true, so the
                    // Eq-bound `conn_idempotent` predicate is the
                    // right comparison here.
                    #[test]
                    fn idempotent(a in $arb_src()) {
                        prop_assert!(laws::conn_idempotent(&$conn, a));
                    }
                }

                // Type-check the rung binding so the macro input
                // doesn't silently diverge from the conn's actual
                // target.
                #[allow(dead_code)]
                fn _type_assert(x: Extended<$Rung>) -> Extended<$Rung> {
                    let _: Conn<_, Extended<$Rung>> = $conn;
                    x
                }
            }
        };
    }

    // Drive the full 9-law battery on every `F064FD<N>` rung. Earlier
    // versions tested only FD06 + FD12 (the two structurally distinct
    // PREC regimes — well below mantissa vs. approaching it); the
    // publish-prep audit pulled the rest forward so each rung gets
    // its own per-PREC saturation coverage.
    float_conn_props!(
        p_f064_fd00,
        F064FD00,
        FD00,
        extended_float_f64,
        extended_fd00
    );
    float_conn_props!(
        p_f064_fd01,
        F064FD01,
        FD01,
        extended_float_f64,
        extended_fd01
    );
    float_conn_props!(
        p_f064_fd02,
        F064FD02,
        FD02,
        extended_float_f64,
        extended_fd02
    );
    float_conn_props!(
        p_f064_fd03,
        F064FD03,
        FD03,
        extended_float_f64,
        extended_fd03
    );
    float_conn_props!(
        p_f064_fd06,
        F064FD06,
        FD06,
        extended_float_f64,
        extended_fd06
    );
    float_conn_props!(
        p_f064_fd09,
        F064FD09,
        FD09,
        extended_float_f64,
        extended_fd09
    );
    float_conn_props!(
        p_f064_fd12,
        F064FD12,
        FD12,
        extended_float_f64,
        extended_fd12
    );
}
