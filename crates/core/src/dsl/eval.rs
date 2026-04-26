//! AST → [`TrackSpec`] evaluator.
//!
//! Trivially recursive on [`Expr`]: each variant maps to the
//! corresponding lattice operation on [`Grid`].

use super::ast::*;
use crate::time::grid::{self, Grid};
use crate::time::swing::SwingConfig;

/// Evaluate a [`TrackAst`] into a [`TrackSpec`].
pub fn eval_track(track: &TrackAst) -> TrackSpec {
    let grid = eval_expr(&track.expr);
    let mut swing = None;
    let mut offset_ticks = None;

    for m in &track.modifiers {
        match m {
            Modifier::Swing(res, amount, _) => {
                swing = Some(SwingConfig {
                    resolution: *res,
                    amount: *amount,
                });
            }
            Modifier::Offset(ticks, _) => {
                offset_ticks = Some(*ticks);
            }
        }
    }

    TrackSpec {
        grid,
        swing,
        offset_ticks,
    }
}

/// Evaluate a grid expression to a [`Grid`].
pub fn eval_expr(expr: &Expr) -> Grid {
    match expr {
        Expr::Atom(g, _) => *g,
        Expr::Neg(inner, _) => grid::neg(eval_expr(inner)),
        Expr::Meet(a, b, _) => grid::meet(eval_expr(a), eval_expr(b)),
        Expr::Join(a, b, _) => grid::join(eval_expr(a), eval_expr(b)),
        Expr::Imply(a, b, _) => grid::imply(eval_expr(a), eval_expr(b)),
        Expr::Coimply(a, b, _) => grid::coimp(eval_expr(a), eval_expr(b)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::lexer::tokenize;
    use crate::dsl::parser::parse_tokens;
    use crate::time::tbase::TBase;

    fn eval(s: &str) -> TrackSpec {
        let tokens = tokenize(s).unwrap();
        let ast = parse_tokens(&tokens, s).unwrap();
        eval_track(&ast)
    }

    #[test]
    fn atom() {
        assert_eq!(eval("T16").grid, Grid::T16);
    }

    #[test]
    fn meet_t16_t8() {
        // meet(T16, T8) = GCD = T16 (finer)
        assert_eq!(eval("T16&T8").grid, grid::meet(Grid::T16, Grid::T8));
    }

    #[test]
    fn join_t16_t8() {
        // join(T16, T8) = LCM = T8 (coarser)
        assert_eq!(eval("T16|T8").grid, grid::join(Grid::T16, Grid::T8));
    }

    #[test]
    fn meet_cross_track() {
        assert_eq!(eval("T16t&T16q").grid, Grid::T16P);
    }

    #[test]
    fn join_cross_track() {
        assert_eq!(eval("T16t|T16q").grid, Grid::T8);
    }

    #[test]
    fn imply() {
        assert_eq!(
            eval("T16>T8").grid,
            grid::imply(Grid::T16, Grid::T8)
        );
    }

    #[test]
    fn coimply() {
        assert_eq!(
            eval("T16<T8").grid,
            grid::coimp(Grid::T16, Grid::T8)
        );
    }

    #[test]
    fn neg() {
        assert_eq!(eval("!T16").grid, grid::neg(Grid::T16));
    }

    #[test]
    fn swing_modifier() {
        let spec = eval("T16~T16:80");
        assert_eq!(
            spec.swing,
            Some(SwingConfig {
                resolution: TBase::T16,
                amount: 80,
            })
        );
    }

    #[test]
    fn offset_modifier() {
        let spec = eval("T16@-5");
        assert_eq!(spec.offset_ticks, Some(-5));
    }

    #[test]
    fn no_modifiers_are_none() {
        let spec = eval("T16");
        assert_eq!(spec.swing, None);
        assert_eq!(spec.offset_ticks, None);
    }
}
