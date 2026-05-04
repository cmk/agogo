//! layer: channel
//! depends-on: time, conn
//!
//! Per-channel configuration + the per-channel transform pipeline.
//! Pure logic — no audio I/O, no MIDI bytes.
//!
//! Submodules:
//! - [`role`]      — per-routing-target role enums (`MidiRole`,
//!   `AudioRole`, `DinRole`, `CvRole`) + the shared [`role::ChannelCommon`]
//!   field set.
//! - [`time`]      — sum-typed [`time::Channel`] enum + the
//!   per-buffer transform pipeline (was `channel/transform.rs`
//!   before Plan 2026-04-29-01 T5).
//! - [`dsl`]       — polyrhythm grid expression parser (was
//!   top-level `dsl/` before T5).
//! - [`spec`]      — channel-spec mini-language for
//!   `agogo run --ch <spec>` (was `machine/spec/` before T5).
//!
//! The block-level event scheduler (`tick_stream`,
//! `tick_stream_into`) moved to `agogo-core::event` in Plan
//! 2026-05-03-06 — it consumes `Channel` configs but its job is
//! runtime traversal, not channel configuration.

pub mod dsl;
pub mod role;
pub mod spec;
pub mod time;

// `DinRole` is still forward-compat scaffolding for future
// heterogeneous output backends. `CvRole::Pulse` is user-facing as
// of Plan 2026-05-04-04; `CvRole::Lfo` remains a stub variant under
// the same public enum until the LFO renderer lands.
#[doc(hidden)]
pub use role::DinRole;
pub use role::{
    AudioRole, ChannelCommon, CvRole, MidiCcConfig, MidiClickAccent, MidiClickConfig, MidiRole,
};
pub use time::{Channel, MAX_DELAY, ScheduledEvent, transform};
