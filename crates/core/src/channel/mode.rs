//! `ChannelMode` — the enum of per-channel output modes.
//!
//! Plan 03 implements only the `MidiClock` rendering path; the other
//! variants exist so downstream pattern matches against the v1 spec
//! surface (agogo.md §3) stay exhaustive. Their per-channel rendering
//! lands in v0.2+.

/// Per-channel output mode.
///
/// v0.1 scope: only `MidiClock` is rendered (Plan 04 wires the bytes;
/// Plan 03 just schedules tick events). `Din`, `AnalogPulse`,
/// `AnalogLfo`, and `MidiCc` are stubs carrying the public API shape.
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
}

#[cfg(test)]
mod tests {
    use super::*;

    /// All five spec-surface variants are reachable. Guards against
    /// an accidental `#[non_exhaustive]` / rename regression.
    #[test]
    fn all_variants_constructible() {
        let modes = [
            ChannelMode::MidiClock,
            ChannelMode::Din,
            ChannelMode::AnalogPulse,
            ChannelMode::AnalogLfo,
            ChannelMode::MidiCc {
                cc: 74,
                range: (0, 127),
            },
        ];
        // Exhaustive match must cover every arm — compile-time check.
        for m in modes {
            match m {
                ChannelMode::MidiClock
                | ChannelMode::Din
                | ChannelMode::AnalogPulse
                | ChannelMode::AnalogLfo
                | ChannelMode::MidiCc { .. } => {}
            }
        }
    }
}
