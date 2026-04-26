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

    fn span_from(&self, start: usize) -> Span {
        let end = if self.pos > 0 {
            self.tokens[self.pos - 1].span.end
        } else {
            start
        };
        Span { start, end }
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

    /// track := expr modifier*
    fn parse_track(&mut self) -> Result<TrackAst, DslError> {
        let start = self
            .peek()
            .map(|t| t.span.start)
            .unwrap_or_else(|| self.source.len());
        let expr = self.parse_expr()?;
        let mut modifiers = Vec::new();
        let mut has_swing = false;
        let mut has_offset = false;

        while let Some(tok) = self.peek() {
            match &tok.kind {
                TokenKind::Tilde => {
                    if has_swing {
                        let span = tok.span;
                        return Err(self.err(DslErrorKind::DuplicateModifier("swing"), span));
                    }
                    let m = self.parse_swing()?;
                    has_swing = true;
                    modifiers.push(m);
                }
                TokenKind::At => {
                    if has_offset {
                        let span = tok.span;
                        return Err(self.err(DslErrorKind::DuplicateModifier("offset"), span));
                    }
                    let m = self.parse_offset()?;
                    has_offset = true;
                    modifiers.push(m);
                }
                _ => break,
            }
        }

        let span = self.span_from(start);
        Ok(TrackAst {
            expr,
            modifiers,
            span,
        })
    }

    /// expr := impl_expr
    fn parse_expr(&mut self) -> Result<Expr, DslError> {
        self.parse_impl_expr()
    }

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
                    return Err(self.err(DslErrorKind::NestingTooDeep, Span { start, end: start + 1 }));
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

    /// primary := atom | '(' expr ')'
    fn parse_primary(&mut self) -> Result<Expr, DslError> {
        let tok = self.peek().ok_or_else(|| {
            self.err(DslErrorKind::UnexpectedEof, self.eof_span())
        })?;

        match &tok.kind {
            TokenKind::Atom(text) => {
                let span = tok.span;
                let grid = text.parse().map_err(|e: String| {
                    self.err(DslErrorKind::InvalidAtom(e), span)
                })?;
                self.advance();
                Ok(Expr::Atom(grid, span))
            }
            TokenKind::LParen => {
                let start = tok.span.start;
                self.advance();
                self.depth += 1;
                if self.depth > MAX_DEPTH {
                    return Err(self.err(DslErrorKind::NestingTooDeep, Span { start, end: start + 1 }));
                }
                let inner = self.parse_expr()?;
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
                        expected: "atom or '('",
                        found,
                    },
                    span,
                ))
            }
        }
    }

    // ── Modifier parsing ─────────────────────────────────────────

    /// swing := '~' <TBase> ':' <i8>
    fn parse_swing(&mut self) -> Result<Modifier, DslError> {
        let start = self.advance().span.start; // consume '~'

        // Expect TBase atom.
        let res_tok = self.peek().ok_or_else(|| {
            self.err(DslErrorKind::UnexpectedEof, self.eof_span())
        })?;
        let (res_text, res_span) = match &res_tok.kind {
            TokenKind::Atom(text) => (text.clone(), res_tok.span),
            _ => {
                let span = res_tok.span;
                return Err(self.err(
                    DslErrorKind::Expected {
                        expected: "swing resolution (e.g. T16)",
                        found: format!("{:?}", res_tok.kind),
                    },
                    span,
                ));
            }
        };
        self.advance();

        let resolution = res_text.parse().map_err(|e: String| {
            self.err(DslErrorKind::InvalidSwingResolution(e), res_span)
        })?;

        // Expect ':'.
        if !self.eat(&TokenKind::Colon) {
            let span = self
                .peek()
                .map(|t| t.span)
                .unwrap_or_else(|| self.eof_span());
            return Err(self.err(
                DslErrorKind::Expected {
                    expected: "':'",
                    found: self
                        .peek()
                        .map(|t| format!("{:?}", t.kind))
                        .unwrap_or_else(|| "end of input".into()),
                },
                span,
            ));
        }

        // Expect i8 amount.
        let amt_tok = self.peek().ok_or_else(|| {
            self.err(DslErrorKind::UnexpectedEof, self.eof_span())
        })?;
        let (amt_val, amt_span) = match &amt_tok.kind {
            TokenKind::Int(n) => (*n, amt_tok.span),
            _ => {
                let span = amt_tok.span;
                return Err(self.err(
                    DslErrorKind::Expected {
                        expected: "swing amount (integer)",
                        found: format!("{:?}", amt_tok.kind),
                    },
                    span,
                ));
            }
        };
        self.advance();

        if amt_val < i8::MIN as i64 || amt_val > i8::MAX as i64 {
            return Err(self.err(DslErrorKind::SwingAmountOutOfRange(amt_val), amt_span));
        }

        let span = Span {
            start,
            end: amt_span.end,
        };
        Ok(Modifier::Swing(resolution, amt_val as i8, span))
    }

    /// offset := '@' <i32>
    fn parse_offset(&mut self) -> Result<Modifier, DslError> {
        let start = self.advance().span.start; // consume '@'

        let val_tok = self.peek().ok_or_else(|| {
            self.err(DslErrorKind::UnexpectedEof, self.eof_span())
        })?;
        let (val, val_span) = match &val_tok.kind {
            TokenKind::Int(n) => (*n, val_tok.span),
            _ => {
                let span = val_tok.span;
                return Err(self.err(
                    DslErrorKind::Expected {
                        expected: "offset value (integer)",
                        found: format!("{:?}", val_tok.kind),
                    },
                    span,
                ));
            }
        };
        self.advance();

        if val < i32::MIN as i64 || val > i32::MAX as i64 {
            return Err(self.err(DslErrorKind::OffsetOutOfRange(val), val_span));
        }

        let span = Span {
            start,
            end: val_span.end,
        };
        Ok(Modifier::Offset(val as i32, span))
    }
}

