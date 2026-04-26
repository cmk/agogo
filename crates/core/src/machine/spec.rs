//! Channel-spec mini-language for `agogo run --ch <spec>`.
//!
//! Grammar: `key=val[,key=val]*`. Whitespace tolerated; `=` and `,`
//! separate; values may be quoted `"..."` to embed spaces or commas.
//!
//! Required keys: `dev`. Optional keys: `grid` (DSL expression,
//! default `T4`), `id`, `out`, `swing` (`[TBase:]i8`, default
//! `T8:0`), `offset` (signed `i32` ticks — **note:** non-zero
//! values are currently rejected until the tempo-dependent
//! Tick→Micro conversion is wired), `delay` (ms),
//! `snap-quantum-us`. Unknown keys are hard errors so typos are
//! caught early.
//!
//! The `grid` value is parsed via `dsl::parse` with a channel
//! environment, so expressions like `kick&T16` that reference
//! earlier channels are valid. Use [`parse_channels`] to resolve
//! a sequence of specs in order, building the environment as it goes.
//!
//! [`ChannelSpec`] holds the parsed form; [`ChannelSpec::into_channel`]
//! converts to a [`Channel`] at the CLI argv boundary, where the only
//! `f64` field (`delay_ms`) crosses via the `F64F06` Conn per CLAUDE.md
//! float exception 4.

use std::fmt::{self, Display};

use crate::channel::transform::MAX_DELAY;
use crate::channel::{Channel, ChannelMode};
use crate::dsl;
use crate::fxp::{Extended, ExtendedFloat, F64F06, Micro};
use crate::time::grid::Grid;
use crate::time::swing::SwingConfig;
use crate::time::tbase::TBase;

/// Parsed `--ch` spec.
#[derive(Debug, Clone, PartialEq)]
pub struct ChannelSpec {
    /// Optional human-readable identifier. Free-form string.
    pub id: Option<String>,
    /// Routing destination kind. `Audio` is reserved for v0.4.
    pub dev: ChannelDev,
    /// Device-specific routing target (port name, channel index).
    pub out: Option<String>,
    /// Grid — resolved from a DSL expression at parse time.
    pub grid: Grid,
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
}

/// Output device kind for a channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelDev {
    Midi,
    Audio,
}

impl Display for ChannelDev {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Midi => f.write_str("midi"),
            Self::Audio => f.write_str("audio"),
        }
    }
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
        let mut dev: Option<ChannelDev> = None;
        let mut out: Option<String> = None;
        let mut grid_str: Option<String> = None;
        let mut swing: SwingConfig = SwingConfig {
            resolution: TBase::T8,
            amount: 0,
        };
        let mut offset_ticks: i32 = 0;
        let mut delay_ms: f64 = 0.0; // argv boundary
        let mut snap_to_quantum_micro: Option<i64> = None;

        for (k, v) in pairs {
            match k.as_str() {
                "id" => id = Some(v),
                "dev" => {
                    dev = Some(match v.as_str() {
                        "midi" => ChannelDev::Midi,
                        "audio" => return Err(ChannelSpecError::AudioDeferred),
                        other => {
                            return Err(ChannelSpecError::BadValue("dev", other.to_string()));
                        }
                    });
                }
                "out" => out = Some(v),
                "grid" => {
                    grid_str = Some(v);
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

        Ok(Self {
            id,
            dev: dev.ok_or(ChannelSpecError::MissingKey("dev"))?,
            out,
            grid,
            swing,
            offset_ticks,
            delay_ms,
            snap_to_quantum_micro,
        })
    }

    /// Convert the parsed spec into the runtime [`Channel`] type.
    pub fn into_channel(self) -> Result<Channel, ChannelSpecError> {
        let mode = match self.dev {
            ChannelDev::Midi => ChannelMode::MidiClock,
            ChannelDev::Audio => return Err(ChannelSpecError::AudioDeferred),
        };
        // argv boundary: delay (ms) crosses into Micro via F64F06.
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

        Ok(Channel {
            mode,
            divider: self.grid,
            shuffle: self.swing,
            delay,
            offset: Micro::ZERO,
            snap_to_quantum: self
                .snap_to_quantum_micro
                .map(|m| crate::fxp::Quantum(Micro(m))),
        })
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

/// Convert a finite millisecond `f64` value to `Micro` via the
/// `F64F06` Conn.
fn micro_from_ms(ms: f64) -> Option<Micro> {
    // argv boundary
    let seconds = ms * 1.0e-3; // argv boundary
    match F64F06.ceil(ExtendedFloat::Finite(seconds)) {
        Extended::Finite(m) => Some(m),
        Extended::PosInf | Extended::NegInf => None,
    }
}

impl Display for ChannelSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "dev={},grid={}", self.dev, self.grid)?;
        if let Some(id) = &self.id {
            write!(f, ",id={}", quote_if_needed(id))?;
        }
        if let Some(out) = &self.out {
            write!(f, ",out={}", quote_if_needed(out))?;
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
        assert_eq!(spec.dev, ChannelDev::Midi);
        assert_eq!(spec.swing, SwingConfig { resolution: TBase::T8, amount: 0 });
        assert_eq!(spec.offset_ticks, 0);
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
        assert_eq!(ch.delay, MAX_DELAY);
    }

    #[test]
    fn into_channel_negative_delay_clamps_to_zero() {
        let spec = ChannelSpec::parse("dev=midi,delay=-50", &[]).unwrap();
        let ch = spec.into_channel().unwrap();
        assert_eq!(ch.delay, Micro(0));
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

    fn arb_dev() -> impl Strategy<Value = ChannelDev> {
        Just(ChannelDev::Midi)
    }

    fn arb_grid() -> impl Strategy<Value = Grid> {
        prop::sample::select(Grid::ALL.as_slice())
    }

    fn arb_tbase() -> impl Strategy<Value = TBase> {
        prop::sample::select(TBase::ALL.as_slice())
    }

    fn arb_spec() -> impl Strategy<Value = ChannelSpec> {
        (
            arb_dev(),
            arb_grid(),
            prop::option::of("[a-zA-Z][a-zA-Z0-9]{0,8}"),
            prop::option::of("[a-zA-Z0-9]{1,10}"),
            arb_tbase(),
            any::<i8>(),
            any::<i32>(),
            (0u32..=300).prop_map(|n| n as f64),
            prop::option::of(any::<i32>().prop_map(|n| n as i64)),
        )
            .prop_map(
                |(dev, grid, id, out, swing_res, swing_amt, offset_ticks, delay_ms, snap)| {
                    ChannelSpec {
                        id,
                        dev,
                        out,
                        grid,
                        swing: SwingConfig {
                            resolution: swing_res,
                            amount: swing_amt,
                        },
                        offset_ticks,
                        delay_ms,
                        snap_to_quantum_micro: snap,
                    }
                },
            )
    }

    proptest! {
        #[test]
        fn spec_round_trip(spec in arb_spec()) {
            let s = spec.to_string();
            let parsed = ChannelSpec::parse(&s, &[])
                .map_err(|e| TestCaseError::fail(format!("parse `{}`: {}", s, e)))?;
            prop_assert_eq!(parsed, spec);
        }
    }
}
