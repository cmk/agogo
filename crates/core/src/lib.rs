#![forbid(unsafe_code)]

//! Runtime orchestration support for agogo.
//!
//! This crate owns agogo's host/runtime boundary: command admission,
//! playhead transport, snapshots, and driver helpers. Pure musical,
//! channel, time, sink, and control-loop types live in `agogo-chan`
//! and are re-exported here so runtime code and downstream callers
//! share one surface.

extern crate self as agogo;

pub use chan_impl::{channel, conn, control, sink, test, time};

pub mod bridge;
pub mod driver;
pub mod event;
pub mod runtime;
pub mod snapshot;
pub mod transport;

pub use bridge::{
    AdmissionMetadata, AdmissionOutcome, AdmissionRejectReason, AdmissionStatus, BridgeError,
    CoalesceKey, CommandApplyReport, CommandDeadline, CommandEnvelope, CommandId,
    CommandTimeDomain, ControlCommand, ControlConsumer, ControlParams, ControlProducer,
    RtCommandDrain, SourceId, apply_control_to_playhead, spsc,
};
pub use driver::{AgogoDriver, AgogoDriverConfig, Tool};
pub use event::{max_events_for_buffer, tick_stream, tick_stream_into};
pub use runtime::{Runtime, RuntimeStepReport, RuntimeSurface, RuntimeToolMetadata};
pub use snapshot::{AgogoSnapshot, SnapshotSlot};
pub use transport::{
    Playhead, PlayheadStopHandle, TransportCommandApply, TransportPolicy, TransportState,
};
