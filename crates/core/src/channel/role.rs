//! Per-routing-target role enums + the shared `ChannelCommon`
//! field set.
//!
//! `Channel` (in [`crate::channel::transform`]) is sum-typed by
//! routing target (`Channel::Midi | Din | Cv`); each variant carries
//! a [`ChannelCommon`] (the field set the scheduler / transform
//! pipeline operates on) and a target-specific `*Role` payload that
//! the renderer consumes.
//!
//! Plan 21 (audit P3): replaced the flat `ChannelMode` enum, which
//! mixed routing-target encoding with role. The new shape makes
//! "non-MIDI channel passed to MIDI renderer" a compile error
//! rather than a silent no-op.

use core::num::{NonZeroU16, NonZeroU32};

use crate::midi::{U4, U7};
use crate::time::grid::Grid;
use crate::time::swing::SwingConfig;
use crate::time::decimal::Micro;

/// Field set shared across all `Channel` variants. The
/// scheduler / transform pipeline operates on this struct alone —
/// roles are only consulted at the renderer layer.
#[derive(Copy, Clone, Debug)]
pub struct ChannelCommon {
    /// Divider expressed as the `Grid` whose tick count is the
    /// channel's step (agogo.md §6 mapping). E.g. `Grid::T16` fires
    /// 16th notes, `Grid::T4` fires quarter notes, `Grid::T8Q` fires
    /// quintuplet 8ths (5 per quarter at 192 ticks each).
    pub divider: Grid,
    /// Swing configuration. Shifts off-beats earlier by
    /// `amount × multiplier` (`time::swing::effective_tick`).
    pub shuffle: SwingConfig,
    /// Positive-only delay compensation, clamped to
    /// `[Micro::ZERO, MAX_DELAY]` on use.
    pub delay: Micro,
    /// Signed calibration offset. Not clamped here — CLI / UI
    /// should pick a musical range (agogo.md §6 cites ±5 ms = ±5
    /// 000 µs).
    pub offset: Micro,
    /// Period multiplier on the channel's grid output. When
    /// `Some(N)`, the channel emits every `N`-th `tick_stream` event
    /// (applied as a pre-renderer filter in `Machine::on_buffer`
    /// against the per-channel `bar_counters` slot).
    /// `NonZeroU16` caps `N` at 65,535 — worst-case multiplied
    /// period `65,535 × Grid::T1.tick_count() (3840) ≈ 251M` ticks
    /// fits in `Tick(u32)` (`u32::MAX ≈ 4.29B`) with no
    /// overflow-check arithmetic.
    pub bar_multiplier: Option<NonZeroU16>,
}

// ── MIDI role family ───────────────────────────────────────────────

/// MIDI-target role: what kind of MIDI channel-voice or system-real-
/// time payload this channel emits per scheduled tick.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum MidiRole {
    /// `0xF8` System Real-Time Clock per scheduled tick. Plan 12.
    Clock,
    /// MIDI Note On + same-sample Note Off per scheduled tick
    /// (metronome / click). Plan 19 (PR #21) introduced; Plan 21
    /// migrates the field type from `ChannelMode::Click(_)`.
    Click(MidiClickConfig),
    /// MIDI Continuous Controller. Spec-surface stub; rendering
    /// path lands in v0.2.
    Cc(MidiCcConfig),
}

/// MIDI Note On / Note Off per scheduled tick.
///
/// `vel` is `U7` but additionally restricted to `1..=127` at the
/// spec parser — `vel=0` is a Note Off in the MIDI spec, so the
/// parser rejects it to avoid silent metronomes. `ch` is the
/// zero-based MIDI channel; user-facing 1–16 is mapped to `U4` at
/// the spec parser.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct MidiClickConfig {
    pub note: U7,
    pub vel: U7,
    pub ch: U4,
    pub accent: Option<MidiClickAccent>,
}

/// Periodic accent on a [`MidiClickConfig`] channel.
///
/// When `Some`, the renderer substitutes `note` / `vel` whenever
/// the per-channel emitted-click counter satisfies
/// `counter % every == 0` (counter starts at 0 and resets on
/// transport stop). `every` is `NonZeroU32` so accent placement
/// is well-defined.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct MidiClickAccent {
    pub every: NonZeroU32,
    pub note: U7,
    pub vel: U7,
}

/// MIDI Continuous Controller payload.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct MidiCcConfig {
    pub cc: U7,
    pub range: (U7, U7),
}

// ── DIN sync target ─────────────────────────────────────────────────

/// DIN-sync target role.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum DinRole {
    /// 24 PPQN sync pulse stream. Spec surface only — rendering
    /// lands when a DIN back-end exists.
    Sync24,
}

// ── CV / analog target ──────────────────────────────────────────────

/// Analog / CV target role. Stub family — rendering lands in v0.2+
/// in a future audio output module (no `out/audio.rs` exists in the
/// repo today). `Pulse` is a single-sample gate; `Lfo` is a sample-
/// rate continuous waveform.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum CvRole {
    Pulse,
    Lfo,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// All role variants are constructible at the type level.
    /// Guards against an accidental `#[non_exhaustive]` regression
    /// or a future field that breaks the `Copy + Clone + Eq`
    /// derive chain.
    #[test]
    fn all_role_variants_constructible() {
        let _ = MidiRole::Clock;
        let _ = MidiRole::Click(MidiClickConfig {
            note: U7(76),
            vel: U7(100),
            ch: U4(9),
            accent: None,
        });
        let _ = MidiRole::Click(MidiClickConfig {
            note: U7(37),
            vel: U7(70),
            ch: U4(9),
            accent: Some(MidiClickAccent {
                every: NonZeroU32::new(4).unwrap(),
                note: U7(38),
                vel: U7(120),
            }),
        });
        let _ = MidiRole::Cc(MidiCcConfig {
            cc: U7(74),
            range: (U7(0), U7(127)),
        });
        let _ = DinRole::Sync24;
        let _ = CvRole::Pulse;
        let _ = CvRole::Lfo;
    }
}
