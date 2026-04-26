//! `ChannelMode` — the enum of per-channel output modes.
//!
//! Plan 03 implemented `MidiClock`. Plan 2026-04-25-03 adds
//! `Click(ClickConfig)` (per-tick metronome trigger). The remaining
//! variants exist so downstream pattern matches against the v1 spec
//! surface (agogo.md §3) stay exhaustive; their rendering paths land
//! in v0.2+.

use core::num::NonZeroU32;

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
    /// MIDI CC controller.
    ///
    /// `cc` and `range` values are in the MIDI `u7` domain (`0..=127`)
    /// but stored as `u8`: agogo-core has no MIDI-crate dependency
    /// that would provide a real `u7` newtype. Spec-surface stub;
    /// rendering is v0.2.
    MidiCc { cc: u8, range: (u8, u8) },
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
/// `note` and `vel` (and the optional accent's note/vel) live in the
/// MIDI `u7` domain (`0..=127`). `ch` is `0..=15` (zero-based MIDI
/// channel; user-facing 1–16 is mapped down at the spec parser).
/// `vel` is `1..=127`; `vel=0` is a Note Off in the MIDI spec, so the
/// parser rejects it explicitly to avoid silent metronomes.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct MidiClickConfig {
    pub note: u8,
    pub vel: u8,
    pub ch: u8,
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
    pub note: u8,
    pub vel: u8,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// All spec-surface variants are reachable. Guards against
    /// an accidental `#[non_exhaustive]` / rename regression.
    #[test]
    fn all_variants_constructible() {
        let click_no_accent = ChannelMode::Click(ClickConfig::Midi(MidiClickConfig {
            note: 76,
            vel: 100,
            ch: 9,
            accent: None,
        }));
        let click_with_accent = ChannelMode::Click(ClickConfig::Midi(MidiClickConfig {
            note: 37,
            vel: 70,
            ch: 9,
            accent: Some(MidiClickAccent {
                every: NonZeroU32::new(4).unwrap(),
                note: 38,
                vel: 120,
            }),
        }));
        let modes = [
            ChannelMode::MidiClock,
            ChannelMode::Din,
            ChannelMode::AnalogPulse,
            ChannelMode::AnalogLfo,
            ChannelMode::MidiCc {
                cc: 74,
                range: (0, 127),
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
