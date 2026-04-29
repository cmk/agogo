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

// `CvRole` and `DinRole` are forward-compat scaffolding for v0.4
// (CV pulse / LFO via `out/audio`) and v0.2 (DIN sync24) backends —
// neither has a renderer in v0.1. Marked `#[doc(hidden)]` so they
// don't surface in `cargo doc` as user-facing API; the attribute
// comes off in the plan that ships the corresponding renderer.
// Plan 09 T6.
pub use role::{ChannelCommon, MidiCcConfig, MidiClickAccent, MidiClickConfig, MidiRole};
#[doc(hidden)]
pub use role::{CvRole, DinRole};
pub use scheduler::tick_stream;
pub use transform::{Channel, MAX_DELAY, ScheduledEvent, transform};
