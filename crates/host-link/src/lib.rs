#![forbid(unsafe_code)]

//! Ableton Link host-side integration for `agogo`.
//!
//! Wraps [`rusty_link`] (thin FFI over Ableton's C wrapper) and exposes
//! a `LinkClock` that implements `agogo_core::sync::PhaseSourceImpl`.
//! This sprint ships the lifecycle surface (enable / tempo /
//! num_peers); the `phase_at_sample` bridge remains `todo!()`-deferred
//! until the fxp refactor lands the final `Phase` / `Sample` types.
//!
//! The `link` submodule (and the `rusty_link` dependency) is gated
//! behind the `rusty-link` feature. Default builds produce an empty
//! crate so `cargo test --workspace` can resolve the dep graph
//! without `ext/rusty_link` being present — CI relies on this.
//! `agogo-cli --features link` activates `rusty-link` transitively.

#[cfg(feature = "rusty-link")]
pub mod link;

#[cfg(feature = "rusty-link")]
pub use link::LinkClock;
