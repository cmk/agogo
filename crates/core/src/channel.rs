//! Per-channel scheduler: consumes a master tick stream and emits
//! sample-indexed events after the divider / shuffle / delay / offset
//! transform pipeline. Pure logic — no audio I/O, no MIDI bytes.
//!
//! Submodules:
//! - [`role`]      — per-routing-target role enums (`MidiRole`,
//!   `DinRole`, `CvRole`) + the shared [`role::ChannelCommon`]
//!   field set.
//! - [`transform`] — sum-typed [`transform::Channel`] enum + the
//!   per-buffer transform pipeline.
//! - [`scheduler`] — `tick_stream` block-level event emission.

pub mod role;
pub mod scheduler;
pub mod transform;

pub use role::{
    ChannelCommon, CvRole, DinRole, MidiCcConfig, MidiClickAccent, MidiClickConfig, MidiRole,
};
pub use scheduler::tick_stream;
pub use transform::{Channel, MAX_DELAY, ScheduledEvent, transform};
