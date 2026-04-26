//! Polyrhythm DSL parser.
//!
//! Parses a grid algebra expression into a [`TrackSpec`]. The grammar
//! supports meet (`&`), join (`|`), implication (`>`), coimplication
//! (`<`), negation (`!`), swing (`~TBase:amount`), and offset
//! (`@ticks`).
//!
//! See `doc/designs/dsl.md` for the full grammar and operator table.

pub mod ast;
pub mod error;

pub use ast::TrackSpec;
pub use error::DslError;
