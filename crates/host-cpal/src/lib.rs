#![forbid(unsafe_code)]

//! cpal back-end for `agogo::core::sink::audio::AudioHost`.
//!
//! `CpalHost` opens cpal input-only or output-only streams (f32
//! samples), surfaces cpal's device enumeration through a stable
//! API, and adapts cpal's per-buffer callback to
//! [`agogo::core::sink::audio::AudioIo`]. Output support starts with
//! the generated audio-click test feature: the core callback stays
//! mono, while this backend fans that signal out to the physical
//! output channel count cpal reports. CV output remains part of the
//! heterogeneous output layer.
//!
//! # Precision note
//!
//! midir-backed MIDI out (the sibling `host-midi` crate) inherits
//! a ~1 ms USB-bus-limited dispatch jitter per
//! `doc/agogo.md` §4. This crate's audio-input side is
//! sample-accurate (the `buffer_start_sample` counter feeds the
//! PLL's phase estimate directly); platform-native MIDI back-ends
//! (CoreMIDI / JACK / ALSA-MIDI / WinMM) that tighten the output
//! side are part of v0.2+ timestamped-output work.

extern crate self as agogo;

pub(crate) mod core {
    pub(crate) use core_impl::*;
}

pub mod cpal;

pub use cpal::CpalHost;
