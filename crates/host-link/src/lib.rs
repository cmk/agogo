#![forbid(unsafe_code)]

//! Ableton Link host-side integration for `agogo`.
//!
//! Wraps [`rusty_link`] (thin FFI over Ableton's C wrapper) and exposes
//! a `LinkClock` that — in later sprints — will implement
//! `agogo_core::sync::PhaseSourceImpl`. This sprint ships the
//! lifecycle surface (enable / tempo / num_peers); the phase-at-sample
//! bridge lands post-fxp.
//!
//! This crate is feature-gated out of the CLI via
//! `agogo-cli --features link`. Default builds do not pull it in,
//! keeping the CMake + C++ toolchain off the critical path.

pub mod link;

pub use link::LinkClock;
