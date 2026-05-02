#![forbid(unsafe_code)]

//! midir back-end for `agogo::core::sink::midi::MidiSink`.
//!
//! `MidirSink` opens a midir output connection by port name and
//! forwards every `send_at` call to the underlying
//! `MidiOutputConnection` immediately. midir has no native scheduler,
//! so the `at_sample` argument is metadata only — dispatch jitter
//! inherits midir's ~1 ms USB-bus-limited figure (`doc/agogo.md` §4).
//!
//! Platform-native MIDI sinks that tighten this side (CoreMIDI's
//! `MIDITimeStamp` for sub-µs scheduling, JACK's frame-indexed sends,
//! WinMM, ALSA-MIDI) live in their own sibling crates. v0.2 starts
//! the timestamped-sink proof and keeps this midir path explicitly
//! best-effort.

extern crate self as agogo;

pub(crate) mod core {
    pub(crate) use core_impl::*;
}

pub mod midir;

pub use midir::{MidirSink, MidirSinkError};
