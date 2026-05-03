#![forbid(unsafe_code)]

//! host adapter support for agogo.
//!
//! This crate starts with agogo-owned real-time control primitives.
//! The actual `stdio_core::driver::McpServer` impl remains behind the
//! dependency boundary so agogo stays useful without a stdio-core runtime
//! dependency. The runtime helper gives the adapter a tested core that
//! sibling stdio-core test/dev code can wrap.

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
