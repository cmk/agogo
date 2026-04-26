//! AST → [`Grid`] evaluator.
//!
//! Trivially recursive on [`Expr`]: each variant maps to the
//! corresponding lattice operation on [`Grid`]. Variables are looked
//! up in an environment of previously-resolved channel grids.

use super::ast::*;
use super::error::{DslError, DslErrorKind};
use crate::time::grid::{self, Grid};

/// Evaluate a grid expression to a [`Grid`], resolving any variable
/// references against `env`.
///
/// Variable lookup is case-sensitive. Returns
/// `DslError::UnknownVariable` if a name is not found.
pub fn eval_expr(expr: &Expr, env: &[(String, Grid)], source: &str) -> Result<Grid, DslError> {
    match expr {
        Expr::Atom(g, _) => Ok(*g),
        Expr::Var(name, span) => env
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, g)| *g)
            .ok_or_else(|| DslError {
                kind: DslErrorKind::UnknownVariable(name.clone()),
                span: *span,
                source: source.to_string(),
            }),
        Expr::Neg(inner, _) => Ok(grid::neg(eval_expr(inner, env, source)?)),
        Expr::Meet(a, b, _) => {
            Ok(grid::meet(eval_expr(a, env, source)?, eval_expr(b, env, source)?))
        }
        Expr::Join(a, b, _) => {
            Ok(grid::join(eval_expr(a, env, source)?, eval_expr(b, env, source)?))
        }
        Expr::Imply(a, b, _) => {
            Ok(grid::imply(eval_expr(a, env, source)?, eval_expr(b, env, source)?))
        }
        Expr::Coimply(a, b, _) => {
            Ok(grid::coimp(eval_expr(a, env, source)?, eval_expr(b, env, source)?))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::lexer::tokenize;
    use crate::dsl::parser::parse_tokens;

    fn eval(s: &str, env: &[(String, Grid)]) -> Result<Grid, DslError> {
        let tokens = tokenize(s)?;
        let expr = parse_tokens(&tokens, s)?;
        eval_expr(&expr, env, s)
    }

    #[test]
    fn atom() {
        assert_eq!(eval("T16", &[]).unwrap(), Grid::T16);
    }

    #[test]
    fn meet() {
        assert_eq!(
            eval("T16&T8", &[]).unwrap(),
            grid::meet(Grid::T16, Grid::T8)
        );
    }

    #[test]
    fn join() {
        assert_eq!(
            eval("T16|T8", &[]).unwrap(),
            grid::join(Grid::T16, Grid::T8)
        );
    }

    #[test]
    fn imply() {
        assert_eq!(
            eval("T16>T8", &[]).unwrap(),
            grid::imply(Grid::T16, Grid::T8)
        );
    }

    #[test]
    fn coimply() {
        assert_eq!(
            eval("T16<T8", &[]).unwrap(),
            grid::coimp(Grid::T16, Grid::T8)
        );
    }

    #[test]
    fn neg() {
        assert_eq!(eval("!T16", &[]).unwrap(), grid::neg(Grid::T16));
    }

    #[test]
    fn var_resolves() {
        let env = vec![("kick".to_string(), Grid::T4)];
        assert_eq!(eval("kick", &env).unwrap(), Grid::T4);
    }

    #[test]
    fn var_in_expr() {
        let env = vec![("kick".to_string(), Grid::T4)];
        assert_eq!(
            eval("kick&T16", &env).unwrap(),
            grid::meet(Grid::T4, Grid::T16)
        );
    }

    #[test]
    fn positional_var() {
        let env = vec![("C1".to_string(), Grid::T8)];
        assert_eq!(eval("C1", &env).unwrap(), Grid::T8);
    }

    #[test]
    fn unknown_var_errors() {
        let err = eval("unknown", &[]).unwrap_err();
        assert_eq!(
            err.kind,
            DslErrorKind::UnknownVariable("unknown".to_string())
        );
    }

    #[test]
    fn forward_ref_errors() {
        // C1 is defined but C2 is not — strict monotonicity.
        let env = vec![("C1".to_string(), Grid::T4)];
        let err = eval("C2&C1", &env).unwrap_err();
        assert_eq!(
            err.kind,
            DslErrorKind::UnknownVariable("C2".to_string())
        );
    }

    #[test]
    fn cross_track_meet() {
        assert_eq!(eval("T16t&T16q", &[]).unwrap(), Grid::T16P);
    }

    #[test]
    fn cross_track_join() {
        assert_eq!(eval("T16t|T16q", &[]).unwrap(), Grid::T8);
    }
}
