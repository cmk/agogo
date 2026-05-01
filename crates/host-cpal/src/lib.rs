#![forbid(unsafe_code)]

//! cpal back-end for `agogo_core::sink::audio::AudioHost`.
//!
//! `CpalHost` opens a cpal input stream (f32 samples), surfaces
//! cpal's device enumeration through a
//! stable API, and adapts cpal's per-buffer callback to
//! [`agogo_core::sink::audio::AudioIo`]. The `output: &mut []` slice is a
//! stub — CV output lands in v0.4's heterogeneous output layer.
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

pub mod cpal;

pub use cpal::CpalHost;
