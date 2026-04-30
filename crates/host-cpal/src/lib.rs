#![forbid(unsafe_code)]

//! cpal back-end for `agogo_core::sink::audio::AudioHost`.
//!
//! Plan 13 T1 ships the `CpalHost` type: opens a cpal input stream
//! (f32 samples), surfaces cpal's device enumeration through a
//! stable API, and adapts cpal's per-buffer callback to
//! [`agogo_core::sink::audio::AudioIo`]. The `output: &mut []` slice is a
//! stub — CV output lands in v0.4's `out/audio` module.
//!
//! # Precision note
//!
//! midir-backed MIDI out (Plan 13's sibling `host-midi` crate)
//! inherits a ~1 ms USB-bus-limited dispatch jitter per
//! `doc/agogo.md` §4. This crate's audio-input side is
//! sample-accurate (the `buffer_start_sample` counter feeds the
//! PLL's phase estimate directly); platform-native MIDI back-ends
//! (CoreMIDI / JACK / ALSA-MIDI / WinMM) that tighten the output
//! side are post-v0.5.

pub mod cpal;

pub use cpal::CpalHost;
