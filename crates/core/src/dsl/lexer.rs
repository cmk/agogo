//! Single-pass lexer for the polyrhythm DSL.
//!
//! Tokenizes a source string into a `Vec<Token>`. Whitespace between
//! tokens is skipped; atoms (`T16`, `t8q`) are recognized greedily.
//! A `-` immediately preceding digits (no space) emits `Int(-N)`.

use super::ast::Span;
use super::error::{DslError, DslErrorKind};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TokenKind {
    /// Grid atom: `T16`, `T8q`, `T2p`, etc. Stores the raw text.
    Atom(String),
    /// `&` meet operator
    Ampersand,
    /// `|` join operator
    Pipe,
    /// `>` implication
    Gt,
    /// `<` coimplication
    Lt,
    /// `!` negation (prefix)
    Bang,
    /// `~` swing prefix
    Tilde,
    /// `@` offset prefix
    At,
    /// `:` separator (used in swing `~T16:80`)
    Colon,
    /// Integer literal (for modifier values). Negative when `-` is
    /// immediately followed by digits.
    Int(i64),
    /// `(`
    LParen,
    /// `)`
    RParen,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

/// Tokenize `input` into a sequence of [`Token`]s.
pub fn tokenize(input: &str) -> Result<Vec<Token>, DslError> {
    let src = input;
    let bytes = input.as_bytes();
    let mut pos = 0;
    let mut tokens = Vec::new();

    while pos < bytes.len() {
        // Skip whitespace.
        if bytes[pos].is_ascii_whitespace() {
            pos += 1;
            continue;
        }

        let start = pos;
        let ch = bytes[pos] as char;

        match ch {
            '&' => {
                tokens.push(tok(TokenKind::Ampersand, start, start + 1));
                pos += 1;
            }
            '|' => {
                tokens.push(tok(TokenKind::Pipe, start, start + 1));
                pos += 1;
            }
            '>' => {
                tokens.push(tok(TokenKind::Gt, start, start + 1));
                pos += 1;
            }
            '<' => {
                tokens.push(tok(TokenKind::Lt, start, start + 1));
                pos += 1;
            }
            '!' => {
                tokens.push(tok(TokenKind::Bang, start, start + 1));
                pos += 1;
            }
            '~' => {
                tokens.push(tok(TokenKind::Tilde, start, start + 1));
                pos += 1;
            }
            '@' => {
                tokens.push(tok(TokenKind::At, start, start + 1));
                pos += 1;
            }
            ':' => {
                tokens.push(tok(TokenKind::Colon, start, start + 1));
                pos += 1;
            }
            '(' => {
                tokens.push(tok(TokenKind::LParen, start, start + 1));
                pos += 1;
            }
            ')' => {
                tokens.push(tok(TokenKind::RParen, start, start + 1));
                pos += 1;
            }
            '-' if pos + 1 < bytes.len() && bytes[pos + 1].is_ascii_digit() => {
                // Negative integer: consume `-` then digits.
                pos += 1;
                let digit_start = pos;
                while pos < bytes.len() && bytes[pos].is_ascii_digit() {
                    pos += 1;
                }
                let digits = &input[digit_start..pos];
                let n: i64 = digits.parse().map_err(|_| DslError {
                    kind: DslErrorKind::Expected {
                        expected: "integer",
                        found: format!("-{digits}"),
                    },
                    span: Span { start, end: pos },
                    source: src.to_string(),
                })?;
                tokens.push(tok(TokenKind::Int(-n), start, pos));
            }
            c if c.is_ascii_digit() => {
                while pos < bytes.len() && bytes[pos].is_ascii_digit() {
                    pos += 1;
                }
                let digits = &input[start..pos];
                let n: i64 = digits.parse().map_err(|_| DslError {
                    kind: DslErrorKind::Expected {
                        expected: "integer",
                        found: digits.to_string(),
                    },
                    span: Span { start, end: pos },
                    source: src.to_string(),
                })?;
                tokens.push(tok(TokenKind::Int(n), start, pos));
            }
            't' | 'T' => {
                // Greedy atom: T + digits + optional suffix letter.
                pos += 1;
                while pos < bytes.len() && bytes[pos].is_ascii_digit() {
                    pos += 1;
                }
                // Optional suffix: t, q, p (case-insensitive).
                if pos < bytes.len() {
                    let suffix = bytes[pos] as char;
                    if matches!(suffix, 't' | 'T' | 'q' | 'Q' | 'p' | 'P') {
                        pos += 1;
                    }
                }
                let text = input[start..pos].to_string();
                tokens.push(tok(TokenKind::Atom(text), start, pos));
            }
            _ => {
                return Err(DslError {
                    kind: DslErrorKind::UnexpectedChar(ch),
                    span: Span {
                        start,
                        end: start + 1,
                    },
                    source: src.to_string(),
                });
            }
        }
    }

    Ok(tokens)
}

fn tok(kind: TokenKind, start: usize, end: usize) -> Token {
    Token {
        kind,
        span: Span { start, end },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(input: &str) -> Vec<TokenKind> {
        tokenize(input).unwrap().into_iter().map(|t| t.kind).collect()
    }

    #[test]
    fn single_atom() {
        assert_eq!(kinds("T16"), vec![TokenKind::Atom("T16".into())]);
    }

    #[test]
    fn atom_lowercase() {
        assert_eq!(kinds("t16"), vec![TokenKind::Atom("t16".into())]);
    }

    #[test]
    fn atom_with_suffix() {
        assert_eq!(kinds("T8q"), vec![TokenKind::Atom("T8q".into())]);
        assert_eq!(kinds("t16t"), vec![TokenKind::Atom("t16t".into())]);
        assert_eq!(kinds("T2p"), vec![TokenKind::Atom("T2p".into())]);
    }

    #[test]
    fn meet_expr() {
        assert_eq!(
            kinds("T16&T8"),
            vec![
                TokenKind::Atom("T16".into()),
                TokenKind::Ampersand,
                TokenKind::Atom("T8".into()),
            ]
        );
    }

    #[test]
    fn join_expr() {
        assert_eq!(
            kinds("T16|T8"),
            vec![
                TokenKind::Atom("T16".into()),
                TokenKind::Pipe,
                TokenKind::Atom("T8".into()),
            ]
        );
    }

    #[test]
    fn imply_coimply() {
        assert_eq!(
            kinds("T16>T8"),
            vec![
                TokenKind::Atom("T16".into()),
                TokenKind::Gt,
                TokenKind::Atom("T8".into()),
            ]
        );
        assert_eq!(
            kinds("T16<T8"),
            vec![
                TokenKind::Atom("T16".into()),
                TokenKind::Lt,
                TokenKind::Atom("T8".into()),
            ]
        );
    }

    #[test]
    fn negation() {
        assert_eq!(
            kinds("!T16"),
            vec![TokenKind::Bang, TokenKind::Atom("T16".into())]
        );
    }

    #[test]
    fn swing_modifier() {
        assert_eq!(
            kinds("T16~T16:80"),
            vec![
                TokenKind::Atom("T16".into()),
                TokenKind::Tilde,
                TokenKind::Atom("T16".into()),
                TokenKind::Colon,
                TokenKind::Int(80),
            ]
        );
    }

    #[test]
    fn offset_negative() {
        assert_eq!(
            kinds("@-5"),
            vec![TokenKind::At, TokenKind::Int(-5)]
        );
    }

    #[test]
    fn offset_positive() {
        assert_eq!(
            kinds("@20"),
            vec![TokenKind::At, TokenKind::Int(20)]
        );
    }

    #[test]
    fn parens() {
        assert_eq!(
            kinds("(T16|T8)&T4"),
            vec![
                TokenKind::LParen,
                TokenKind::Atom("T16".into()),
                TokenKind::Pipe,
                TokenKind::Atom("T8".into()),
                TokenKind::RParen,
                TokenKind::Ampersand,
                TokenKind::Atom("T4".into()),
            ]
        );
    }

    #[test]
    fn whitespace_between_tokens() {
        assert_eq!(
            kinds("T16 & T8"),
            vec![
                TokenKind::Atom("T16".into()),
                TokenKind::Ampersand,
                TokenKind::Atom("T8".into()),
            ]
        );
    }

    #[test]
    fn empty_input_returns_empty() {
        assert_eq!(kinds(""), Vec::<TokenKind>::new());
        assert_eq!(kinds("   "), Vec::<TokenKind>::new());
    }

    #[test]
    fn unexpected_char_errors() {
        let err = tokenize("T16 # T8").unwrap_err();
        assert_eq!(err.kind, DslErrorKind::UnexpectedChar('#'));
        assert_eq!(err.span, Span { start: 4, end: 5 });
    }

    #[test]
    fn swing_with_negative_amount() {
        assert_eq!(
            kinds("~T16:-40"),
            vec![
                TokenKind::Tilde,
                TokenKind::Atom("T16".into()),
                TokenKind::Colon,
                TokenKind::Int(-40),
            ]
        );
    }

    // ── Proptest ─────────────────────────────────────────────────

    use proptest::prelude::*;

    proptest! {
        /// Lexer never panics on arbitrary input.
        #[test]
        fn tokenize_never_panics(s in ".*") {
            let _ = tokenize(&s);
        }
    }
}
