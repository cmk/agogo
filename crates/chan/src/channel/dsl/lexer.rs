//! Single-pass lexer for the polyrhythm DSL.
//!
//! Tokenizes a source string into a `Vec<Token>`. Whitespace between
//! tokens is skipped. Identifiers (grid names like `T16` and variable
//! names like `kick`) are recognized greedily: any run of ASCII
//! letters + digits starting with a letter.

use super::ast::Span;
use super::error::{DslError, DslErrorKind};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TokenKind {
    /// Identifier: grid name (`T16`, `t8q`) or variable (`kick`, `C1`).
    /// Disambiguation happens at parse/eval time via `Grid::from_str`.
    Ident(String),
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
            '(' => {
                tokens.push(tok(TokenKind::LParen, start, start + 1));
                pos += 1;
            }
            ')' => {
                tokens.push(tok(TokenKind::RParen, start, start + 1));
                pos += 1;
            }
            c if c.is_ascii_alphabetic() => {
                // Greedy identifier: letters + digits.
                pos += 1;
                while pos < bytes.len() && bytes[pos].is_ascii_alphanumeric() {
                    pos += 1;
                }
                let text = input[start..pos].to_string();
                tokens.push(tok(TokenKind::Ident(text), start, pos));
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
        tokenize(input)
            .unwrap()
            .into_iter()
            .map(|t| t.kind)
            .collect()
    }

    #[test]
    fn grid_atom() {
        assert_eq!(kinds("T16"), vec![TokenKind::Ident("T16".into())]);
        assert_eq!(kinds("t8q"), vec![TokenKind::Ident("t8q".into())]);
        assert_eq!(kinds("T2p"), vec![TokenKind::Ident("T2p".into())]);
    }

    #[test]
    fn variable_name() {
        assert_eq!(kinds("kick"), vec![TokenKind::Ident("kick".into())]);
        assert_eq!(kinds("C1"), vec![TokenKind::Ident("C1".into())]);
        assert_eq!(kinds("hats"), vec![TokenKind::Ident("hats".into())]);
    }

    #[test]
    fn meet_expr() {
        assert_eq!(
            kinds("kick&T16"),
            vec![
                TokenKind::Ident("kick".into()),
                TokenKind::Ampersand,
                TokenKind::Ident("T16".into()),
            ]
        );
    }

    #[test]
    fn all_operators() {
        assert_eq!(
            kinds("a&b|c>d<e"),
            vec![
                TokenKind::Ident("a".into()),
                TokenKind::Ampersand,
                TokenKind::Ident("b".into()),
                TokenKind::Pipe,
                TokenKind::Ident("c".into()),
                TokenKind::Gt,
                TokenKind::Ident("d".into()),
                TokenKind::Lt,
                TokenKind::Ident("e".into()),
            ]
        );
    }

    #[test]
    fn negation() {
        assert_eq!(
            kinds("!T16"),
            vec![TokenKind::Bang, TokenKind::Ident("T16".into())]
        );
    }

    #[test]
    fn parens() {
        assert_eq!(
            kinds("(kick|T8)&T4"),
            vec![
                TokenKind::LParen,
                TokenKind::Ident("kick".into()),
                TokenKind::Pipe,
                TokenKind::Ident("T8".into()),
                TokenKind::RParen,
                TokenKind::Ampersand,
                TokenKind::Ident("T4".into()),
            ]
        );
    }

    #[test]
    fn whitespace_between_tokens() {
        assert_eq!(
            kinds("T16 & T8"),
            vec![
                TokenKind::Ident("T16".into()),
                TokenKind::Ampersand,
                TokenKind::Ident("T8".into()),
            ]
        );
    }

    #[test]
    fn empty_input() {
        assert_eq!(kinds(""), Vec::<TokenKind>::new());
        assert_eq!(kinds("   "), Vec::<TokenKind>::new());
    }

    #[test]
    fn unexpected_char() {
        let err = tokenize("T16 # T8").unwrap_err();
        assert_eq!(err.kind, DslErrorKind::UnexpectedChar('#'));
    }

    #[test]
    fn digits_only_is_unexpected() {
        // Bare digits (no leading letter) are not valid identifiers.
        let err = tokenize("123").unwrap_err();
        assert_eq!(err.kind, DslErrorKind::UnexpectedChar('1'));
    }

    use proptest::prelude::*;

    proptest! {
        #[test]
        fn tokenize_never_panics(s in ".*") {
            let _ = tokenize(&s);
        }
    }
}
