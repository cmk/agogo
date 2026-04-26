//! AST types for the polyrhythm DSL.
//!
//! The DSL is pure grid algebra — it evaluates to a [`Grid`]. Swing,
//! offset, and delay are channel-level parameters in `ChannelSpec`,
//! not part of the DSL grammar.

use crate::time::grid::Grid;

/// Byte-offset span into the source string: `[start, end)`.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

/// A grid expression in the DSL.
#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    /// A grid literal: `T16`, `T8q`, `T2p`, etc.
    Atom(Grid, Span),
    /// A channel variable reference: `kick`, `C1`, etc.
    Var(String, Span),
    /// Heyting negation (pseudo-complement): `!a`
    Neg(Box<Expr>, Span),
    /// Meet (GCD / common refinement): `a & b`
    Meet(Box<Expr>, Box<Expr>, Span),
    /// Join (LCM / alignment intersection): `a | b`
    Join(Box<Expr>, Box<Expr>, Span),
    /// Heyting implication: `a > b`
    Imply(Box<Expr>, Box<Expr>, Span),
    /// Co-Heyting coimplication: `a < b`
    Coimply(Box<Expr>, Box<Expr>, Span),
}

impl Expr {
    /// The source span of this expression.
    pub fn span(&self) -> Span {
        match self {
            Expr::Atom(_, s)
            | Expr::Var(_, s)
            | Expr::Neg(_, s)
            | Expr::Meet(_, _, s)
            | Expr::Join(_, _, s)
            | Expr::Imply(_, _, s)
            | Expr::Coimply(_, _, s) => *s,
        }
    }
}
