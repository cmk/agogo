//! `ChannelMode` — the enum of per-channel output modes.
//!
//! Plan 03 implemented `MidiClock`. Plan 2026-04-25-03 adds
//! `Click(ClickConfig)` (per-tick metronome trigger). The remaining
//! variants exist so downstream pattern matches against the v1 spec
//! surface (agogo.md §3) stay exhaustive; their rendering paths land
//! in v0.2+.

use core::num::NonZeroU32;

use crate::midi::{U4, U7};

/// Per-channel output mode.
///
/// `MidiClock` and `Click(_)` are rendered. `Din`, `AnalogPulse`,
/// `AnalogLfo`, and `MidiCc` are stubs carrying the public API shape;
/// their rendering paths land in v0.2+.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum ChannelMode {
    /// MIDI clock bytes (0xF8 per PPQN/24). Rendered in Plan 04.
    MidiClock,
    /// DIN sync24 pulse stream. Stub — spec surface only.
    Din,
    /// Single analog pulse/gate per tick. Stub — rendering is v0.2
    /// (`out/audio.rs`).
    AnalogPulse,
    /// Continuous LFO waveform rendered at sample rate. Stub —
    /// `channel/lfo.rs` lives in v0.2.
    AnalogLfo,
    /// MIDI CC controller. Spec-surface stub; rendering is v0.2.
    MidiCc { cc: U7, range: (U7, U7) },
    /// Per-tick metronome trigger.
    ///
    /// `Click(_)` is the agnostic role; the nested [`ClickConfig`]
    /// carries the output-specific payload. Today the only variant
    /// is [`ClickConfig::Midi`] (Note On per tick); future audio-
    /// sample or CV-impulse click outputs slot in alongside it
    /// without touching this enum.
    Click(ClickConfig),
}

/// Output-specific configuration for a [`ChannelMode::Click`] channel.
///
/// New click targets are added as new variants; the agnostic
/// `ChannelMode::Click(_)` shell stays the same.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum ClickConfig {
    /// Emit a MIDI Note On (followed by a same-sample Note Off) per
    /// scheduled tick.
    Midi(MidiClickConfig),
    // Future: Audio(AudioClickConfig), Cv(CvClickConfig).
}

/// MIDI Note On / Note Off per scheduled tick.
///
/// `vel` is `U7` but additionally restricted to `1..=127` at the spec
/// parser — `vel=0` is a Note Off in the MIDI spec, so the parser
/// rejects it to avoid silent metronomes. `ch` is the zero-based MIDI
/// channel; user-facing 1–16 is mapped to `U4` at the spec parser.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct MidiClickConfig {
    pub note: U7,
    pub vel: U7,
    pub ch: U4,
    pub accent: Option<MidiClickAccent>,
}

/// Periodic accent on a [`MidiClickConfig`] channel.
///
/// When `Some`, the renderer substitutes `note` / `vel` whenever the
/// per-channel emitted-click counter satisfies `counter % every == 0`
/// (counter starts at 0 and resets on transport stop). `every` is
/// `NonZeroU32` so accent placement is well-defined.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct MidiClickAccent {
    pub every: NonZeroU32,
    pub note: U7,
    pub vel: U7,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// All spec-surface variants are reachable. Guards against
    /// an accidental `#[non_exhaustive]` / rename regression.
    #[test]
    fn all_variants_constructible() {
        let click_no_accent = ChannelMode::Click(ClickConfig::Midi(MidiClickConfig {
            note: U7(76),
            vel: U7(100),
            ch: U4(9),
            accent: None,
        }));
        let click_with_accent = ChannelMode::Click(ClickConfig::Midi(MidiClickConfig {
            note: U7(37),
            vel: U7(70),
            ch: U4(9),
            accent: Some(MidiClickAccent {
                every: NonZeroU32::new(4).unwrap(),
                note: U7(38),
                vel: U7(120),
            }),
        }));
        let modes = [
            ChannelMode::MidiClock,
            ChannelMode::Din,
            ChannelMode::AnalogPulse,
            ChannelMode::AnalogLfo,
            ChannelMode::MidiCc {
                cc: U7(74),
                range: (U7(0), U7(127)),
            },
            click_no_accent,
            click_with_accent,
        ];
        // Exhaustive match must cover every arm — compile-time check.
        for m in modes {
            match m {
                ChannelMode::MidiClock
                | ChannelMode::Din
                | ChannelMode::AnalogPulse
                | ChannelMode::AnalogLfo
                | ChannelMode::MidiCc { .. }
                | ChannelMode::Click(_) => {}
            }
        }
    }
}