/// Parse a token stream into a [`TrackAst`]. Returns an error if
/// tokens remain after the track is fully parsed.
pub fn parse_tokens(tokens: &[Token], source: &str) -> Result<TrackAst, DslError> {
    let mut parser = Parser::new(tokens, source);

    if tokens.is_empty() {
        return Err(parser.err(DslErrorKind::EmptyInput, parser.eof_span()));
    }

    let track = parser.parse_track()?;

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

    Ok(track)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::lexer::tokenize;
    use crate::time::grid::Grid;
    use crate::time::tbase::TBase;

    fn parse(s: &str) -> Result<TrackAst, DslError> {
        let tokens = tokenize(s)?;
        parse_tokens(&tokens, s)
    }

    fn expr(s: &str) -> Expr {
        parse(s).unwrap().expr
    }

    fn grid_of(e: &Expr) -> Grid {
        match e {
            Expr::Atom(g, _) => *g,
            _ => panic!("expected Atom, got {e:?}"),
        }
    }

    // ── Atoms ────────────────────────────────────────────────────

    #[test]
    fn single_atom() {
        assert_eq!(grid_of(&expr("T16")), Grid::T16);
    }

    #[test]
    fn atom_triplet() {
        assert_eq!(grid_of(&expr("t16t")), Grid::T16T);
    }

    #[test]
    fn atom_quintuplet() {
        assert_eq!(grid_of(&expr("T8Q")), Grid::T8Q);
    }

    // ── Binary ops ──────────────────────────────────────────────

    #[test]
    fn meet() {
        match expr("T16&T8") {
            Expr::Meet(a, b, _) => {
                assert_eq!(grid_of(&a), Grid::T16);
                assert_eq!(grid_of(&b), Grid::T8);
            }
            other => panic!("expected Meet, got {other:?}"),
        }
    }

    #[test]
    fn join() {
        match expr("T16|T8") {
            Expr::Join(a, b, _) => {
                assert_eq!(grid_of(&a), Grid::T16);
                assert_eq!(grid_of(&b), Grid::T8);
            }
            other => panic!("expected Join, got {other:?}"),
        }
    }

    #[test]
    fn imply() {
        match expr("T16>T8") {
            Expr::Imply(a, b, _) => {
                assert_eq!(grid_of(&a), Grid::T16);
                assert_eq!(grid_of(&b), Grid::T8);
            }
            other => panic!("expected Imply, got {other:?}"),
        }
    }

    #[test]
    fn coimply() {
        match expr("T16<T8") {
            Expr::Coimply(a, b, _) => {
                assert_eq!(grid_of(&a), Grid::T16);
                assert_eq!(grid_of(&b), Grid::T8);
            }
            other => panic!("expected Coimply, got {other:?}"),
        }
    }

    // ── Negation ────────────────────────────────────────────────

    #[test]
    fn neg() {
        match expr("!T16") {
            Expr::Neg(inner, _) => assert_eq!(grid_of(&inner), Grid::T16),
            other => panic!("expected Neg, got {other:?}"),
        }
    }

    #[test]
    fn double_neg() {
        match expr("!!T16") {
            Expr::Neg(inner, _) => match *inner {
                Expr::Neg(inner2, _) => assert_eq!(grid_of(&inner2), Grid::T16),
                other => panic!("expected Neg(Neg), got {other:?}"),
            },
            other => panic!("expected Neg, got {other:?}"),
        }
    }

    // ── Precedence ──────────────────────────────────────────────

    #[test]
    fn meet_binds_tighter_than_join() {
        // T16|T8&T4 → Join(T16, Meet(T8, T4))
        match expr("T16|T8&T4") {
            Expr::Join(a, b, _) => {
                assert_eq!(grid_of(&a), Grid::T16);
                match *b {
                    Expr::Meet(c, d, _) => {
                        assert_eq!(grid_of(&c), Grid::T8);
                        assert_eq!(grid_of(&d), Grid::T4);
                    }
                    other => panic!("expected Meet, got {other:?}"),
                }
            }
            other => panic!("expected Join, got {other:?}"),
        }
    }

    #[test]
    fn join_binds_tighter_than_imply() {
        // T16|T8>T4 → Imply(Join(T16, T8), T4)
        match expr("T16|T8>T4") {
            Expr::Imply(a, b, _) => {
                match *a {
                    Expr::Join(c, d, _) => {
                        assert_eq!(grid_of(&c), Grid::T16);
                        assert_eq!(grid_of(&d), Grid::T8);
                    }
                    other => panic!("expected Join, got {other:?}"),
                }
                assert_eq!(grid_of(&b), Grid::T4);
            }
            other => panic!("expected Imply, got {other:?}"),
        }
    }

    #[test]
    fn neg_binds_tighter_than_meet() {
        // !T16&T8 → Meet(Neg(T16), T8)
        match expr("!T16&T8") {
            Expr::Meet(a, b, _) => {
                match *a {
                    Expr::Neg(inner, _) => assert_eq!(grid_of(&inner), Grid::T16),
                    other => panic!("expected Neg, got {other:?}"),
                }
                assert_eq!(grid_of(&b), Grid::T8);
            }
            other => panic!("expected Meet, got {other:?}"),
        }
    }

    #[test]
    fn left_assoc_meet() {
        // T16&T8&T4 → Meet(Meet(T16, T8), T4)
        match expr("T16&T8&T4") {
            Expr::Meet(a, b, _) => {
                assert_eq!(grid_of(&b), Grid::T4);
                match *a {
                    Expr::Meet(c, d, _) => {
                        assert_eq!(grid_of(&c), Grid::T16);
                        assert_eq!(grid_of(&d), Grid::T8);
                    }
                    other => panic!("expected inner Meet, got {other:?}"),
                }
            }
            other => panic!("expected Meet, got {other:?}"),
        }
    }

    #[test]
    fn parens_override_precedence() {
        // (T16|T8)&T4 → Meet(Join(T16, T8), T4)
        match expr("(T16|T8)&T4") {
            Expr::Meet(a, b, _) => {
                match *a {
                    Expr::Join(c, d, _) => {
                        assert_eq!(grid_of(&c), Grid::T16);
                        assert_eq!(grid_of(&d), Grid::T8);
                    }
                    other => panic!("expected Join, got {other:?}"),
                }
                assert_eq!(grid_of(&b), Grid::T4);
            }
            other => panic!("expected Meet, got {other:?}"),
        }
    }

    // ── Modifiers ───────────────────────────────────────────────

    #[test]
    fn swing_modifier() {
        let t = parse("T16~T16:80").unwrap();
        assert_eq!(t.modifiers.len(), 1);
        match &t.modifiers[0] {
            Modifier::Swing(res, amt, _) => {
                assert_eq!(*res, TBase::T16);
                assert_eq!(*amt, 80);
            }
            other => panic!("expected Swing, got {other:?}"),
        }
    }

    #[test]
    fn offset_modifier() {
        let t = parse("T16@-5").unwrap();
        assert_eq!(t.modifiers.len(), 1);
        match &t.modifiers[0] {
            Modifier::Offset(v, _) => assert_eq!(*v, -5),
            other => panic!("expected Offset, got {other:?}"),
        }
    }

    #[test]
    fn both_modifiers() {
        let t = parse("T16~T16:80@-5").unwrap();
        assert_eq!(t.modifiers.len(), 2);
    }

    // ── Error cases ─────────────────────────────────────────────

    #[test]
    fn empty_input() {
        let err = parse("").unwrap_err();
        assert_eq!(err.kind, DslErrorKind::EmptyInput);
    }

    #[test]
    fn whitespace_only() {
        let err = parse("   ").unwrap_err();
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
    fn duplicate_swing() {
        let err = parse("T16~T16:80~T8:40").unwrap_err();
        assert_eq!(err.kind, DslErrorKind::DuplicateModifier("swing"));
    }

    #[test]
    fn duplicate_offset() {
        let err = parse("T16@5@10").unwrap_err();
        assert_eq!(err.kind, DslErrorKind::DuplicateModifier("offset"));
    }

    #[test]
    fn invalid_atom() {
        let err = parse("T3").unwrap_err();
        assert!(matches!(err.kind, DslErrorKind::InvalidAtom(_)));
    }

    #[test]
    fn nesting_depth_exceeded() {
        // 33 consecutive `!` exceeds MAX_DEPTH (32).
        let s = format!("{}T16", "!".repeat(33));
        let err = parse(&s).unwrap_err();
        assert_eq!(err.kind, DslErrorKind::NestingTooDeep);
    }

    #[test]
    fn nesting_depth_at_limit_succeeds() {
        // 32 consecutive `!` is exactly at the limit — should succeed.
        let s = format!("{}T16", "!".repeat(32));
        assert!(parse(&s).is_ok());
    }

    // ── Proptest ─────────────────────────────────────────────────

    use proptest::prelude::*;

    proptest! {
        /// Parser never panics on arbitrary input.
        #[test]
        fn parser_never_panics(s in ".*") {
            let _ = parse(&s);
        }
    }
}
