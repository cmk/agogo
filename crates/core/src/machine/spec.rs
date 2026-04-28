//! Channel-spec mini-language for `agogo run --ch <spec>`.
//!
//! Grammar: `key=val[,key=val]*`. Whitespace tolerated; `=` and `,`
//! separate; values may be quoted `"..."` to embed spaces or commas.
//!
//! Required keys: `dev`. Optional keys: `grid` (DSL expression,
//! default `T4`), `id`, `out`, `mode` (`clock`|`click`, default
//! `clock` — see Plan 2026-04-25-03), `swing` (`[TBase:]i8`,
//! default `T8:0`), `offset` (signed `i32` ticks — **note:**
//! non-zero values are currently rejected until the
//! tempo-dependent Tick→Micro conversion is wired), `delay` (ms),
//! `snap-quantum-us`, `bars` (divider-agnostic period multiplier:
//! keep every `N`-th scheduled event; `grid=t1` is the idiomatic
//! "bars" case — see Plan 2026-04-25-03). `mode=click` adds:
//! `note`, `vel` (required), `mch` (default 10), `accent-every`
//! (optional; if set, requires `accent-vel` and optionally
//! `accent-note`). Unknown keys are hard errors so typos are
//! caught early.
//!
//! The `grid` value is parsed via `dsl::parse` with a channel
//! environment, so expressions like `kick&T16` that reference
//! earlier channels are valid. Use [`parse_channels`] to resolve
//! a sequence of specs in order, building the environment as it goes.
//!
//! [`ChannelSpec`] holds the parsed form; [`ChannelSpec::into_channel`]
//! converts to a [`Channel`] at the CLI argv boundary, where the only
//! `f64` field (`delay_ms`) crosses via the `F064FD06` Conn per CLAUDE.md
//! float exception 4.

use core::num::{NonZeroU16, NonZeroU32};
use std::fmt::{self, Display};

use crate::channel::role::{ChannelCommon, MidiClickAccent, MidiClickConfig, MidiRole};
use crate::channel::transform::MAX_DELAY;
use crate::channel::Channel;
use crate::dsl;
use crate::fxp::{Extended, ExtendedFloat, F064FD06, Micro};
use crate::midi::{U4, U7};
use crate::time::grid::Grid;
use crate::time::swing::SwingConfig;
use crate::time::tbase::TBase;

