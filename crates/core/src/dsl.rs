//! Polyrhythm DSL parser.
//!
//! Parses a grid algebra expression into a [`TrackSpec`]. The grammar
//! supports meet (`&`), join (`|`), implication (`>`), coimplication
//! (`<`), negation (`!`), swing (`~TBase:amount`), and offset
//! (`@ticks`).
//!
//! See `doc/designs/dsl.md` for the full grammar and operator table.

pub mod ast;
mod display;
pub mod error;
mod eval;
mod lexer;
mod parser;

pub use ast::TrackSpec;
pub use error::DslError;

/// Parse a DSL expression into a [`TrackSpec`].
///
/// Returns one `TrackSpec` per call — polyrhythm is achieved by
/// calling `parse` once per `--ch` flag, not by a separator in
/// the grammar.
///
/// # Examples
///
/// ```
/// use agogo_core::dsl;
///
/// let spec = dsl::parse("T16~T16:80@-5").unwrap();
/// assert_eq!(spec.grid, agogo_core::time::grid::Grid::T16);
/// assert_eq!(spec.offset_ticks, Some(-5));
/// ```
pub fn parse(input: &str) -> Result<TrackSpec, DslError> {
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
    let track_ast = parser::parse_tokens(&tokens, input)?;
    Ok(eval::eval_track(&track_ast))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::ast::{Expr, Span, TrackAst};
    use crate::dsl::eval;
    use crate::time::grid::{self, Grid};
    use crate::time::swing::SwingConfig;
    use crate::time::tbase::TBase;
    use proptest::prelude::*;

    // ── Spot checks from doc/designs/dsl.md examples ────────────

    #[test]
    fn example_swung_16ths() {
        let spec = parse("T16~T16:80").unwrap();
        assert_eq!(spec.grid, Grid::T16);
        assert_eq!(
            spec.swing,
            Some(SwingConfig {
                resolution: TBase::T16,
                amount: 80,
            })
        );
    }

    #[test]
    fn example_swung_with_offset() {
        let spec = parse("T16~T16:80@-50").unwrap();
        assert_eq!(spec.grid, Grid::T16);
        assert_eq!(spec.offset_ticks, Some(-50));
    }

    #[test]
    fn example_alignment_intersection() {
        // T8 | T8t = join(T8, T8T) = T4
        let spec = parse("T8|T8t").unwrap();
        assert_eq!(spec.grid, Grid::T4);
    }

    #[test]
    fn example_common_refinement() {
        // T16 & T16t = meet(T16, T16T) = T32T
        let spec = parse("T16&T16t").unwrap();
        assert_eq!(spec.grid, Grid::T32T);
    }

    #[test]
    fn example_cross_track_refinement() {
        // T16t & T16q = meet(T16T, T16Q) = T16P
        let spec = parse("T16t&T16q").unwrap();
        assert_eq!(spec.grid, Grid::T16P);
    }

    #[test]
    fn example_neg() {
        let spec = parse("!T16").unwrap();
        assert_eq!(spec.grid, grid::neg(Grid::T16));
    }

    #[test]
    fn example_imply() {
        let spec = parse("T16>T8").unwrap();
        assert_eq!(spec.grid, grid::imply(Grid::T16, Grid::T8));
    }

    #[test]
    fn example_coimply() {
        let spec = parse("T16<T8").unwrap();
        assert_eq!(spec.grid, grid::coimp(Grid::T16, Grid::T8));
    }

    #[test]
    fn example_complex() {
        // (T16t & T16q) | T8
        let spec = parse("(T16t&T16q)|T8").unwrap();
        let expected = grid::join(grid::meet(Grid::T16T, Grid::T16Q), Grid::T8);
        assert_eq!(spec.grid, expected);
    }

    // ── Single-atom round-trips ─────────────────────────────────

    #[test]
    fn all_36_atoms_round_trip() {
        for g in Grid::ALL {
            let s = g.to_string();
            let spec = parse(&s).unwrap_or_else(|e| {
                panic!("failed to parse {s:?} (from {g:?}): {e}");
            });
            assert_eq!(spec.grid, g, "atom round-trip failed for {g:?}");
            assert_eq!(spec.swing, None);
            assert_eq!(spec.offset_ticks, None);
        }
    }

    // ── Proptest: eval preserves lattice ops ─────────────────────

    fn arb_grid() -> impl Strategy<Value = Grid> {
        prop::sample::select(Grid::ALL.as_slice())
    }

    fn arb_expr() -> impl Strategy<Value = Expr> {
        let sp = Span { start: 0, end: 0 };
        let leaf = arb_grid().prop_map(move |g| Expr::Atom(g, sp));
        leaf.prop_recursive(4, 16, 2, move |inner| {
            prop_oneof![
                3 => inner.clone(),
                1 => inner.clone().prop_map(move |a| Expr::Neg(Box::new(a), sp)),
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
            let spec = parse(&s).unwrap();
            prop_assert_eq!(spec.grid, grid::meet(a, b));
        }

        #[test]
        fn eval_preserves_join(a in arb_grid(), b in arb_grid()) {
            let s = format!("{}|{}", a, b);
            let spec = parse(&s).unwrap();
            prop_assert_eq!(spec.grid, grid::join(a, b));
        }

        #[test]
        fn eval_preserves_imply(a in arb_grid(), b in arb_grid()) {
            let s = format!("{}>{}", a, b);
            let spec = parse(&s).unwrap();
            prop_assert_eq!(spec.grid, grid::imply(a, b));
        }

        #[test]
        fn eval_preserves_coimply(a in arb_grid(), b in arb_grid()) {
            let s = format!("{}<{}", a, b);
            let spec = parse(&s).unwrap();
            prop_assert_eq!(spec.grid, grid::coimp(a, b));
        }

        #[test]
        fn eval_preserves_neg(a in arb_grid()) {
            let s = format!("!{}", a);
            let spec = parse(&s).unwrap();
            prop_assert_eq!(spec.grid, grid::neg(a));
        }

        /// Meet is commutative through the parser.
        #[test]
        fn eval_meet_commutative(a in arb_grid(), b in arb_grid()) {
            let s1 = format!("{}&{}", a, b);
            let s2 = format!("{}&{}", b, a);
            prop_assert_eq!(parse(&s1).unwrap().grid, parse(&s2).unwrap().grid);
        }

        /// Join is commutative through the parser.
        #[test]
        fn eval_join_commutative(a in arb_grid(), b in arb_grid()) {
            let s1 = format!("{}|{}", a, b);
            let s2 = format!("{}|{}", b, a);
            prop_assert_eq!(parse(&s1).unwrap().grid, parse(&s2).unwrap().grid);
        }

        /// Meet is associative: a&b&c == a&(b&c)
        #[test]
        fn eval_meet_associative(
            a in arb_grid(), b in arb_grid(), c in arb_grid(),
        ) {
            let s1 = format!("{}&{}&{}", a, b, c);
            let s2 = format!("{}&({}&{})", a, b, c);
            prop_assert_eq!(parse(&s1).unwrap().grid, parse(&s2).unwrap().grid);
        }

        /// Distributivity: a&(b|c) == (a&b)|(a&c)
        #[test]
        fn eval_distributive(
            a in arb_grid(), b in arb_grid(), c in arb_grid(),
        ) {
            let s1 = format!("{}&({}|{})", a, b, c);
            let s2 = format!("({}&{})|({}&{})", a, b, a, c);
            prop_assert_eq!(parse(&s1).unwrap().grid, parse(&s2).unwrap().grid);
        }

        /// Display → parse round-trip preserves evaluation for all
        /// operator variants and nesting depths up to 4.
        #[test]
        fn display_parse_round_trip(expr in arb_expr()) {
            let sp = Span { start: 0, end: 0 };
            let ast = TrackAst {
                expr: expr.clone(),
                modifiers: vec![],
                span: sp,
            };
            let displayed = ast.to_string();
            let reparsed = parse(&displayed).map_err(|e| {
                TestCaseError::fail(format!("parse({displayed:?}): {e}"))
            })?;
            let expected = eval::eval_expr(&expr);
            prop_assert_eq!(reparsed.grid, expected);
        }
    }
}
