//! `ChannelSpec` parsed-form type + `snap_intent` accessor.
//!
//! The parser (`parse`), validator (`into_channel`), and Display impl
//! live in sibling modules; this file holds the data shape they
//! share. Plan 2026-04-28-06 T1: extracted from `machine/spec.rs`.

use core::num::NonZeroU16;

use crate::channel::role::MidiRole;
use crate::conn::fixed::Micro;
use crate::time::grid::Grid;
use crate::time::swing::SwingConfig;

/// Parsed `--ch` spec.
///
/// Audit P4 (Plan 22): the spec carries no routing-target tag —
/// every `ChannelSpec` is implicitly MIDI-targeted, because that's
/// the only target the parser produces today (`dev=audio` is
/// rejected at parse time as `AudioDeferred`). When later output work
/// adds `dev=din` or `dev=cv` parsers, `ChannelSpec` becomes a sum type
/// `enum { Midi, Din, Cv }` mirroring [`Channel`](crate::channel::Channel)'s
/// variants from audit P3.
#[derive(Debug, Clone, PartialEq)]
pub struct ChannelSpec {
    /// Optional human-readable identifier. Free-form string.
    pub id: Option<String>,
    /// Device-specific routing target (port name, channel index).
    pub out: Option<String>,
    /// Grid — resolved from a DSL expression at parse time.
    pub grid: Grid,
    /// Per-channel output role. Default `MidiRole::Clock`;
    /// `mode=click` produces `MidiRole::Click(MidiClickConfig)`.
    /// The spec parser only ever produces MIDI-target roles
    /// (audit P3 reshaped Channel into per-target variants;
    /// non-MIDI targets aren't user-constructible from the
    /// `--ch` mini-language today).
    pub mode: MidiRole,
    /// Swing configuration. Default: `T8:0` (no swing at eighth-note
    /// resolution).
    pub swing: SwingConfig,
    /// Musical offset in ticks (signed). Default: 0.
    pub offset_ticks: i32,
    /// Non-negative delay compensation. Parser rejects negative
    /// inputs (Q3 round-1 fix); `into_channel` further clamps to
    /// MAX_DELAY (300 ms) on the upper bound. Stored as `Micro`
    /// (i64 µs) so the f64 surface area is contained to the
    /// parser's first line — audit P0b / Q3 closure for finding K.
    pub delay: Micro,
    /// Optional quantum snap, in microseconds (Link-aware channels).
    pub snap_to_quantum_micro: Option<i64>,
    /// Per-channel `bar_multiplier`. Plan 2026-04-25-03: emit every
    /// `N`-th `tick_stream` event; divider-agnostic ("bars" reflects
    /// the most idiomatic `grid=t1` case but the mechanism applies
    /// to any grid).
    pub bars: Option<NonZeroU16>,
}

impl ChannelSpec {
    /// Parsed `snap-quantum-us=N` intent, if present. Returns the raw
    /// microbeat count as `Option<Micro>`; the orchestrator wraps it
    /// in `agogo::host::link::Quantum` at the host-link boundary.
    ///
    /// Plan 2026-04-28-03 T4 changed the return type from
    /// `Option<Quantum>` to `Option<Micro>`: `Quantum` is a
    /// host-link-shaped type and `core` shouldn't produce it. The
    /// wrap happens where it's consumed (`LinkSession::snap_offset_for`).
    ///
    /// Audit P2 (Plan 20) dropped `snap_to_quantum` from the runtime
    /// `Channel` because the field was never read in production
    /// (`arm_channel` was test-only). The intent now lives only on
    /// `ChannelSpec`; orchestrator wiring that actually applies the
    /// snap to `Channel.offset` is a follow-up.
    pub fn snap_intent(&self) -> Option<Micro> {
        self.snap_to_quantum_micro.map(Micro)
    }
}
