//! Shared proptest strategies for the workspace.
//!
//! Import from any crate's test module:
//!
//! ```ignore
//! use project_core::arb;
//! ```
//!
//! Define strategies as functions returning `impl Strategy<Value = T>`,
//! not via `Arbitrary` derive. Use `prop_oneof!` with frequency weights
//! to bias toward boundary values and edge cases.
//!
//! This module is the equivalent of a shared Gen.hs — one place to
//! maintain generators for domain types so they don't get copy-pasted
//! across test modules.

// Strategies go here. Example:
//
// use proptest::prelude::*;
//
// pub fn arb_name() -> impl Strategy<Value = String> {
//     "[a-z][a-z0-9_]{0,63}"
// }
