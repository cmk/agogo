#![forbid(unsafe_code)]

//! midir back-end for `agogo::core::sink::midi::MidiSink`.
//!
//! `MidirSink` opens a midir output connection by port name and
//! forwards every `send_at` call to the underlying
//! `MidiOutputConnection` immediately. midir has no native scheduler,
//! so the `at_sample` argument is metadata only — dispatch jitter
//! inherits midir's ~1 ms USB-bus-limited figure (`doc/agogo.md` §4).
//!
//! `MidirSink` reports
//! [`MidiTimingCapability`](agogo::core::sink::midi::MidiTimingCapability)
//! as immediate best-effort dispatch. Platform-native MIDI sinks that
//! tighten this side (CoreMIDI's `MIDITimeStamp` for sub-us scheduling,
//! JACK's frame-indexed sends, WinMM, ALSA-MIDI) live in their own
//! sibling crates.

extern crate self as agogo;

pub(crate) mod core {
    pub(crate) use core_impl::*;
}

pub mod midir;

pub use midir::{MidirSink, MidirSinkError};
