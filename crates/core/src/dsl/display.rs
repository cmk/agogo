//! `Display` impls for AST nodes (precedence-aware parenthesization).
//!
//! Used by the `display_parse_round_trip` proptest: for any well-formed
//! AST, `eval(parse(display(ast))) == eval(ast)`.

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
            Expr::Atom(..) => 4,
        }
    }

    fn fmt_with_prec(&self, f: &mut fmt::Formatter<'_>, parent_prec: u8) -> fmt::Result {
        let need_parens = self.prec() < parent_prec;
        if need_parens {
            write!(f, "(")?;
        }
        match self {
            Expr::Atom(g, _) => write!(f, "{g}")?,
            Expr::Neg(inner, _) => {
                write!(f, "!")?;
                inner.fmt_with_prec(f, 3)?;
            }
            Expr::Meet(a, b, _) => {
                a.fmt_with_prec(f, 2)?;
                write!(f, "&")?;
                // Right child needs prec+1 for left-associativity.
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

impl fmt::Display for Modifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Modifier::Swing(res, amount, _) => write!(f, "~{res}:{amount}"),
            Modifier::Offset(ticks, _) => write!(f, "@{ticks}"),
        }
    }
}

impl fmt::Display for TrackAst {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.expr)?;
        for m in &self.modifiers {
            write!(f, "{m}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::grid::Grid;
    use crate::time::tbase::TBase;

    fn s(start: usize, end: usize) -> Span {
        Span { start, end }
    }

    fn atom(g: Grid) -> Expr {
        Expr::Atom(g, s(0, 0))
    }

    #[test]
    fn atom_display() {
        assert_eq!(atom(Grid::T16).to_string(), "t16");
    }

    #[test]
    fn meet_display() {
        let e = Expr::Meet(Box::new(atom(Grid::T16)), Box::new(atom(Grid::T8)), s(0, 0));
        assert_eq!(e.to_string(), "t16&t8");
    }

    #[test]
    fn join_display() {
        let e = Expr::Join(Box::new(atom(Grid::T16)), Box::new(atom(Grid::T8)), s(0, 0));
        assert_eq!(e.to_string(), "t16|t8");
    }

    #[test]
    fn imply_display() {
        let e = Expr::Imply(Box::new(atom(Grid::T16)), Box::new(atom(Grid::T8)), s(0, 0));
        assert_eq!(e.to_string(), "t16>t8");
    }

    #[test]
    fn coimply_display() {
        let e = Expr::Coimply(Box::new(atom(Grid::T16)), Box::new(atom(Grid::T8)), s(0, 0));
        assert_eq!(e.to_string(), "t16<t8");
    }

    #[test]
    fn neg_display() {
        let e = Expr::Neg(Box::new(atom(Grid::T16)), s(0, 0));
        assert_eq!(e.to_string(), "!t16");
    }

    #[test]
    fn parens_when_join_inside_meet() {
        // (T16|T8)&T4
        let inner = Expr::Join(Box::new(atom(Grid::T16)), Box::new(atom(Grid::T8)), s(0, 0));
        let e = Expr::Meet(Box::new(inner), Box::new(atom(Grid::T4)), s(0, 0));
        assert_eq!(e.to_string(), "(t16|t8)&t4");
    }

    #[test]
    fn no_parens_when_meet_inside_join() {
        // T16&T8|T4 — meet binds tighter, no parens needed
        let inner = Expr::Meet(Box::new(atom(Grid::T16)), Box::new(atom(Grid::T8)), s(0, 0));
        let e = Expr::Join(Box::new(inner), Box::new(atom(Grid::T4)), s(0, 0));
        assert_eq!(e.to_string(), "t16&t8|t4");
    }

    #[test]
    fn parens_when_imply_inside_meet() {
        // (T16>T8)&T4
        let inner = Expr::Imply(Box::new(atom(Grid::T16)), Box::new(atom(Grid::T8)), s(0, 0));
        let e = Expr::Meet(Box::new(inner), Box::new(atom(Grid::T4)), s(0, 0));
        assert_eq!(e.to_string(), "(t16>t8)&t4");
    }

    #[test]
    fn no_parens_join_inside_imply() {
        // T16|T8>T4 — join binds tighter than imply
        let inner = Expr::Join(Box::new(atom(Grid::T16)), Box::new(atom(Grid::T8)), s(0, 0));
        let e = Expr::Imply(Box::new(inner), Box::new(atom(Grid::T4)), s(0, 0));
        assert_eq!(e.to_string(), "t16|t8>t4");
    }

    #[test]
    fn swing_modifier_display() {
        let m = Modifier::Swing(TBase::T16, 80, s(0, 0));
        assert_eq!(m.to_string(), "~t16:80");
    }

    #[test]
    fn offset_modifier_display() {
        let m = Modifier::Offset(-5, s(0, 0));
        assert_eq!(m.to_string(), "@-5");
    }

    #[test]
    fn track_display() {
        let t = TrackAst {
            expr: atom(Grid::T16),
            modifiers: vec![
                Modifier::Swing(TBase::T16, 80, s(0, 0)),
                Modifier::Offset(-5, s(0, 0)),
            ],
            span: s(0, 0),
        };
        assert_eq!(t.to_string(), "t16~t16:80@-5");
    }
}