/// Parsed `--ch` spec.
///
/// Audit P4 (Plan 22): the spec carries no routing-target tag —
/// every `ChannelSpec` is implicitly MIDI-targeted, because that's
/// the only target the parser produces today (`dev=audio` is
/// rejected at parse time as `AudioDeferred`). When v0.2/v0.4 add
/// `dev=din` or `dev=cv` parsers, `ChannelSpec` becomes a sum type
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
    /// Positive delay compensation in milliseconds. Clamped to
    /// MAX_DELAY (300 ms) in `into_channel`.
    pub delay_ms: f64, // argv boundary
    /// Optional quantum snap, in microseconds (Link-aware channels).
    pub snap_to_quantum_micro: Option<i64>,
    /// Per-channel `bar_multiplier`. Plan 2026-04-25-03: emit every
    /// `N`-th `tick_stream` event; divider-agnostic ("bars" reflects
    /// the most idiomatic `grid=t1` case but the mechanism applies
    /// to any grid).
    pub bars: Option<NonZeroU16>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ChannelSpecError {
    #[error("channel spec: empty key")]
    EmptyKey,
    #[error("channel spec: unknown key `{0}`")]
    UnknownKey(String),
    #[error("channel spec: missing `{0}`")]
    MissingKey(&'static str),
    #[error("channel spec: bad value for `{0}`: {1}")]
    BadValue(&'static str, String),
    #[error("channel spec: dev=audio requires v0.4 (out/audio)")]
    AudioDeferred,
    #[error("channel spec: malformed (expected `key=val,...`): {0}")]
    Malformed(String),
}

impl ChannelSpec {
    /// Parse a `key=val[,key=val]*` spec into a [`ChannelSpec`].
    ///
    /// The `grid=` value is resolved via `dsl::parse` using `env` for
    /// variable references. Pass `&[]` for the first channel.
    pub fn parse(s: &str, env: &[(String, Grid)]) -> Result<Self, ChannelSpecError> {
        let pairs = tokenize(s)?;

        let mut id: Option<String> = None;
        // The `dev=` key is required (audit P4 keeps that contract)
        // but its value is implicitly `midi` — `dev=audio` errors
        // at parse, `dev=midi` validates and stores nothing. Only
        // the presence of the key is tracked.
        let mut dev_seen: bool = false;
        let mut out: Option<String> = None;
        let mut grid_str: Option<String> = None;
        let mut swing: SwingConfig = SwingConfig {
            resolution: TBase::T8,
            amount: 0,
        };
        let mut offset_ticks: i32 = 0;
        let mut delay_ms: f64 = 0.0; // argv boundary
        let mut snap_to_quantum_micro: Option<i64> = None;

        // Plan 2026-04-25-03 keys. Defer mode/click validation until
        // after the loop: cross-key constraints (mode=click requires
        // note+vel; clock rejects click keys) are easier to enforce
        // in one pass at the end.
        let mut mode_kw: Option<&'static str> = None; // "clock" | "click"
        let mut note: Option<U7> = None;
        let mut vel: Option<U7> = None;
        let mut mch_zero_based: Option<U4> = None;
        let mut accent_every: Option<u32> = None;
        let mut accent_note: Option<U7> = None;
        let mut accent_vel: Option<U7> = None;
        let mut bars: Option<NonZeroU16> = None;

        for (k, v) in pairs {
            match k.as_str() {
                "id" => id = Some(v),
                "dev" => {
                    match v.as_str() {
                        "midi" => {} // ok — only valid value today
                        "audio" => return Err(ChannelSpecError::AudioDeferred),
                        other => {
                            return Err(ChannelSpecError::BadValue("dev", other.to_string()));
                        }
                    }
                    dev_seen = true;
                }
                "out" => out = Some(v),
                "grid" => {
                    grid_str = Some(v);
                }
                "mode" => {
                    mode_kw = Some(match v.as_str() {
                        "clock" => "clock",
                        "click" => "click",
                        other => {
                            return Err(ChannelSpecError::BadValue(
                                "mode",
                                format!("expected `clock` or `click`, got `{other}`"),
                            ));
                        }
                    });
                }
                "note" => {
                    let n = v
                        .parse::<u8>()
                        .map_err(|e| ChannelSpecError::BadValue("note", e.to_string()))?;
                    note = Some(U7::new(n).ok_or_else(|| {
                        ChannelSpecError::BadValue("note", format!("must be 0..=127, got {n}"))
                    })?);
                }
                "vel" => {
                    let n = v
                        .parse::<u8>()
                        .map_err(|e| ChannelSpecError::BadValue("vel", e.to_string()))?;
                    // vel=0 is a Note Off in the MIDI spec — reject
                    // separately so the error message is meaningful.
                    if n == 0 {
                        return Err(ChannelSpecError::BadValue(
                            "vel",
                            format!("must be 1..=127 (vel=0 is Note Off), got {n}"),
                        ));
                    }
                    vel = Some(U7::new(n).ok_or_else(|| {
                        ChannelSpecError::BadValue(
                            "vel",
                            format!("must be 1..=127 (vel=0 is Note Off), got {n}"),
                        )
                    })?);
                }
                "mch" => {
                    let n = v
                        .parse::<u8>()
                        .map_err(|e| ChannelSpecError::BadValue("mch", e.to_string()))?;
                    if !(1..=16).contains(&n) {
                        return Err(ChannelSpecError::BadValue(
                            "mch",
                            format!("must be 1..=16 (user-facing), got {n}"),
                        ));
                    }
                    // n ∈ 1..=16, so n - 1 ∈ 0..=15 ⊂ U4 — `new` is total.
                    mch_zero_based = Some(U4::new(n - 1).expect("n - 1 in 0..=15"));
                }
                "accent-every" => {
                    let n = v
                        .parse::<u32>()
                        .map_err(|e| ChannelSpecError::BadValue("accent-every", e.to_string()))?;
                    if n == 0 {
                        return Err(ChannelSpecError::BadValue(
                            "accent-every",
                            "must be > 0".into(),
                        ));
                    }
                    accent_every = Some(n);
                }
                "accent-note" => {
                    let n = v
                        .parse::<u8>()
                        .map_err(|e| ChannelSpecError::BadValue("accent-note", e.to_string()))?;
                    accent_note = Some(U7::new(n).ok_or_else(|| {
                        ChannelSpecError::BadValue(
                            "accent-note",
                            format!("must be 0..=127, got {n}"),
                        )
                    })?);
                }
                "accent-vel" => {
                    let n = v
                        .parse::<u8>()
                        .map_err(|e| ChannelSpecError::BadValue("accent-vel", e.to_string()))?;
                    if n == 0 {
                        return Err(ChannelSpecError::BadValue(
                            "accent-vel",
                            format!("must be 1..=127 (vel=0 is Note Off), got {n}"),
                        ));
                    }
                    accent_vel = Some(U7::new(n).ok_or_else(|| {
                        ChannelSpecError::BadValue(
                            "accent-vel",
                            format!("must be 1..=127 (vel=0 is Note Off), got {n}"),
                        )
                    })?);
                }
                "bars" => {
                    let n = v
                        .parse::<u16>()
                        .map_err(|e| ChannelSpecError::BadValue("bars", e.to_string()))?;
                    bars = Some(NonZeroU16::new(n).ok_or_else(|| {
                        ChannelSpecError::BadValue("bars", "must be > 0".into())
                    })?);
                }
                "swing" => {
                    swing = parse_swing(&v)?;
                }
                "offset" => {
                    offset_ticks = v
                        .parse::<i32>()
                        .map_err(|e| ChannelSpecError::BadValue("offset", e.to_string()))?;
                }
                "delay" => {
                    let parsed = v
                        .parse::<f64>()
                        .map_err(|e| ChannelSpecError::BadValue("delay", e.to_string()))?;
                    if !parsed.is_finite() {
                        return Err(ChannelSpecError::BadValue(
                            "delay",
                            "must be finite".into(),
                        ));
                    }
                    delay_ms = parsed;
                }
                "snap-quantum-us" => {
                    snap_to_quantum_micro = Some(
                        v.parse::<i64>().map_err(|e| {
                            ChannelSpecError::BadValue("snap-quantum-us", e.to_string())
                        })?,
                    );
                }
                other => return Err(ChannelSpecError::UnknownKey(other.to_string())),
            }
        }

        // Resolve grid expression via DSL parser.
        let grid_expr = grid_str.as_deref().unwrap_or("T4");
        let grid = dsl::parse(grid_expr, env)
            .map_err(|e| ChannelSpecError::BadValue("grid", e.to_string()))?;

        // Cross-key validation: assemble `mode` from the per-key
        // values according to the mode= keyword. Defaults to
        // MidiRole::Clock if mode= absent.
        let mode = match mode_kw.unwrap_or("clock") {
            "clock" => {
                // Reject click-only keys when mode is clock — they're
                // silently ignored otherwise, which hides typos.
                // Each first element is a string literal (`&'static
                // str`), which `BadValue`'s first param requires.
                const CLICK_ONLY_KEYS: &[&str] = &[
                    "note",
                    "vel",
                    "mch",
                    "accent-every",
                    "accent-note",
                    "accent-vel",
                ];
                let presence = [
                    note.is_some(),
                    vel.is_some(),
                    mch_zero_based.is_some(),
                    accent_every.is_some(),
                    accent_note.is_some(),
                    accent_vel.is_some(),
                ];
                for (i, &key) in CLICK_ONLY_KEYS.iter().enumerate() {
                    if presence[i] {
                        return Err(ChannelSpecError::BadValue(
                            key,
                            "only valid with mode=click".into(),
                        ));
                    }
                }
                MidiRole::Clock
            }
            "click" => {
                let note = note.ok_or(ChannelSpecError::MissingKey("note"))?;
                let vel = vel.ok_or(ChannelSpecError::MissingKey("vel"))?;
                // GM drum kit default = mch 10 (1-based) = U4(9).
                let ch = mch_zero_based.unwrap_or(U4(9));
                let accent = match accent_every {
                    Some(e) => {
                        let av =
                            accent_vel.ok_or(ChannelSpecError::MissingKey("accent-vel"))?;
                        let an = accent_note.unwrap_or(note);
                        Some(MidiClickAccent {
                            // SAFETY: e > 0 enforced at parse time.
                            every: NonZeroU32::new(e).expect("accent-every > 0"),
                            note: an,
                            vel: av,
                        })
                    }
                    None => {
                        // accent-vel and accent-note alone (without
                        // accent-every) are dead config — call them
                        // out to catch typos.
                        if accent_vel.is_some() || accent_note.is_some() {
                            return Err(ChannelSpecError::BadValue(
                                "accent-every",
                                "required when accent-vel or accent-note is set".into(),
                            ));
                        }
                        None
                    }
                };
                MidiRole::Click(MidiClickConfig {
                    note,
                    vel,
                    ch,
                    accent,
                })
            }
            _ => unreachable!("mode_kw is constrained to clock|click at parse"),
        };

        if !dev_seen {
            return Err(ChannelSpecError::MissingKey("dev"));
        }
        Ok(Self {
            id,
            out,
            grid,
            mode,
            swing,
            offset_ticks,
            delay_ms,
            snap_to_quantum_micro,
            bars,
        })
    }

    /// Convert the parsed spec into the runtime [`Channel`] type.
    pub fn into_channel(self) -> Result<Channel, ChannelSpecError> {
        // `dev=audio` is rejected at parse time (audit P4); by here
        // every spec is implicitly MIDI-targeted. The mode/click
        // validation already happened in `parse`, so `self.mode` is
        // the ready-to-use MidiRole (Clock or Click(MidiClickConfig)).
        // argv boundary: delay (ms) crosses into Micro via F064FD06.
        let delay = match micro_from_ms(self.delay_ms) {
            Some(m) => Micro(m.0.clamp(0, MAX_DELAY.0)),
            None => {
                return Err(ChannelSpecError::BadValue(
                    "delay",
                    format!("{} ms out of range", self.delay_ms),
                ));
            }
        };

        // Offset in ticks requires tempo to convert to Micro. Until
        // the tempo-dependent Tick→Micro path is wired, reject non-zero
        // values rather than silently storing ticks as microseconds.
        if self.offset_ticks != 0 {
            return Err(ChannelSpecError::BadValue(
                "offset",
                format!(
                    "non-zero offset ({} ticks) requires tempo-dependent conversion \
                     (not yet implemented)",
                    self.offset_ticks
                ),
            ));
        }

        Ok(Channel::Midi {
            common: ChannelCommon {
                divider: self.grid,
                shuffle: self.swing,
                delay,
                offset: Micro::ZERO,
                bar_multiplier: self.bars,
            },
            role: self.mode,
        })
    }

    /// Parsed `snap-quantum-us=N` intent, if present. Returns
    /// `Option<Quantum>` for the orchestrator to feed into
    /// `LinkSession::snap_offset_for` at startup.
    ///
    /// Audit P2 (Plan 20) dropped `snap_to_quantum` from the runtime
    /// `Channel` because the field was never read in production
    /// (`arm_channel` was test-only). The intent now lives only on
    /// `ChannelSpec`; orchestrator wiring that actually applies the
    /// snap to `Channel.offset` is a follow-up.
    pub fn snap_intent(&self) -> Option<crate::fxp::Quantum> {
        self.snap_to_quantum_micro
            .map(|m| crate::fxp::Quantum(Micro(m)))
    }
}

/// Parse a sequence of `--ch` specs in order, building the variable
/// environment as each channel's grid is resolved. Unnamed channels
/// are auto-assigned IDs (`C1`, `C2`, ...).
///
/// Returns `(id, ChannelSpec)` pairs in definition order.
pub fn parse_channels(
    specs: &[String],
) -> Result<Vec<(String, ChannelSpec)>, ChannelSpecError> {
    let mut env: Vec<(String, Grid)> = Vec::new();
    let mut result = Vec::new();
    for (i, s) in specs.iter().enumerate() {
        let spec = ChannelSpec::parse(s, &env)?;
        let id = spec
            .id
            .clone()
            .unwrap_or_else(|| format!("C{}", i + 1));
        // Reject IDs that are valid grid names — they can never be
        // referenced as variables since the parser always tries
        // Grid::from_str first.
        if id.parse::<Grid>().is_ok() {
            return Err(ChannelSpecError::BadValue(
                "id",
                format!("`{id}` is a grid literal and cannot be used as a channel ID"),
            ));
        }
        // Reject duplicate IDs — linear scan finds the first match,
        // so a duplicate would silently shadow.
        if env.iter().any(|(n, _)| n == &id) {
            return Err(ChannelSpecError::BadValue(
                "id",
                format!("duplicate channel ID `{id}`"),
            ));
        }
        env.push((id.clone(), spec.grid));
        result.push((id, spec));
    }
    Ok(result)
}

/// Parse `swing=<[TBase:]i8>`. If the value contains `:`, the part
/// before is the resolution (TBase) and after is the amount (i8).
/// Without `:`, the entire value is the amount with default
/// resolution `T8`.
fn parse_swing(v: &str) -> Result<SwingConfig, ChannelSpecError> {
    if let Some((res_str, amt_str)) = v.split_once(':') {
        let resolution = res_str
            .parse::<TBase>()
            .map_err(|e| ChannelSpecError::BadValue("swing", e))?;
        let amount = amt_str
            .parse::<i8>()
            .map_err(|e| ChannelSpecError::BadValue("swing", e.to_string()))?;
        Ok(SwingConfig {
            resolution,
            amount,
        })
    } else {
        let amount = v
            .parse::<i8>()
            .map_err(|e| ChannelSpecError::BadValue("swing", e.to_string()))?;
        Ok(SwingConfig {
            resolution: TBase::T8,
            amount,
        })
    }
}

/// Convert a finite millisecond `f64` value to `Micro`.
///
/// **User-unit shift, not Conn-composable.** `F064FD06` interprets
/// its f64 argument as **canonical seconds** (per the `time::decimal`
/// module's float-conn convention). The `× 10⁻³` here converts
/// user-input milliseconds to canonical seconds for the F-ladder
/// boundary; this shift has no Conn equivalent because the F-ladder
/// is rooted in seconds, not in arbitrary user-input units.
/// Documented argv-boundary per CLAUDE.md exception 4 — the `f64`
/// dies on the same line via `F064FD06.ceil`.
fn micro_from_ms(ms: f64) -> Option<Micro> {
    // argv boundary: ms → canonical seconds for F064FD06's input.
    let seconds = ms * 1.0e-3;
    match F064FD06.ceil(ExtendedFloat::Extend(seconds)) {
        Extended::Finite(m) => Some(m),
        Extended::PosInf | Extended::NegInf => None,
    }
}

impl Display for ChannelSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // `dev=midi` is the only supported value (audit P4); emitted
        // as a literal so the round-trip parser still sees the
        // required key.
        write!(f, "dev=midi,grid={}", self.grid)?;
        if let Some(id) = &self.id {
            write!(f, ",id={}", quote_if_needed(id))?;
        }
        if let Some(out) = &self.out {
            write!(f, ",out={}", quote_if_needed(out))?;
        }
        // mode + click keys: emit only when non-default
        // (MidiRole::Clock is the implicit default, so it's omitted).
        match &self.mode {
            MidiRole::Clock => {}
            MidiRole::Click(cfg) => {
                write!(
                    f,
                    ",mode=click,note={},vel={},mch={}",
                    cfg.note,
                    cfg.vel,
                    cfg.ch.0 + 1, // user-facing 1-based
                )?;
                if let Some(a) = &cfg.accent {
                    write!(f, ",accent-every={},accent-vel={}", a.every, a.vel)?;
                    // `accent-note` defaults to `note` at parse;
                    // emit it only when it differs so the round-trip
                    // doesn't introduce a redundant key.
                    if a.note != cfg.note {
                        write!(f, ",accent-note={}", a.note)?;
                    }
                }
            }
            // MidiRole::Cc is a spec-surface stub; the parser doesn't
            // produce it today, so the Display side stays silent.
            MidiRole::Cc(_) => {}
        }
        if self.swing.amount != 0 || self.swing.resolution != TBase::T8 {
            if self.swing.resolution == TBase::T8 {
                write!(f, ",swing={}", self.swing.amount)?;
            } else {
                write!(f, ",swing={}:{}", self.swing.resolution, self.swing.amount)?;
            }
        }
        if self.offset_ticks != 0 {
            write!(f, ",offset={}", self.offset_ticks)?;
        }
        if self.delay_ms != 0.0 {
            write!(f, ",delay={}", self.delay_ms)?;
        }
        if let Some(q) = self.snap_to_quantum_micro {
            write!(f, ",snap-quantum-us={}", q)?;
        }
        if let Some(b) = self.bars {
            write!(f, ",bars={}", b)?;
        }
        Ok(())
    }
}

