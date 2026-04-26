//! DSL error types with source-span context.

use super::ast::Span;

/// Errors from DSL parsing and evaluation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DslError {
    pub kind: DslErrorKind,
    pub span: Span,
    /// The original input string (owned for error display).
    pub source: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DslErrorKind {
    /// Empty input (after stripping whitespace).
    EmptyInput,
    /// Unexpected character in the input.
    UnexpectedChar(char),
    /// Expected a token but found end of input.
    UnexpectedEof,
    /// Expected a specific token kind.
    Expected {
        expected: &'static str,
        found: String,
    },
    /// Invalid grid atom (e.g., `T3`, `T1t`).
    InvalidAtom(String),
    /// Invalid swing resolution (not a valid TBase).
    InvalidSwingResolution(String),
    /// Swing amount out of i8 range.
    SwingAmountOutOfRange(i64),
    /// Duplicate modifier (e.g., two `~` on the same track).
    DuplicateModifier(&'static str),
    /// Offset value out of i32 range.
    OffsetOutOfRange(i64),
    /// Unmatched parenthesis.
    UnmatchedParen,
    /// Nesting depth exceeded (max 32).
    NestingTooDeep,
}

impl std::fmt::Display for DslError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DSL error: {}\n  {}\n  ", self.kind, self.source)?;
        for _ in 0..self.span.start {
            write!(f, " ")?;
        }
        let width = (self.span.end.saturating_sub(self.span.start)).max(1);
        for _ in 0..width {
            write!(f, "^")?;
        }
        Ok(())
    }
}

impl std::fmt::Display for DslErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyInput => write!(f, "empty input"),
            Self::UnexpectedChar(c) => write!(f, "unexpected character '{c}'"),
            Self::UnexpectedEof => write!(f, "unexpected end of input"),
            Self::Expected { expected, found } => {
                write!(f, "expected {expected}, found {found}")
            }
            Self::InvalidAtom(s) => write!(f, "invalid grid atom: {s}"),
            Self::InvalidSwingResolution(s) => {
                write!(f, "invalid swing resolution: {s}")
            }
            Self::SwingAmountOutOfRange(n) => {
                write!(f, "swing amount {n} out of i8 range")
            }
            Self::DuplicateModifier(kind) => {
                write!(f, "duplicate {kind} modifier")
            }
            Self::OffsetOutOfRange(n) => {
                write!(f, "offset {n} out of i32 range")
            }
            Self::UnmatchedParen => write!(f, "unmatched parenthesis"),
            Self::NestingTooDeep => write!(f, "nesting depth exceeds 32"),
        }
    }
}

impl std::error::Error for DslError {}
