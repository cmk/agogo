//! `Display` impls for AST nodes (precedence-aware parenthesization).
//!
//! Used by the `display_parse_round_trip` proptest: for any well-formed
//! AST, `parse(display(expr), env)` recovers the same `Grid`.

use std::fmt;

use super::ast::*;

impl Expr {
    /// Precedence level of this expression node.
    /// Higher = tighter binding.
    fn prec(&self) -> u8 {
        match self {
            Expr::Imply(..) | Expr::Coimply(..) => 0,
            Expr::Join(..) => 1,
            Expr::Meet(..) => 2,
            Expr::Neg(..) => 3,
            Expr::Atom(..) | Expr::Var(..) => 4,
        }
    }

    fn fmt_with_prec(&self, f: &mut fmt::Formatter<'_>, parent_prec: u8) -> fmt::Result {
        let need_parens = self.prec() < parent_prec;
        if need_parens {
            write!(f, "(")?;
        }
        match self {
            Expr::Atom(g, _) => write!(f, "{g}")?,
            Expr::Var(name, _) => write!(f, "{name}")?,
            Expr::Neg(inner, _) => {
                write!(f, "!")?;
                inner.fmt_with_prec(f, 3)?;
            }
            Expr::Meet(a, b, _) => {
                a.fmt_with_prec(f, 2)?;
                write!(f, "&")?;
                b.fmt_with_prec(f, 3)?;
            }
            Expr::Join(a, b, _) => {
                a.fmt_with_prec(f, 1)?;
                write!(f, "|")?;
                b.fmt_with_prec(f, 2)?;
            }
            Expr::Imply(a, b, _) => {
                a.fmt_with_prec(f, 0)?;
                write!(f, ">")?;
                b.fmt_with_prec(f, 1)?;
            }
            Expr::Coimply(a, b, _) => {
                a.fmt_with_prec(f, 0)?;
                write!(f, "<")?;
                b.fmt_with_prec(f, 1)?;
            }
        }
        if need_parens {
            write!(f, ")")?;
        }
        Ok(())
    }
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.fmt_with_prec(f, 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::grid::Grid;

    fn s(start: usize, end: usize) -> Span {
        Span { start, end }
    }

    fn atom(g: Grid) -> Expr {
        Expr::Atom(g, s(0, 0))
    }

    fn var(name: &str) -> Expr {
        Expr::Var(name.to_string(), s(0, 0))
    }

    #[test]
    fn atom_display() {
        assert_eq!(atom(Grid::T16).to_string(), "t16");
    }

    #[test]
    fn var_display() {
        assert_eq!(var("kick").to_string(), "kick");
        assert_eq!(var("C1").to_string(), "C1");
    }

    #[test]
    fn meet_display() {
        let e = Expr::Meet(Box::new(var("kick")), Box::new(atom(Grid::T16)), s(0, 0));
        assert_eq!(e.to_string(), "kick&t16");
    }

    #[test]
    fn join_display() {
        let e = Expr::Join(Box::new(atom(Grid::T16)), Box::new(atom(Grid::T8)), s(0, 0));
        assert_eq!(e.to_string(), "t16|t8");
    }

    #[test]
    fn neg_display() {
        let e = Expr::Neg(Box::new(atom(Grid::T16)), s(0, 0));
        assert_eq!(e.to_string(), "!t16");
    }

    #[test]
    fn parens_when_join_inside_meet() {
        let inner = Expr::Join(Box::new(atom(Grid::T16)), Box::new(atom(Grid::T8)), s(0, 0));
        let e = Expr::Meet(Box::new(inner), Box::new(atom(Grid::T4)), s(0, 0));
        assert_eq!(e.to_string(), "(t16|t8)&t4");
    }

    #[test]
    fn no_parens_when_meet_inside_join() {
        let inner = Expr::Meet(Box::new(atom(Grid::T16)), Box::new(atom(Grid::T8)), s(0, 0));
        let e = Expr::Join(Box::new(inner), Box::new(atom(Grid::T4)), s(0, 0));
        assert_eq!(e.to_string(), "t16&t8|t4");
    }

    #[test]
    fn parens_when_imply_inside_meet() {
        let inner = Expr::Imply(Box::new(atom(Grid::T16)), Box::new(atom(Grid::T8)), s(0, 0));
        let e = Expr::Meet(Box::new(inner), Box::new(atom(Grid::T4)), s(0, 0));
        assert_eq!(e.to_string(), "(t16>t8)&t4");
    }

    #[test]
    fn var_in_complex_expr() {
        let inner = Expr::Meet(Box::new(var("kick")), Box::new(atom(Grid::T16)), s(0, 0));
        let e = Expr::Join(Box::new(inner), Box::new(var("C2")), s(0, 0));
        assert_eq!(e.to_string(), "kick&t16|C2");
    }
}
