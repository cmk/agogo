//! Per-channel configuration + the per-channel transform pipeline.
//! Pure logic — no audio I/O, no MIDI bytes.
//!
//! Submodules:
//! - [`role`]      — per-routing-target role enums (`MidiRole`,
//!   `DinRole`, `CvRole`) + the shared [`role::ChannelCommon`]
//!   field set.
//! - [`time`]      — sum-typed [`time::Channel`] enum + the
//!   per-buffer transform pipeline (was `channel/transform.rs`
//!   before Plan 2026-04-29-01 T5).
//! - [`scheduler`] — `tick_stream` block-level event emission.
//! - [`dsl`]       — polyrhythm grid expression parser (was
//!   top-level `dsl/` before T5).
//! - [`spec`]      — channel-spec mini-language for
//!   `agogo run --ch <spec>` (was `machine/spec/` before T5).

pub mod dsl;
pub mod role;
pub mod scheduler;
pub mod spec;
pub mod time;

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
pub use time::{Channel, MAX_DELAY, ScheduledEvent, transform};
