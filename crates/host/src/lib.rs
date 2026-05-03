#![forbid(unsafe_code)]

//! host adapter support for agogo.
//!
//! This crate starts with agogo-owned real-time control primitives.
//! The actual `stdio_core::driver::StudioMcpServer` impl remains
//! behind the dependency boundary because the sibling stdio-core
//! checkout currently pins a newer Rust toolchain than this workspace.
//! Keeping the RT bridge independent preserves `cargo test --workspace`
//! on agogo's pinned toolchain while giving the adapter a tested core.

extern crate self as agogo;

pub(crate) mod core {
    pub(crate) use core_impl::*;
}

pub mod bridge;
pub mod driver;
pub mod runtime;
pub mod snapshot;

pub use bridge::{
    AdmissionMetadata, AdmissionOutcome, AdmissionRejectReason, AdmissionStatus, BridgeError,
    CoalesceKey, CommandApplyReport, CommandDeadline, CommandEnvelope, CommandId,
    CommandTimeDomain, ControlCommand, ControlConsumer, ControlParams, ControlProducer,
    RtCommandDrain, SourceId, apply_control_to_playhead, spsc,
};
pub use driver::{AgogoDriver, AgogoDriverConfig, Tool};
pub use runtime::{Runtime, RuntimeStepReport, RuntimeSurface, RuntimeToolMetadata};
pub use snapshot::{AgogoSnapshot, SnapshotSlot};
