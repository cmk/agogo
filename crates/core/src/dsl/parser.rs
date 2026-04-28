//! Recursive descent parser: tokens → AST.
//!
//! Precedence (tightest to loosest):
//!   1. `!`   prefix negation
//!   2. `&`   meet / GCD
//!   3. `|`   join / LCM
//!   4. `>`/`<` imply / coimply
//!
//! All binary operators are left-associative. Max nesting depth 32.

use super::ast::*;
use super::error::{DslError, DslErrorKind};
use super::lexer::{Token, TokenKind};

const MAX_DEPTH: usize = 32;

struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
    source: &'a str,
    depth: usize,
}

impl<'a> Parser<'a> {
    fn new(tokens: &'a [Token], source: &'a str) -> Self {
        Parser {
            tokens,
            pos: 0,
            source,
            depth: 0,
        }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn advance(&mut self) -> &Token {
        let tok = &self.tokens[self.pos];
        self.pos += 1;
        tok
    }

    fn at_end(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    fn eat(&mut self, kind: &TokenKind) -> bool {
        if let Some(tok) = self.peek() {
            if std::mem::discriminant(&tok.kind) == std::mem::discriminant(kind) {
                self.pos += 1;
                return true;
            }
        }
        false
    }

    fn eof_span(&self) -> Span {
        let pos = self.source.len();
        Span {
            start: pos,
            end: pos,
        }
    }

    fn err(&self, kind: DslErrorKind, span: Span) -> DslError {
        DslError {
            kind,
            span,
            source: self.source.to_string(),
        }
    }

    // ── Grammar productions ──────────────────────────────────────

    /// impl_expr := join_expr ( ('>' | '<') join_expr )*
    fn parse_impl_expr(&mut self) -> Result<Expr, DslError> {
        let mut left = self.parse_join_expr()?;
        loop {
            let op = match self.peek().map(|t| &t.kind) {
                Some(TokenKind::Gt) => true,
                Some(TokenKind::Lt) => false,
                _ => break,
            };
            self.advance();
            let right = self.parse_join_expr()?;
            let span = Span {
                start: left.span().start,
                end: right.span().end,
            };
            left = if op {
                Expr::Imply(Box::new(left), Box::new(right), span)
            } else {
                Expr::Coimply(Box::new(left), Box::new(right), span)
            };
        }
        Ok(left)
    }

    /// join_expr := meet_expr ( '|' meet_expr )*
    fn parse_join_expr(&mut self) -> Result<Expr, DslError> {
        let mut left = self.parse_meet_expr()?;
        while self.eat(&TokenKind::Pipe) {
            let right = self.parse_meet_expr()?;
            let span = Span {
                start: left.span().start,
                end: right.span().end,
            };
            left = Expr::Join(Box::new(left), Box::new(right), span);
        }
        Ok(left)
    }

    /// meet_expr := unary ( '&' unary )*
    fn parse_meet_expr(&mut self) -> Result<Expr, DslError> {
        let mut left = self.parse_unary()?;
        while self.eat(&TokenKind::Ampersand) {
            let right = self.parse_unary()?;
            let span = Span {
                start: left.span().start,
                end: right.span().end,
            };
            left = Expr::Meet(Box::new(left), Box::new(right), span);
        }
        Ok(left)
    }

    /// unary := '!' unary | primary
    fn parse_unary(&mut self) -> Result<Expr, DslError> {
        if let Some(tok) = self.peek() {
            if matches!(tok.kind, TokenKind::Bang) {
                let start = tok.span.start;
                self.advance();
                self.depth += 1;
                if self.depth > MAX_DEPTH {
                    return Err(self.err(
                        DslErrorKind::NestingTooDeep,
                        Span {
                            start,
                            end: start + 1,
                        },
                    ));
                }
                let inner = self.parse_unary()?;
                self.depth -= 1;
                let span = Span {
                    start,
                    end: inner.span().end,
                };
                return Ok(Expr::Neg(Box::new(inner), span));
            }
        }
        self.parse_primary()
    }

    /// primary := ident | '(' expr ')'
    ///
    /// An ident is tried as a `Grid` name first (via `Grid::from_str`);
    /// if that fails, it's treated as a variable reference.
    fn parse_primary(&mut self) -> Result<Expr, DslError> {
        let tok = self
            .peek()
            .ok_or_else(|| self.err(DslErrorKind::UnexpectedEof, self.eof_span()))?;

        match &tok.kind {
            TokenKind::Ident(text) => {
                let span = tok.span;
                let text = text.clone();
                self.advance();
                // Try as grid name first; fall back to variable.
                match text.parse() {
                    Ok(grid) => Ok(Expr::Atom(grid, span)),
                    Err(_) => Ok(Expr::Var(text, span)),
                }
            }
            TokenKind::LParen => {
                let start = tok.span.start;
                self.advance();
                self.depth += 1;
                if self.depth > MAX_DEPTH {
                    return Err(self.err(
                        DslErrorKind::NestingTooDeep,
                        Span {
                            start,
                            end: start + 1,
                        },
                    ));
                }
                let inner = self.parse_impl_expr()?;
                self.depth -= 1;
                if !self.eat(&TokenKind::RParen) {
                    return Err(self.err(
                        DslErrorKind::UnmatchedParen,
                        Span {
                            start,
                            end: inner.span().end,
                        },
                    ));
                }
                Ok(inner)
            }
            _ => {
                let span = tok.span;
                let found = format!("{:?}", tok.kind);
                Err(self.err(
                    DslErrorKind::Expected {
                        expected: "identifier or '('",
                        found,
                    },
                    span,
                ))
            }
        }
    }
}

/// Parse a token stream into an [`Expr`]. Returns an error if
/// tokens remain after the expression is fully parsed.
pub fn parse_tokens(tokens: &[Token], source: &str) -> Result<Expr, DslError> {
    let mut parser = Parser::new(tokens, source);

    if tokens.is_empty() {
        return Err(parser.err(DslErrorKind::EmptyInput, parser.eof_span()));
    }

    let expr = parser.parse_impl_expr()?;

    if !parser.at_end() {
        let tok = &parser.tokens[parser.pos];
        return Err(parser.err(
            DslErrorKind::Expected {
                expected: "end of input",
                found: format!("{:?}", tok.kind),
            },
            tok.span,
        ));
    }

    Ok(expr)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::lexer::tokenize;
    use crate::time::grid::Grid;

    fn parse(s: &str) -> Result<Expr, DslError> {
        let tokens = tokenize(s)?;
        parse_tokens(&tokens, s)
    }

    fn grid_of(e: &Expr) -> Grid {
        match e {
            Expr::Atom(g, _) => *g,
            _ => panic!("expected Atom, got {e:?}"),
        }
    }

    fn var_of(e: &Expr) -> &str {
        match e {
            Expr::Var(name, _) => name,
            _ => panic!("expected Var, got {e:?}"),
        }
    }

    // ── Atoms ────────────────────────────────────────────────────

    #[test]
    fn single_atom() {
        assert_eq!(grid_of(&parse("T16").unwrap()), Grid::T16);
    }

    #[test]
    fn atom_triplet() {
        assert_eq!(grid_of(&parse("t16t").unwrap()), Grid::T16T);
    }

    // ── Variables ────────────────────────────────────────────────

    #[test]
    fn variable_name() {
        assert_eq!(var_of(&parse("kick").unwrap()), "kick");
    }

    #[test]
    fn positional_variable() {
        assert_eq!(var_of(&parse("C1").unwrap()), "C1");
    }

    #[test]
    fn var_in_expr() {
        match parse("kick&T16").unwrap() {
            Expr::Meet(a, b, _) => {
                assert_eq!(var_of(&a), "kick");
                assert_eq!(grid_of(&b), Grid::T16);
            }
            other => panic!("expected Meet, got {other:?}"),
        }
    }

    // ── Binary ops ──────────────────────────────────────────────

    #[test]
    fn meet() {
        match parse("T16&T8").unwrap() {
            Expr::Meet(a, b, _) => {
                assert_eq!(grid_of(&a), Grid::T16);
                assert_eq!(grid_of(&b), Grid::T8);
            }
            other => panic!("expected Meet, got {other:?}"),
        }
    }

    #[test]
    fn join() {
        match parse("T16|T8").unwrap() {
            Expr::Join(a, b, _) => {
                assert_eq!(grid_of(&a), Grid::T16);
                assert_eq!(grid_of(&b), Grid::T8);
            }
            other => panic!("expected Join, got {other:?}"),
        }
    }

    #[test]
    fn imply() {
        match parse("T16>T8").unwrap() {
            Expr::Imply(a, b, _) => {
                assert_eq!(grid_of(&a), Grid::T16);
                assert_eq!(grid_of(&b), Grid::T8);
            }
            other => panic!("expected Imply, got {other:?}"),
        }
    }

    #[test]
    fn coimply() {
        match parse("T16<T8").unwrap() {
            Expr::Coimply(a, b, _) => {
                assert_eq!(grid_of(&a), Grid::T16);
                assert_eq!(grid_of(&b), Grid::T8);
            }
            other => panic!("expected Coimply, got {other:?}"),
        }
    }

    #[test]
    fn neg() {
        match parse("!T16").unwrap() {
            Expr::Neg(inner, _) => assert_eq!(grid_of(&inner), Grid::T16),
            other => panic!("expected Neg, got {other:?}"),
        }
    }

    #[test]
    fn double_neg() {
        match parse("!!T16").unwrap() {
            Expr::Neg(inner, _) => match *inner {
                Expr::Neg(inner2, _) => assert_eq!(grid_of(&inner2), Grid::T16),
                other => panic!("expected inner Neg, got {other:?}"),
            },
            other => panic!("expected Neg, got {other:?}"),
        }
    }

    // ── Precedence ──────────────────────────────────────────────

    #[test]
    fn meet_binds_tighter_than_join() {
        match parse("T16|T8&T4").unwrap() {
            Expr::Join(a, b, _) => {
                assert_eq!(grid_of(&a), Grid::T16);
                assert!(matches!(*b, Expr::Meet(..)));
            }
            other => panic!("expected Join, got {other:?}"),
        }
    }

    #[test]
    fn join_binds_tighter_than_imply() {
        match parse("T16|T8>T4").unwrap() {
            Expr::Imply(a, b, _) => {
                assert!(matches!(*a, Expr::Join(..)));
                assert_eq!(grid_of(&b), Grid::T4);
            }
            other => panic!("expected Imply, got {other:?}"),
        }
    }

    #[test]
    fn neg_binds_tighter_than_meet() {
        match parse("!T16&T8").unwrap() {
            Expr::Meet(a, b, _) => {
                assert!(matches!(*a, Expr::Neg(..)));
                assert_eq!(grid_of(&b), Grid::T8);
            }
            other => panic!("expected Meet, got {other:?}"),
        }
    }

    #[test]
    fn left_assoc_meet() {
        match parse("T16&T8&T4").unwrap() {
            Expr::Meet(a, b, _) => {
                assert_eq!(grid_of(&b), Grid::T4);
                assert!(matches!(*a, Expr::Meet(..)));
            }
            other => panic!("expected Meet, got {other:?}"),
        }
    }

    #[test]
    fn parens_override_precedence() {
        match parse("(T16|T8)&T4").unwrap() {
            Expr::Meet(a, b, _) => {
                assert!(matches!(*a, Expr::Join(..)));
                assert_eq!(grid_of(&b), Grid::T4);
            }
            other => panic!("expected Meet, got {other:?}"),
        }
    }

    // ── Errors ──────────────────────────────────────────────────

    #[test]
    fn empty_input() {
        let err = parse("").unwrap_err();
        assert_eq!(err.kind, DslErrorKind::EmptyInput);
    }

    #[test]
    fn unmatched_paren() {
        let err = parse("(T16").unwrap_err();
        assert_eq!(err.kind, DslErrorKind::UnmatchedParen);
    }

    #[test]
    fn trailing_garbage() {
        let err = parse("T16 T8").unwrap_err();
        assert!(matches!(err.kind, DslErrorKind::Expected { .. }));
    }

    #[test]
    fn nesting_depth_exceeded() {
        let s = format!("{}T16", "!".repeat(33));
        let err = parse(&s).unwrap_err();
        assert_eq!(err.kind, DslErrorKind::NestingTooDeep);
    }

    #[test]
    fn nesting_at_limit_succeeds() {
        let s = format!("{}T16", "!".repeat(32));
        assert!(parse(&s).is_ok());
    }

    // ── Proptest ─────────────────────────────────────────────────

    use proptest::prelude::*;

    proptest! {
        #[test]
        fn parser_never_panics(s in ".*") {
            let _ = parse(&s);
        }
    }
}
