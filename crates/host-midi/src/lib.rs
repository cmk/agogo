#![forbid(unsafe_code)]

//! midir back-end for `agogo_core::out::midi::MidiSink`.
//!
//! Plan 13 T2 ships `MidirSink`: opens a midir output connection by
//! port name and forwards every `send_at` call to the underlying
//! `MidiOutputConnection` immediately. midir has no native scheduler,
//! so the `at_sample` argument is metadata only — dispatch jitter
//! inherits midir's ~1 ms USB-bus-limited figure (`doc/agogo.md` §4).
//!
//! Platform-native MIDI sinks that tighten this side (CoreMIDI's
//! `MIDITimeStamp` for sub-µs scheduling, JACK's frame-indexed sends,
//! WinMM, ALSA-MIDI) live in their own sibling crates and arrive
//! post-v0.5.

pub mod midir;

pub use midir::{MidirSink, MidirSinkError};
