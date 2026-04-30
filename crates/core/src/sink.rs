//! layer: sink
//! depends-on: channel, time, conn
//!
//! Output sinks — where the per-channel `ScheduledEvent` stream
//! becomes bytes on a wire.
//!
//! Members:
//!
//! - [`audio`] — audio I/O abstraction (`AudioHost`, `AudioIo`,
//!   `Config`, `Handle`) — the cpal callback boundary lives in
//!   `crates/host-cpal`; this module defines the interface the
//!   `Machine` consumes.
//! - [`midi`] — MIDI byte rendering (`MidiSink`, `MidiRtByte`,
//!   `render_midi_channel`) and the realtime status-byte
//!   constants.
//!
//! Plan 2026-04-29-01 T6 unified the previous `host` (audio) and
//! `out` (MIDI) parents under one intent-named module.

pub mod audio;
pub mod midi;
