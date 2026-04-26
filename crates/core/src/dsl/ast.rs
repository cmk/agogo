//! AST types for the polyrhythm DSL.
//!
//! The DSL produces a single [`TrackSpec`] per expression (one channel).
//! Polyrhythm is achieved by specifying multiple `--ch` flags.

use crate::time::grid::Grid;
use crate::time::swing::SwingConfig;
use crate::time::tbase::TBase;

/// Byte-offset span into the source string: `[start, end)`.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

/// A grid expression in the DSL.
#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    /// A single grid atom: `T16`, `T8q`, `T2p`, etc.
    Atom(Grid, Span),
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
            | Expr::Neg(_, s)
            | Expr::Meet(_, _, s)
            | Expr::Join(_, _, s)
            | Expr::Imply(_, _, s)
            | Expr::Coimply(_, _, s) => *s,
        }
    }
}

/// A modifier attached to a track expression.
#[derive(Clone, Debug, PartialEq)]
pub enum Modifier {
    /// `~T16:80` — swing with explicit resolution and amount.
    Swing(TBase, i8, Span),
    /// `@-5` — musical offset in ticks (signed).
    Offset(i32, Span),
}

/// A single track: an expression plus zero or more modifiers.
#[derive(Clone, Debug, PartialEq)]
pub struct TrackAst {
    pub expr: Expr,
    pub modifiers: Vec<Modifier>,
    pub span: Span,
}

/// The evaluated output of a single track. This is what the DSL
/// parser's public API returns.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct TrackSpec {
    pub grid: Grid,
    pub swing: Option<SwingConfig>,
    pub offset_ticks: Option<i32>,
}
