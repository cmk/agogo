//! Per-channel scheduler: consumes a master tick stream and emits
//! sample-indexed events after the divider / shuffle / shift / offset
//! transform pipeline. Pure logic — no audio I/O, no MIDI bytes.
//!
//! Submodules:
//! - [`mode`]      — enum `ChannelMode` (MidiClock, Din, AnalogPulse, AnalogLfo, MidiCc).
//! - [`transform`] — `Channel` struct + per-buffer transform pipeline.
//! - [`scheduler`] — `tick_stream` block-level event emission.

pub mod mode;
pub mod scheduler;
pub mod transform;
