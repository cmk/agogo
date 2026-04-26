//! Channel-spec mini-language for `agogo run --ch <spec>`.
//!
//! Grammar: `key=val[,key=val]*`. Whitespace tolerated; `=` and `,`
//! separate; values may be quoted `"..."` to embed spaces or commas.
//!
//! Required keys: `grid`, `dev`. Optional keys: `id`, `out`, `delay`
//! (latency compensation in ms), `snap-quantum-us`. Unknown keys are
//! hard errors so typos are caught early.
//!
//! The `grid` value is currently a plain grid name (`t16`, `t8q`,
//! etc.) parsed via `Grid::from_str`. When the DSL parser lands
//! (Plan 16), it will accept full DSL expressions with lattice ops,
//! swing, and offset (`T16~T16:80@-5`).
//!
//! [`ChannelSpec`] holds the parsed form; [`ChannelSpec::into_channel`]
//! converts to a [`Channel`] at the CLI argv boundary, where the only
//! `f64` field (`delay_ms`) crosses via the `F64F06` Conn per CLAUDE.md
//! float exception 4.

use std::fmt::{self, Display};

use crate::channel::transform::MAX_DELAY;
use crate::channel::{Channel, ChannelMode};
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
    /// Grid expression. Currently a plain `Grid` name; Plan 16 will
    /// upgrade this to a full DSL expression (grid + swing + offset).
    pub grid: Grid,
    /// Positive delay compensation in milliseconds. Clamped to
    /// MAX_DELAY (300 ms) in `into_channel`.
    pub delay_ms: f64, // argv boundary
    /// Optional quantum snap, in microseconds (Link-aware channels).
    pub snap_to_quantum_micro: Option<i64>,
}

/// Output device kind for a channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelDev {
    /// MIDI sink (midir back-end in v0.1; CoreMIDI/JACK/WinMM
    /// post-v0.5 per `version-0.1.md`).
    Midi,
    /// Audio sink for CV pulse / analog LFO. Reserved — Plan 14
    /// rejects this with [`ChannelSpecError::AudioDeferred`]; v0.4's
    /// `out/audio` lights it up.
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
    /// Returns [`ChannelSpecError::AudioDeferred`] on `dev=audio`
    /// (reserved for v0.4) — call sites still get a `Result` so they
    /// can format the deferral cleanly to the user.
    pub fn parse(s: &str) -> Result<Self, ChannelSpecError> {
        let pairs = tokenize(s)?;

        let mut id: Option<String> = None;
        let mut dev: Option<ChannelDev> = None;
        let mut out: Option<String> = None;
        let mut grid: Option<Grid> = None;
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
                    grid = Some(
                        v.parse::<Grid>()
                            .map_err(|e| ChannelSpecError::BadValue("grid", e.to_string()))?,
                    );
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

        Ok(Self {
            id,
            dev: dev.ok_or(ChannelSpecError::MissingKey("dev"))?,
            out,
            grid: grid.ok_or(ChannelSpecError::MissingKey("grid"))?,
            delay_ms,
            snap_to_quantum_micro,
        })
    }

    /// Convert the parsed spec into the runtime [`Channel`] type. The
    /// only `f64` in this crate dies here at the `// argv boundary`
    /// via the `F64F06` Conn (per CLAUDE.md float exception 4).
    pub fn into_channel(self) -> Result<Channel, ChannelSpecError> {
        let mode = match self.dev {
            ChannelDev::Midi => ChannelMode::MidiClock,
            ChannelDev::Audio => return Err(ChannelSpecError::AudioDeferred),
        };
        // argv boundary: delay (ms) crosses into Micro via F64F06.
        let delay = micro_from_ms(self.delay_ms);
        let delay = Micro(delay.0.clamp(0, MAX_DELAY.0));

        Ok(Channel {
            mode,
            divider: self.grid,
            shuffle: SwingConfig {
                resolution: TBase::T16,
                amount: 0,
            },
            delay,
            offset: Micro::ZERO,
            snap_to_quantum: self
                .snap_to_quantum_micro
                .map(|m| crate::fxp::Quantum(Micro(m))),
        })
    }
}

