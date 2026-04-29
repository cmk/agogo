#![forbid(unsafe_code)]

//! Ableton Link host-side integration for `agogo`.
//!
//! Wraps [`rusty_link`] (thin FFI over Ableton's C wrapper) and exposes
//! a `LinkClock` that implements `agogo_core::sync::PhaseSourceImpl`.
//! Ships the lifecycle surface (enable / tempo / num_peers) plus the
//! host-time bridge (`phase_at_sample` via `HostTimeAnchor`) as of
//! Plan 08; bidirectional (tempo push, transport, quantum snap) is
//! Plan 09.
//!
//! The `link` submodule (and the `rusty_link` dependency) is gated
//! behind the `rusty-link` feature. Default builds produce an empty
//! crate so `cargo test --workspace` can resolve the dep graph
//! without `ext/rusty_link` being present — CI relies on this.
//! `agogo-cli --features link` activates `rusty-link` transitively.

pub mod quantum;

pub use quantum::{Quantum, f64_beats_to_quantum, parse_quantum_from_beats};

#[cfg(feature = "rusty-link")]
pub mod link;

#[cfg(feature = "rusty-link")]
pub mod transport;

#[cfg(feature = "rusty-link")]
pub mod session;

#[cfg(feature = "rusty-link")]
pub mod source;

#[cfg(feature = "rusty-link")]
pub use link::{HostTimeAnchor, LinkClock};

#[cfg(feature = "rusty-link")]
pub use session::{LinkSession, LinkWriteConfig, apply_snap_offsets};

#[cfg(feature = "rusty-link")]
pub use source::{LinkPhaseSource, LinkSessionHandle};

#[cfg(feature = "rusty-link")]
pub use transport::{TransportEvent, TransportFsm, TransportOutput, TransportState};