fn quote_if_needed(v: &str) -> String {
    if v.chars()
        .any(|c| c == ',' || c == '=' || c.is_whitespace())
    {
        format!("\"{}\"", v)
    } else {
        v.to_string()
    }
}

/// Tokenise a `key=val[,key=val]*` string into key/value pairs.
fn tokenize(s: &str) -> Result<Vec<(String, String)>, ChannelSpecError> {
    let s = s.trim();
    if s.is_empty() {
        return Ok(vec![]);
    }
    let mut pairs = Vec::new();
    let mut chars = s.chars().peekable();

    while chars.peek().is_some() {
        while let Some(&c) = chars.peek() {
            if c.is_whitespace() || c == ',' {
                chars.next();
            } else {
                break;
            }
        }
        if chars.peek().is_none() {
            break;
        }
        let mut key = String::new();
        for c in chars.by_ref() {
            if c == '=' {
                break;
            }
            key.push(c);
        }
        let key = key.trim().to_string();
        if key.is_empty() {
            return Err(ChannelSpecError::EmptyKey);
        }
        let mut val = String::new();
        while let Some(&c) = chars.peek() {
            if c.is_whitespace() {
                chars.next();
            } else {
                break;
            }
        }
        if let Some(&'"') = chars.peek() {
            chars.next();
            let mut closed = false;
            for c in chars.by_ref() {
                if c == '"' {
                    closed = true;
                    break;
                }
                val.push(c);
            }
            if !closed {
                return Err(ChannelSpecError::Malformed(format!(
                    "unterminated quoted value for `{}`",
                    key
                )));
            }
        } else {
            while let Some(&c) = chars.peek() {
                if c == ',' {
                    break;
                }
                val.push(c);
                chars.next();
            }
            val = val.trim().to_string();
        }
        pairs.push((key, val));
    }

    Ok(pairs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    // ── Basic parsing ────────────────────────────────────────────

    #[test]
    fn parse_minimal() {
        let spec = ChannelSpec::parse("dev=midi", &[]).unwrap();
        assert_eq!(spec.grid, Grid::T4); // default
        // dev field is gone (audit P4); the parser still requires
        // the `dev=` key but stores nothing.
        assert_eq!(spec.swing, SwingConfig { resolution: TBase::T8, amount: 0 });
        assert_eq!(spec.offset_ticks, 0);
    }

    #[test]
    fn parse_rejects_dev_unknown() {
        let err = ChannelSpec::parse("dev=florble", &[]).unwrap_err();
        match err {
            ChannelSpecError::BadValue(key, val) => {
                assert_eq!(key, "dev");
                assert_eq!(val, "florble");
            }
            other => panic!("expected BadValue, got {other:?}"),
        }
    }

    #[test]
    fn parse_with_grid() {
        let spec = ChannelSpec::parse("dev=midi,grid=T16", &[]).unwrap();
        assert_eq!(spec.grid, Grid::T16);
    }

    #[test]
    fn parse_grid_dsl_expr() {
        let spec = ChannelSpec::parse("dev=midi,grid=T16&T16t", &[]).unwrap();
        assert_eq!(spec.grid, Grid::T32T); // meet(T16, T16T) = T32T
    }

    #[test]
    fn parse_grid_with_variable() {
        let env = vec![("kick".to_string(), Grid::T4)];
        let spec = ChannelSpec::parse("dev=midi,grid=kick&T16", &env).unwrap();
        assert_eq!(spec.grid, Grid::T16); // meet(T4, T16) = T16
    }

    // ── Swing ────────────────────────────────────────────────────

    #[test]
    fn parse_swing_bare_amount() {
        let spec = ChannelSpec::parse("dev=midi,swing=80", &[]).unwrap();
        assert_eq!(spec.swing, SwingConfig { resolution: TBase::T8, amount: 80 });
    }

    #[test]
    fn parse_swing_with_resolution() {
        let spec = ChannelSpec::parse("dev=midi,swing=T16:80", &[]).unwrap();
        assert_eq!(spec.swing, SwingConfig { resolution: TBase::T16, amount: 80 });
    }

    #[test]
    fn parse_swing_negative() {
        let spec = ChannelSpec::parse("dev=midi,swing=-40", &[]).unwrap();
        assert_eq!(spec.swing, SwingConfig { resolution: TBase::T8, amount: -40 });
    }

    #[test]
    fn parse_swing_explicit_negative() {
        let spec = ChannelSpec::parse("dev=midi,swing=T16:-40", &[]).unwrap();
        assert_eq!(spec.swing, SwingConfig { resolution: TBase::T16, amount: -40 });
    }

    // ── Offset ───────────────────────────────────────────────────

    #[test]
    fn parse_offset() {
        let spec = ChannelSpec::parse("dev=midi,offset=20", &[]).unwrap();
        assert_eq!(spec.offset_ticks, 20);
    }

    #[test]
    fn parse_offset_negative() {
        let spec = ChannelSpec::parse("dev=midi,offset=-5", &[]).unwrap();
        assert_eq!(spec.offset_ticks, -5);
    }

    // ── parse_channels ───────────────────────────────────────────

    #[test]
    fn parse_channels_positional_var() {
        let specs = vec![
            "dev=midi,grid=T4".to_string(),
            "dev=midi,grid=C1&T16".to_string(),
        ];
        let result = parse_channels(&specs).unwrap();
        assert_eq!(result[0].0, "C1");
        assert_eq!(result[0].1.grid, Grid::T4);
        assert_eq!(result[1].0, "C2");
        assert_eq!(result[1].1.grid, Grid::T16); // meet(T4, T16)
    }

    #[test]
    fn parse_channels_named_var() {
        let specs = vec![
            "id=kick,dev=midi,grid=T4".to_string(),
            "dev=midi,grid=kick&T16".to_string(),
        ];
        let result = parse_channels(&specs).unwrap();
        assert_eq!(result[0].0, "kick");
        assert_eq!(result[1].1.grid, Grid::T16);
    }

    #[test]
    fn parse_channels_forward_ref_errors() {
        let specs = vec![
            "dev=midi,grid=T4".to_string(),
            "dev=midi,grid=C3&T16".to_string(), // C3 doesn't exist
        ];
        let err = parse_channels(&specs).unwrap_err();
        assert!(matches!(err, ChannelSpecError::BadValue("grid", _)));
    }

    #[test]
    fn parse_channels_default_grid() {
        let specs = vec!["dev=midi".to_string()];
        let result = parse_channels(&specs).unwrap();
        assert_eq!(result[0].1.grid, Grid::T4);
    }

    #[test]
    fn parse_channels_three_channels() {
        let specs = vec![
            "dev=midi,grid=T4".to_string(),
            "dev=midi,grid=C1&T16".to_string(),
            "dev=midi,grid=C2|T8t".to_string(),
        ];
        let result = parse_channels(&specs).unwrap();
        // C1 = T4, C2 = meet(T4, T16) = T16, C3 = join(T16, T8T) = T4
        assert_eq!(result[2].1.grid, Grid::T4);
    }

    #[test]
    fn parse_channels_rejects_duplicate_id() {
        let specs = vec![
            "id=kick,dev=midi,grid=T4".to_string(),
            "id=kick,dev=midi,grid=T8".to_string(),
        ];
        let err = parse_channels(&specs).unwrap_err();
        assert!(matches!(err, ChannelSpecError::BadValue("id", _)));
    }

    #[test]
    fn parse_channels_rejects_grid_literal_id() {
        let specs = vec!["id=T16,dev=midi".to_string()];
        let err = parse_channels(&specs).unwrap_err();
        assert!(matches!(err, ChannelSpecError::BadValue("id", _)));
    }

    // ── Error cases ──────────────────────────────────────────────

    #[test]
    fn parse_rejects_unknown_key() {
        let err = ChannelSpec::parse("dev=midi,unknown=x", &[]).unwrap_err();
        assert_eq!(err, ChannelSpecError::UnknownKey("unknown".into()));
    }

    #[test]
    fn parse_rejects_missing_dev() {
        let err = ChannelSpec::parse("grid=T16", &[]).unwrap_err();
        assert_eq!(err, ChannelSpecError::MissingKey("dev"));
    }

    #[test]
    fn parse_rejects_dev_audio() {
        let err = ChannelSpec::parse("dev=audio", &[]).unwrap_err();
        assert_eq!(err, ChannelSpecError::AudioDeferred);
    }

    #[test]
    fn parse_rejects_bad_grid() {
        let err = ChannelSpec::parse("dev=midi,grid=T3", &[]).unwrap_err();
        assert!(matches!(err, ChannelSpecError::BadValue("grid", _)));
    }

    // ── Delay ────────────────────────────────────────────────────

    #[test]
    fn into_channel_clamps_delay() {
        let spec = ChannelSpec::parse("dev=midi,delay=500", &[]).unwrap();
        let ch = spec.into_channel().unwrap();
        assert_eq!(ch.common().delay, MAX_DELAY);
    }

    #[test]
    fn into_channel_negative_delay_clamps_to_zero() {
        let spec = ChannelSpec::parse("dev=midi,delay=-50", &[]).unwrap();
        let ch = spec.into_channel().unwrap();
        assert_eq!(ch.common().delay, Micro(0));
    }

    // ── Display round-trip ───────────────────────────────────────

    #[test]
    fn display_round_trip_minimal() {
        let spec = ChannelSpec::parse("dev=midi", &[]).unwrap();
        let s = spec.to_string();
        let reparsed = ChannelSpec::parse(&s, &[]).unwrap();
        assert_eq!(spec, reparsed);
    }

    #[test]
    fn display_round_trip_full() {
        let spec = ChannelSpec::parse("dev=midi,grid=T16,swing=T16:80,offset=20,delay=5", &[]).unwrap();
        let s = spec.to_string();
        let reparsed = ChannelSpec::parse(&s, &[]).unwrap();
        assert_eq!(spec, reparsed);
    }

    #[test]
    fn display_swing_default_res_omits_resolution() {
        let spec = ChannelSpec::parse("dev=midi,swing=80", &[]).unwrap();
        let s = spec.to_string();
        assert!(s.contains("swing=80"), "got: {s}");
        assert!(!s.contains("swing=t8:"), "should omit default resolution, got: {s}");
    }

    #[test]
    fn display_swing_explicit_res_includes_resolution() {
        let spec = ChannelSpec::parse("dev=midi,swing=T16:80", &[]).unwrap();
        let s = spec.to_string();
        assert!(s.contains("swing=t16:80"), "got: {s}");
    }

    #[test]
    fn display_quotes_values_with_spaces() {
        let spec = ChannelSpec::parse(r#"dev=midi,out="IAC Bus 1""#, &[]).unwrap();
        let s = spec.to_string();
        assert!(s.contains(r#"out="IAC Bus 1""#), "got: {s}");
        let reparsed = ChannelSpec::parse(&s, &[]).unwrap();
        assert_eq!(spec, reparsed);
    }

    #[test]
    fn display_quotes_values_with_commas() {
        let spec =
            ChannelSpec::parse(r#"dev=midi,out="port,with,commas""#, &[]).unwrap();
        let s = spec.to_string();
        assert!(s.contains(r#"out="port,with,commas""#), "got: {s}");
        let reparsed = ChannelSpec::parse(&s, &[]).unwrap();
        assert_eq!(spec, reparsed);
    }

    // ── Proptest ─────────────────────────────────────────────────

    fn arb_grid() -> impl Strategy<Value = Grid> {
        prop::sample::select(Grid::ALL.as_slice())
    }

    fn arb_tbase() -> impl Strategy<Value = TBase> {
        prop::sample::select(TBase::ALL.as_slice())
    }

    /// Generate a `MidiRole` reachable from the spec parser:
    /// `Clock` or `Click(MidiClickConfig)` with arbitrary
    /// note/vel/ch and an optional accent.
    ///
    /// `accent.every` spans the full `NonZeroU32` domain — the
    /// round-trip property is u32-shape-preserving (parse-as-u32,
    /// Display via `Display for NonZeroU32`), so the entire domain
    /// is safe to sample. Per CLAUDE.md: don't bound to "keep
    /// things small," only to avoid documented hazards.
    fn arb_mode() -> impl Strategy<Value = MidiRole> {
        let click = (
            0u8..=127,
            1u8..=127,
            0u8..=15,
            prop::option::of((
                any::<u32>().prop_filter("every > 0", |&n| n > 0),
                0u8..=127,
                1u8..=127,
            )),
        )
            .prop_map(|(note, vel, ch, accent_triple)| {
                let accent = accent_triple.map(|(every, an, av)| MidiClickAccent {
                    every: NonZeroU32::new(every).unwrap(),
                    note: U7(an),
                    vel: U7(av),
                });
                MidiRole::Click(MidiClickConfig {
                    note: U7(note),
                    vel: U7(vel),
                    ch: U4(ch),
                    accent,
                })
            });
        prop_oneof![Just(MidiRole::Clock), click]
    }

    /// Full `NonZeroU16` domain for `bars` — same justification as
    /// `arb_mode`'s `every`: parser is u16-shape-preserving, no
    /// arithmetic hazards in the round-trip path.
    fn arb_bars() -> impl Strategy<Value = Option<NonZeroU16>> {
        prop::option::of(
            any::<u16>().prop_filter("bars > 0", |&n| n > 0)
                .prop_map(|n| NonZeroU16::new(n).unwrap()),
        )
    }

    fn arb_spec() -> impl Strategy<Value = ChannelSpec> {
        (
            arb_grid(),
            prop::option::of("[a-zA-Z][a-zA-Z0-9]{0,8}"),
            prop::option::of("[a-zA-Z0-9]{1,10}"),
            arb_tbase(),
            any::<i8>(),
            any::<i32>(),
            (0u32..=300).prop_map(|n| n as f64),
            prop::option::of(any::<i32>().prop_map(|n| n as i64)),
            arb_mode(),
            arb_bars(),
        )
            .prop_map(
                |(
                    grid,
                    id,
                    out,
                    swing_res,
                    swing_amt,
                    offset_ticks,
                    delay_ms,
                    snap,
                    mode,
                    bars,
                )| {
                    ChannelSpec {
                        id,
                        out,
                        grid,
                        mode,
                        swing: SwingConfig {
                            resolution: swing_res,
                            amount: swing_amt,
                        },
                        offset_ticks,
                        delay_ms,
                        snap_to_quantum_micro: snap,
                        bars,
                    }
                },
            )
    }

    proptest! {
        /// Plan 14 property `spec_round_trip`: the `Display` impl
        /// emits parser-stable output, so `parse(spec.to_string())`
        /// recovers the same spec for every value the strategy
        /// generates. Plan 2026-04-25-03 extends the strategy with
        /// `mode=click` (note/vel/mch/accent-*) and `bars` so
        /// round-trip pins both the existing and new keys.
        #[test]
        fn spec_round_trip(spec in arb_spec()) {
            let s = spec.to_string();
            let parsed = ChannelSpec::parse(&s, &[])
                .map_err(|e| TestCaseError::fail(format!("parse `{}`: {}", s, e)))?;
            prop_assert_eq!(parsed, spec);
        }
    }

    // ── Plan 2026-04-25-03: mode=click + bars spot checks ──────

    #[test]
    fn parse_default_mode_is_clock() {
        let spec = ChannelSpec::parse("dev=midi,grid=t4", &[]).unwrap();
        assert_eq!(spec.mode, MidiRole::Clock);
    }

    #[test]
    fn parse_full_click_spec() {
        let s = "dev=midi,mode=click,grid=t4,note=37,vel=80,mch=10,\
                 accent-every=4,accent-note=38,accent-vel=120";
        let spec = ChannelSpec::parse(s, &[]).expect("parse");
        let cfg = match spec.mode {
            MidiRole::Click(c) => c,
            other => panic!("expected Click(Midi), got {:?}", other),
        };
        assert_eq!(cfg.note, U7(37));
        assert_eq!(cfg.vel, U7(80));
        assert_eq!(cfg.ch, U4(9), "mch=10 → ch=9 (zero-based)");
        let accent = cfg.accent.expect("accent set");
        assert_eq!(accent.every.get(), 4);
        assert_eq!(accent.note, U7(38));
        assert_eq!(accent.vel, U7(120));
    }

    #[test]
    fn parse_click_defaults_mch_to_10() {
        let spec = ChannelSpec::parse("dev=midi,mode=click,grid=t4,note=76,vel=100", &[])
            .unwrap();
        let cfg = match spec.mode {
            MidiRole::Click(c) => c,
            _ => panic!(),
        };
        assert_eq!(cfg.ch, U4(9));
    }

    #[test]
    fn parse_click_accent_note_defaults_to_note() {
        let s = "dev=midi,mode=click,grid=t4,note=37,vel=70,accent-every=4,accent-vel=120";
        let spec = ChannelSpec::parse(s, &[]).unwrap();
        let cfg = match spec.mode {
            MidiRole::Click(c) => c,
            _ => panic!(),
        };
        let accent = cfg.accent.unwrap();
        assert_eq!(accent.note, U7(37), "accent-note absent → defaults to note");
    }

    #[test]
    fn parse_rejects_clock_with_click_keys() {
        let err = ChannelSpec::parse("dev=midi,grid=t4,note=42", &[]).unwrap_err();
        match err {
            ChannelSpecError::BadValue(key, msg) => {
                assert_eq!(key, "note");
                assert!(msg.contains("mode=click"), "got: {msg}");
            }
            other => panic!("unexpected: {:?}", other),
        }
    }

    #[test]
    fn parse_rejects_click_missing_note() {
        let err = ChannelSpec::parse("dev=midi,mode=click,grid=t4,vel=80", &[]).unwrap_err();
        assert_eq!(err, ChannelSpecError::MissingKey("note"));
    }

    #[test]
    fn parse_rejects_click_missing_vel() {
        let err = ChannelSpec::parse("dev=midi,mode=click,grid=t4,note=37", &[]).unwrap_err();
        assert_eq!(err, ChannelSpecError::MissingKey("vel"));
    }

    #[test]
    fn parse_rejects_vel_zero() {
        let err =
            ChannelSpec::parse("dev=midi,mode=click,grid=t4,note=37,vel=0", &[]).unwrap_err();
        match err {
            ChannelSpecError::BadValue(key, _) => assert_eq!(key, "vel"),
            other => panic!("unexpected: {:?}", other),
        }
    }

    #[test]
    fn parse_rejects_note_above_127() {
        let err =
            ChannelSpec::parse("dev=midi,mode=click,grid=t4,note=200,vel=80", &[]).unwrap_err();
        match err {
            ChannelSpecError::BadValue(key, _) => assert_eq!(key, "note"),
            other => panic!("unexpected: {:?}", other),
        }
    }

    #[test]
    fn parse_rejects_mch_zero_or_above_16() {
        let err =
            ChannelSpec::parse("dev=midi,mode=click,grid=t4,note=37,vel=80,mch=0", &[])
                .unwrap_err();
        match err {
            ChannelSpecError::BadValue(key, _) => assert_eq!(key, "mch"),
            _ => panic!("expected BadValue"),
        }
        let err =
            ChannelSpec::parse("dev=midi,mode=click,grid=t4,note=37,vel=80,mch=17", &[])
                .unwrap_err();
        match err {
            ChannelSpecError::BadValue(key, _) => assert_eq!(key, "mch"),
            _ => panic!("expected BadValue"),
        }
    }

    #[test]
    fn parse_rejects_accent_every_zero() {
        let s = "dev=midi,mode=click,grid=t4,note=37,vel=80,accent-every=0,accent-vel=120";
        let err = ChannelSpec::parse(s, &[]).unwrap_err();
        match err {
            ChannelSpecError::BadValue(key, _) => assert_eq!(key, "accent-every"),
            _ => panic!("expected BadValue"),
        }
    }

    #[test]
    fn parse_requires_accent_vel_when_accent_every_set() {
        let s = "dev=midi,mode=click,grid=t4,note=37,vel=80,accent-every=4";
        let err = ChannelSpec::parse(s, &[]).unwrap_err();
        assert_eq!(err, ChannelSpecError::MissingKey("accent-vel"));
    }

    #[test]
    fn parse_accepts_bars_on_any_divider() {
        let spec = ChannelSpec::parse("dev=midi,grid=t8,bars=3", &[]).unwrap();
        assert_eq!(spec.bars, NonZeroU16::new(3));
        assert_eq!(spec.grid, Grid::T8);
    }

    #[test]
    fn parse_rejects_bars_zero() {
        let err = ChannelSpec::parse("dev=midi,grid=t1,bars=0", &[]).unwrap_err();
        match err {
            ChannelSpecError::BadValue(key, _) => assert_eq!(key, "bars"),
            _ => panic!("expected BadValue"),
        }
    }

    #[test]
    fn parse_rejects_bars_above_u16_max() {
        let err = ChannelSpec::parse("dev=midi,grid=t1,bars=70000", &[]).unwrap_err();
        match err {
            ChannelSpecError::BadValue(key, _) => assert_eq!(key, "bars"),
            _ => panic!("expected BadValue"),
        }
    }

    #[test]
    fn into_channel_bars_round_trips_via_nonzero() {
        let spec = ChannelSpec::parse("dev=midi,grid=t1,bars=4", &[]).unwrap();
        let ch = spec.into_channel().unwrap();
        assert_eq!(ch.common().bar_multiplier, NonZeroU16::new(4));
    }

    #[test]
    fn into_channel_click_maps_mch_to_zero_based() {
        let spec =
            ChannelSpec::parse("dev=midi,mode=click,grid=t4,note=37,vel=80,mch=10", &[])
                .unwrap();
        let ch = spec.into_channel().unwrap();
        match ch {
            Channel::Midi { role: MidiRole::Click(cfg), .. } => assert_eq!(cfg.ch, U4(9)),
            _ => panic!("expected Channel::Midi {{ role: Click(_) }}"),
        }
    }

    #[test]
    fn into_channel_click_no_accent_when_accent_every_absent() {
        let spec =
            ChannelSpec::parse("dev=midi,mode=click,grid=t4,note=37,vel=80", &[]).unwrap();
        let ch = spec.into_channel().unwrap();
        match ch {
            Channel::Midi { role: MidiRole::Click(cfg), .. } => assert!(cfg.accent.is_none()),
            _ => panic!(),
        }
    }

    // ── Plan 20: snap_intent accessor (audit P2). ──

    #[test]
    fn snap_intent_none_when_key_absent() {
        let spec = ChannelSpec::parse("dev=midi,grid=t4", &[]).unwrap();
        assert!(spec.snap_intent().is_none());
    }

    proptest! {
        /// `snap-quantum-us=N` parsed back through `snap_intent()`
        /// recovers `Some(Quantum(Micro(N)))` for every signed `i64`.
        /// Generator spans the full domain — the parse path stores
        /// the raw `i64` and `snap_intent` just rewraps; bounding
        /// would hide nothing.
        #[test]
        fn snap_intent_round_trips_through_spec(n in any::<i64>()) {
            let s = format!("dev=midi,grid=t4,snap-quantum-us={n}");
            let spec = ChannelSpec::parse(&s, &[])
                .map_err(|e| TestCaseError::fail(format!("parse `{s}`: {e}")))?;
            prop_assert_eq!(
                spec.snap_intent(),
                Some(crate::fxp::Quantum(Micro(n)))
            );
        }
    }
}