/// Convert a finite millisecond `f64` value to `Micro` via the
/// `F64F06` Conn. `F64F06` operates on seconds, so multiply by 1e-3
/// first; the parser already validated finiteness, so an
/// `Extended::PosInf` / `Extended::NegInf` here means the user
/// asked for a value billions-of-years out of `Micro`'s ±i64 range
/// — saturate rather than panic.
fn micro_from_ms(ms: f64) -> Micro {
    // argv boundary
    let seconds = ms * 1.0e-3; // argv boundary
    match F64F06.ceil(ExtendedFloat::Finite(seconds)) {
        Extended::Finite(m) => m,
        Extended::PosInf => Micro(i64::MAX),
        Extended::NegInf => Micro(i64::MIN),
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
        if self.delay_ms != 0.0 {
            write!(f, ",delay={}", self.delay_ms)?;
        }
        if let Some(q) = self.snap_to_quantum_micro {
            write!(f, ",snap-quantum-us={}", q)?;
        }
        Ok(())
    }
}

/// Wrap `v` in `"..."` if it contains any of the tokenizer's
/// separators (`,`, `=`, whitespace) so the round-trip survives.
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
/// Tolerates whitespace; supports `"..."`-quoted values for embedded
/// commas / equals.
fn tokenize(s: &str) -> Result<Vec<(String, String)>, ChannelSpecError> {
    let s = s.trim();
    if s.is_empty() {
        return Ok(vec![]);
    }
    let mut pairs = Vec::new();
    let mut chars = s.chars().peekable();

    while chars.peek().is_some() {
        // Skip leading whitespace and commas.
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
        // Read key up to '='.
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
        // Read value, respecting quotes.
        let mut val = String::new();
        // Skip leading whitespace inside the value.
        while let Some(&c) = chars.peek() {
            if c.is_whitespace() {
                chars.next();
            } else {
                break;
            }
        }
        if let Some(&'"') = chars.peek() {
            chars.next(); // consume opening quote
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
            // Unquoted: read up to next ',' or end.
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

    #[test]
    fn parse_full_spec() {
        let s = "grid=t32t,dev=midi,out=IAC Bus 1,delay=2.5";
        let spec = ChannelSpec::parse(s).expect("parse");
        assert_eq!(spec.grid, Grid::T32T);
        assert_eq!(spec.dev, ChannelDev::Midi);
        assert_eq!(spec.out.as_deref(), Some("IAC Bus 1"));
        assert!((spec.delay_ms - 2.5).abs() < 1e-9);
        assert_eq!(spec.snap_to_quantum_micro, None);
    }

    #[test]
    fn parse_quintuplet_grid() {
        let spec = ChannelSpec::parse("dev=midi,grid=t8q").unwrap();
        assert_eq!(spec.grid, Grid::T8Q);
    }

    #[test]
    fn parse_quoted_value_with_spaces_and_commas() {
        let s = r#"dev=midi,grid=t32t,out="IAC Bus 1, port 2""#;
        let spec = ChannelSpec::parse(s).expect("parse");
        assert_eq!(spec.out.as_deref(), Some("IAC Bus 1, port 2"));
    }

    #[test]
    fn parse_rejects_unknown_key() {
        let err = ChannelSpec::parse("dev=midi,grid=t32t,unknown=x").unwrap_err();
        assert_eq!(err, ChannelSpecError::UnknownKey("unknown".into()));
    }

    #[test]
    fn parse_rejects_legacy_keys() {
        // div, swing, swing-res are no longer accepted — use grid=
        // (plain grid name now; full DSL expressions in a future PR).
        let err = ChannelSpec::parse("dev=midi,div=t32t").unwrap_err();
        assert_eq!(err, ChannelSpecError::UnknownKey("div".into()));
        let err = ChannelSpec::parse("dev=midi,grid=t32t,swing=80").unwrap_err();
        assert_eq!(err, ChannelSpecError::UnknownKey("swing".into()));
        let err = ChannelSpec::parse("dev=midi,grid=t32t,swing-res=t8").unwrap_err();
        assert_eq!(err, ChannelSpecError::UnknownKey("swing-res".into()));
    }

    #[test]
    fn parse_rejects_missing_required() {
        let err = ChannelSpec::parse("dev=midi").unwrap_err();
        assert_eq!(err, ChannelSpecError::MissingKey("grid"));
        let err = ChannelSpec::parse("grid=t32t").unwrap_err();
        assert_eq!(err, ChannelSpecError::MissingKey("dev"));
    }

    #[test]
    fn parse_rejects_dev_audio_with_audio_deferred() {
        let err = ChannelSpec::parse("dev=audio,grid=t32t").unwrap_err();
        assert_eq!(err, ChannelSpecError::AudioDeferred);
    }

    #[test]
    fn parse_rejects_bad_grid() {
        let err = ChannelSpec::parse("dev=midi,grid=t3").unwrap_err();
        match err {
            ChannelSpecError::BadValue(key, _) => assert_eq!(key, "grid"),
            other => panic!("unexpected: {:?}", other),
        }
    }

    #[test]
    fn into_channel_clamps_delay() {
        // 500 ms exceeds MAX_DELAY (300 ms); clamped to 300 ms.
        let spec = ChannelSpec::parse("dev=midi,grid=t32t,delay=500").unwrap();
        let ch = spec.into_channel().unwrap();
        assert_eq!(ch.delay, MAX_DELAY);
    }

    #[test]
    fn into_channel_negative_delay_clamps_to_zero() {
        let spec = ChannelSpec::parse("dev=midi,grid=t32t,delay=-50").unwrap();
        let ch = spec.into_channel().unwrap();
        assert_eq!(ch.delay, Micro(0));
    }

    #[test]
    fn display_round_trip_minimal() {
        let spec = ChannelSpec::parse("dev=midi,grid=t32t").unwrap();
        let s = spec.to_string();
        let reparsed = ChannelSpec::parse(&s).unwrap();
        assert_eq!(spec, reparsed);
    }

    #[test]
    fn display_quotes_values_with_spaces() {
        let spec = ChannelSpec::parse(r#"dev=midi,grid=t32t,out="IAC Bus 1""#).unwrap();
        let s = spec.to_string();
        assert!(s.contains(r#"out="IAC Bus 1""#), "got: {s}");
        let reparsed = ChannelSpec::parse(&s).unwrap();
        assert_eq!(spec, reparsed);
    }

    #[test]
    fn display_quotes_values_with_commas() {
        let spec =
            ChannelSpec::parse(r#"dev=midi,grid=t32t,out="port,with,commas""#).unwrap();
        let s = spec.to_string();
        assert!(s.contains(r#"out="port,with,commas""#), "got: {s}");
        let reparsed = ChannelSpec::parse(&s).unwrap();
        assert_eq!(spec, reparsed);
    }

    fn arb_dev() -> impl Strategy<Value = ChannelDev> {
        Just(ChannelDev::Midi)
    }

    fn arb_grid() -> impl Strategy<Value = Grid> {
        prop::sample::select(Grid::ALL.as_slice())
    }

    fn arb_spec() -> impl Strategy<Value = ChannelSpec> {
        let ident = r#"[a-zA-Z0-9 ,=_]{1,15}"#;
        (
            arb_dev(),
            arb_grid(),
            prop::option::of(ident),
            prop::option::of(ident),
            (0u32..=300).prop_map(|n| n as f64),
            prop::option::of(any::<i32>().prop_map(|n| n as i64)),
        )
            .prop_map(|(dev, grid, id, out, delay_ms, snap_to_quantum_micro)| {
                ChannelSpec {
                    id,
                    dev,
                    out,
                    grid,
                    delay_ms,
                    snap_to_quantum_micro,
                }
            })
    }

    proptest! {
        /// Plan 14 property `spec_round_trip`: the `Display` impl
        /// emits parser-stable output, so `parse(spec.to_string())`
        /// recovers the same spec for every value the strategy
        /// generates.
        #[test]
        fn spec_round_trip(spec in arb_spec()) {
            let s = spec.to_string();
            let parsed = ChannelSpec::parse(&s)
                .map_err(|e| TestCaseError::fail(format!("parse `{}`: {}", s, e)))?;
            prop_assert_eq!(parsed, spec);
        }
    }
}
