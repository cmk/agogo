//! Polyrhythm DSL parser — pure grid algebra.
//!
//! Parses a grid expression into a [`Grid`]. The grammar supports
//! meet (`&`), join (`|`), implication (`>`), coimplication (`<`),
//! negation (`!`), and channel variable references (`kick`, `C1`).
//!
//! Swing, offset, and delay are channel-level parameters in
//! `ChannelSpec`, not part of the DSL.
//!
//! See `doc/designs/dsl.md` for the full grammar and operator table.

pub mod ast;
mod display;
pub mod error;
mod eval;
mod lexer;
mod parser;

pub use error::DslError;

use crate::time::grid::Grid;

/// Parse a DSL expression into a [`Grid`].
///
/// `env` maps channel variable names to previously-resolved grids.
/// Pass `&[]` when no prior channels exist.
///
/// # Examples
///
/// ```
/// use agogo_core::dsl;
/// use agogo_core::time::grid::Grid;
///
/// // Plain grid atom:
/// assert_eq!(dsl::parse("T16", &[]).unwrap(), Grid::T16);
///
/// // Variable reference:
/// let env = vec![("kick".to_string(), Grid::T4)];
/// assert_eq!(dsl::parse("kick&T16", &env).unwrap(), Grid::T16);
/// ```
pub fn parse(input: &str, env: &[(String, Grid)]) -> Result<Grid, DslError> {
    let tokens = lexer::tokenize(input)?;
    if tokens.is_empty() {
        return Err(DslError {
            kind: error::DslErrorKind::EmptyInput,
            span: ast::Span {
                start: 0,
                end: input.len(),
            },
            source: input.to_string(),
        });
    }
    let expr = parser::parse_tokens(&tokens, input)?;
    eval::eval_expr(&expr, env, input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::ast::{Expr, Span};
    use crate::dsl::eval;
    use crate::time::grid::Grid;
    use connections::lattice::{Coheyting, Heyting, Join, Meet};
    use proptest::prelude::*;

    // ── Spot checks ─────────────────────────────────────────────

    #[test]
    fn atom() {
        assert_eq!(parse("T16", &[]).unwrap(), Grid::T16);
    }

    #[test]
    fn alignment_intersection() {
        // T8 | T8t = join(T8, T8T) = T4
        assert_eq!(parse("T8|T8t", &[]).unwrap(), Grid::T4);
    }

    #[test]
    fn common_refinement() {
        // T16 & T16t = meet(T16, T16T) = T32T
        assert_eq!(parse("T16&T16t", &[]).unwrap(), Grid::T32T);
    }

    #[test]
    fn cross_track() {
        assert_eq!(parse("T16t&T16q", &[]).unwrap(), Grid::T16P);
    }

    #[test]
    fn neg() {
        assert_eq!(parse("!T16", &[]).unwrap(), Grid::T16.neg());
    }

    #[test]
    fn imply() {
        assert_eq!(parse("T16>T8", &[]).unwrap(), Grid::T16.imp(&Grid::T8));
    }

    #[test]
    fn coimply() {
        assert_eq!(parse("T16<T8", &[]).unwrap(), Grid::T16.coimp(&Grid::T8));
    }

    #[test]
    fn complex() {
        let expected = Grid::T16T.meet(&Grid::T16Q).join(&Grid::T8);
        assert_eq!(parse("(T16t&T16q)|T8", &[]).unwrap(), expected);
    }

    // ── Variables ────────────────────────────────────────────────

    #[test]
    fn var_resolves() {
        let env = vec![("kick".to_string(), Grid::T4)];
        assert_eq!(parse("kick", &env).unwrap(), Grid::T4);
    }

    #[test]
    fn var_in_meet() {
        let env = vec![("kick".to_string(), Grid::T4)];
        assert_eq!(parse("kick&T16", &env).unwrap(), Grid::T4.meet(&Grid::T16));
    }

    #[test]
    fn positional_var() {
        let env = vec![("C1".to_string(), Grid::T8)];
        assert_eq!(parse("C1", &env).unwrap(), Grid::T8);
    }

    #[test]
    fn unknown_var() {
        let err = parse("unknown", &[]).unwrap_err();
        assert_eq!(
            err.kind,
            error::DslErrorKind::UnknownVariable("unknown".to_string())
        );
    }

    #[test]
    fn forward_ref() {
        let env = vec![("C1".to_string(), Grid::T4)];
        let err = parse("C2&C1", &env).unwrap_err();
        assert_eq!(
            err.kind,
            error::DslErrorKind::UnknownVariable("C2".to_string())
        );
    }

    // ── All 36 atoms round-trip ──────────────────────────────────

    #[test]
    fn all_36_atoms_round_trip() {
        for g in Grid::ALL {
            let s = g.to_string();
            let result = parse(&s, &[]).unwrap_or_else(|e| {
                panic!("failed to parse {s:?} (from {g:?}): {e}");
            });
            assert_eq!(result, g, "atom round-trip failed for {g:?}");
        }
    }

    // ── Proptests ────────────────────────────────────────────────

    fn arb_grid() -> impl Strategy<Value = Grid> {
        prop::sample::select(Grid::ALL.as_slice())
    }

    fn arb_expr() -> impl Strategy<Value = Expr> {
        let sp = Span { start: 0, end: 0 };
        let leaf = prop_oneof![
            3 => arb_grid().prop_map(move |g| Expr::Atom(g, sp)),
            1 => Just(Expr::Var("x".to_string(), sp)),
        ];
        leaf.prop_recursive(4, 16, 2, move |inner| {
            prop_oneof![
                3 => inner.clone(),
                1 => inner
                    .clone()
                    .prop_map(move |a| Expr::Neg(Box::new(a), sp)),
                1 => (inner.clone(), inner.clone())
                    .prop_map(move |(a, b)| Expr::Meet(Box::new(a), Box::new(b), sp)),
                1 => (inner.clone(), inner.clone())
                    .prop_map(move |(a, b)| Expr::Join(Box::new(a), Box::new(b), sp)),
                1 => (inner.clone(), inner.clone())
                    .prop_map(move |(a, b)| Expr::Imply(Box::new(a), Box::new(b), sp)),
                1 => (inner.clone(), inner)
                    .prop_map(move |(a, b)| Expr::Coimply(Box::new(a), Box::new(b), sp)),
            ]
        })
    }

    proptest! {
        #[test]
        fn eval_preserves_meet(a in arb_grid(), b in arb_grid()) {
            let s = format!("{}&{}", a, b);
            prop_assert_eq!(parse(&s, &[]).unwrap(), a.meet(&b));
        }

        #[test]
        fn eval_preserves_join(a in arb_grid(), b in arb_grid()) {
            let s = format!("{}|{}", a, b);
            prop_assert_eq!(parse(&s, &[]).unwrap(), a.join(&b));
        }

        #[test]
        fn eval_preserves_imply(a in arb_grid(), b in arb_grid()) {
            let s = format!("{}>{}", a, b);
            prop_assert_eq!(parse(&s, &[]).unwrap(), a.imp(&b));
        }

        #[test]
        fn eval_preserves_coimply(a in arb_grid(), b in arb_grid()) {
            let s = format!("{}<{}", a, b);
            prop_assert_eq!(parse(&s, &[]).unwrap(), a.coimp(&b));
        }

        #[test]
        fn eval_preserves_neg(a in arb_grid()) {
            let s = format!("!{}", a);
            prop_assert_eq!(parse(&s, &[]).unwrap(), a.neg());
        }

        #[test]
        fn eval_meet_commutative(a in arb_grid(), b in arb_grid()) {
            let s1 = format!("{}&{}", a, b);
            let s2 = format!("{}&{}", b, a);
            prop_assert_eq!(parse(&s1, &[]).unwrap(), parse(&s2, &[]).unwrap());
        }

        #[test]
        fn eval_join_commutative(a in arb_grid(), b in arb_grid()) {
            let s1 = format!("{}|{}", a, b);
            let s2 = format!("{}|{}", b, a);
            prop_assert_eq!(parse(&s1, &[]).unwrap(), parse(&s2, &[]).unwrap());
        }

        #[test]
        fn eval_meet_associative(
            a in arb_grid(), b in arb_grid(), c in arb_grid(),
        ) {
            let s1 = format!("{}&{}&{}", a, b, c);
            let s2 = format!("{}&({}&{})", a, b, c);
            prop_assert_eq!(parse(&s1, &[]).unwrap(), parse(&s2, &[]).unwrap());
        }

        #[test]
        fn eval_distributive(
            a in arb_grid(), b in arb_grid(), c in arb_grid(),
        ) {
            let s1 = format!("{}&({}|{})", a, b, c);
            let s2 = format!("({}&{})|({}&{})", a, b, a, c);
            prop_assert_eq!(parse(&s1, &[]).unwrap(), parse(&s2, &[]).unwrap());
        }

        /// Variable resolves to any grid in the env.
        #[test]
        fn var_resolves_to_grid(g in arb_grid()) {
            let env = vec![("x".to_string(), g)];
            prop_assert_eq!(parse("x", &env).unwrap(), g);
        }

        /// Variable in an expression composes correctly.
        #[test]
        fn var_in_expr(g in arb_grid()) {
            let env = vec![("x".to_string(), g)];
            prop_assert_eq!(
                parse("x&T16", &env).unwrap(),
                g.meet(&Grid::T16)
            );
        }

        /// Display → parse round-trip preserves evaluation.
        #[test]
        fn display_parse_round_trip(expr in arb_expr()) {
            // Provide "x" in env so Var("x") resolves.
            let env = vec![("x".to_string(), Grid::T4)];
            let displayed = expr.to_string();
            let expected = eval::eval_expr(&expr, &env, &displayed);
            let reparsed = parse(&displayed, &env);
            match (expected, reparsed) {
                (Ok(e), Ok(r)) => prop_assert_eq!(r, e),
                (Ok(e), Err(err)) => {
                    return Err(TestCaseError::fail(format!(
                        "eval OK ({e:?}) but reparse failed: {err}"
                    )));
                }
                (Err(err), Ok(r)) => {
                    return Err(TestCaseError::fail(format!(
                        "eval failed ({err}) but reparse OK: {r:?}"
                    )));
                }
                (Err(err_eval), Err(err_parse)) => {
                    return Err(TestCaseError::fail(format!(
                        "both eval and reparse failed for {displayed:?}: \
                         eval: {err_eval}; parse: {err_parse}"
                    )));
                }
            }
        }
    }
}
