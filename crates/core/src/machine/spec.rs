//! Channel-spec mini-language for `agogo run --ch <spec>`.
//!
//! Grammar: `key=val[,key=val]*`. Whitespace tolerated; `=` and `,`
//! separate; values may be quoted `"..."` to embed spaces or commas.
//!
//! Required keys: `div`, `dev`. Optional keys: `id`, `out`, `swing`
//! (i8 tick offset), `swing-res` (binary resolution, default `t16`),
//! `shift-ms`, `offset-ms`, `snap-quantum-us`. Unknown keys are hard
//! errors so typos are caught early.
//!
//! [`ChannelSpec`] holds the parsed form; [`ChannelSpec::into_channel`]
//! converts to a [`Channel`] at the CLI argv boundary, where the only
//! `f64` fields (`shift_ms`, `offset_ms`) cross via the `F64F06` Conn
//! per CLAUDE.md float exception 4.

use std::fmt::{self, Display};

use crate::channel::transform::MAX_SHIFT;
use crate::channel::{Channel, ChannelMode};
use crate::fxp::{Extended, ExtendedFloat, F64F06, Micro};
use crate::time::grid::Grid;
use crate::time::swing::SwingConfig;
use crate::time::tbase::TBase;

/// Parsed `--ch` spec. Lossless: a `Channel` round-trips through
/// `ChannelSpec::into_channel().into_spec()` only if every field
/// was set explicitly (defaults aren't recoverable from a `Channel`).
#[derive(Debug, Clone, PartialEq)]
pub struct ChannelSpec {
    /// Optional human-readable identifier. Free-form string.
    pub id: Option<String>,
    /// Routing destination kind. `Audio` is reserved for v0.4.
    pub dev: ChannelDev,
    /// Device-specific routing target (port name, channel index).
    pub out: Option<String>,
    /// Tempo divider. Required. Any 36-element `Grid` value is
    /// allowed (`t4`, `t16`, `t8q`, `t32t`, `t2p`, …).
    pub div: Grid,
    /// `SwingConfig::amount` — signed `i8` tick offset on the
    /// resolution grid. Default 0 (no swing).
    pub swing: i8,
    /// `SwingConfig::resolution` — binary subdivision the swing
    /// grid lives on. Default `TBase::T16`.
    pub swing_res: TBase,
    /// Positive shift in milliseconds. Clamped to MAX_SHIFT (300 ms)
    /// in `into_channel`.
    pub shift_ms: f64, // argv boundary
    /// Signed offset in milliseconds.
    pub offset_ms: f64, // argv boundary
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
        let mut div: Option<Grid> = None;
        let mut swing: i8 = 0;
        let mut swing_res: TBase = TBase::T16;
        let mut shift_ms: f64 = 0.0; // argv boundary
        let mut offset_ms: f64 = 0.0; // argv boundary
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
                "div" => {
                    div = Some(
                        v.parse::<Grid>()
                            .map_err(|e| ChannelSpecError::BadValue("div", e.to_string()))?,
                    );
                }
                "swing" => {
                    swing = v
                        .parse::<i8>()
                        .map_err(|e| ChannelSpecError::BadValue("swing", e.to_string()))?;
                }
                "swing-res" => {
                    swing_res = v
                        .parse::<TBase>()
                        .map_err(|e| ChannelSpecError::BadValue("swing-res", e.to_string()))?;
                }
                "shift-ms" => {
                    let parsed = v
                        .parse::<f64>()
                        .map_err(|e| ChannelSpecError::BadValue("shift-ms", e.to_string()))?;
                    if !parsed.is_finite() {
                        return Err(ChannelSpecError::BadValue(
                            "shift-ms",
                            "must be finite".into(),
                        ));
                    }
                    shift_ms = parsed;
                }
                "offset-ms" => {
                    let parsed = v
                        .parse::<f64>()
                        .map_err(|e| ChannelSpecError::BadValue("offset-ms", e.to_string()))?;
                    if !parsed.is_finite() {
                        return Err(ChannelSpecError::BadValue(
                            "offset-ms",
                            "must be finite".into(),
                        ));
                    }
                    offset_ms = parsed;
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
            div: div.ok_or(ChannelSpecError::MissingKey("div"))?,
            swing,
            swing_res,
            shift_ms,
            offset_ms,
            snap_to_quantum_micro,
        })
    }

    /// Convert the parsed spec into the runtime [`Channel`] type. The
    /// only `f64`s in this crate die here at the `// argv boundary`
    /// via the `F64F06` Conn (per CLAUDE.md float exception 4).
    pub fn into_channel(self) -> Result<Channel, ChannelSpecError> {
        let mode = match self.dev {
            ChannelDev::Midi => ChannelMode::MidiClock,
            ChannelDev::Audio => return Err(ChannelSpecError::AudioDeferred),
        };
        // argv boundary: shift-ms / offset-ms cross into Micro via F64F06.
        // Shift-ms multiplied by 1e3 since F64F06 is seconds → micros.
        let shift = micro_from_ms(self.shift_ms);
        let offset = micro_from_ms(self.offset_ms);
        let shift = Micro(shift.0.clamp(0, MAX_SHIFT.0));

        Ok(Channel {
            mode,
            divider: self.div,
            shuffle: SwingConfig {
                resolution: self.swing_res,
                amount: self.swing,
            },
            shift,
            offset,
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
    /// Serialise back into a parseable spec. `parse(spec.to_string())`
    /// recovers the same spec for any valid `ChannelSpec`. Values
    /// containing `,`, `=`, or whitespace are quoted so the
    /// docker-style port name `out=IAC Bus 1` round-trips through
    /// `Display` → `parse` correctly.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "dev={},div={}", self.dev, self.div)?;
        if let Some(id) = &self.id {
            write!(f, ",id={}", quote_if_needed(id))?;
        }
        if let Some(out) = &self.out {
            write!(f, ",out={}", quote_if_needed(out))?;
        }
        if self.swing != 0 {
            write!(f, ",swing={}", self.swing)?;
        }
        if self.swing_res != TBase::T16 {
            write!(f, ",swing-res={}", self.swing_res)?;
        }
        if self.shift_ms != 0.0 {
            write!(f, ",shift-ms={}", self.shift_ms)?;
        }
        if self.offset_ms != 0.0 {
            write!(f, ",offset-ms={}", self.offset_ms)?;
        }
        if let Some(q) = self.snap_to_quantum_micro {
            write!(f, ",snap-quantum-us={}", q)?;
        }
        Ok(())
    }
}

/// Wrap `v` in `"..."` if it contains any of the tokenizer's
/// separators (`,`, `=`, whitespace) so the round-trip survives.
/// Values without those characters are emitted verbatim. The parser
/// has no escape sequence inside quoted values, so a value
/// containing a literal `"` cannot round-trip — `Display` returns it
/// verbatim and `parse` will still succeed if no separators are
/// present, but a value like `name with " quote` will silently
/// produce a `Malformed` error from `parse` later. v0.1 doesn't
/// expose any code paths that produce such values; v0.2's preset
/// I/O will need a real escaping convention.
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
        let s = "div=t32t,dev=midi,out=IAC Bus 1,swing=10,shift-ms=2.5,offset-ms=-1.0";
        let spec = ChannelSpec::parse(s).expect("parse");
        assert_eq!(spec.div, Grid::T32T);
        assert_eq!(spec.dev, ChannelDev::Midi);
        assert_eq!(spec.out.as_deref(), Some("IAC Bus 1"));
        assert_eq!(spec.swing, 10);
        assert_eq!(spec.swing_res, TBase::T16);
        assert!((spec.shift_ms - 2.5).abs() < 1e-9);
        assert!((spec.offset_ms - -1.0).abs() < 1e-9);
        assert_eq!(spec.snap_to_quantum_micro, None);
    }

    #[test]
    fn parse_swing_res_default_t16() {
        let spec = ChannelSpec::parse("dev=midi,div=t16,swing=80").unwrap();
        assert_eq!(spec.swing_res, TBase::T16);
        assert_eq!(spec.swing, 80);
    }

    #[test]
    fn parse_swing_res_explicit() {
        let spec =
            ChannelSpec::parse("dev=midi,div=t8,swing=40,swing-res=t8").unwrap();
        assert_eq!(spec.swing_res, TBase::T8);
        assert_eq!(spec.swing, 40);
    }

    #[test]
    fn parse_quintuplet_divider() {
        let spec = ChannelSpec::parse("dev=midi,div=t8q").unwrap();
        assert_eq!(spec.div, Grid::T8Q);
    }

    #[test]
    fn parse_quoted_value_with_spaces_and_commas() {
        let s = r#"dev=midi,div=t32t,out="IAC Bus 1, port 2""#;
        let spec = ChannelSpec::parse(s).expect("parse");
        assert_eq!(spec.out.as_deref(), Some("IAC Bus 1, port 2"));
    }

    #[test]
    fn parse_rejects_unknown_key() {
        let err = ChannelSpec::parse("dev=midi,div=t32t,unknown=x").unwrap_err();
        assert_eq!(err, ChannelSpecError::UnknownKey("unknown".into()));
    }

    #[test]
    fn parse_rejects_missing_required() {
        let err = ChannelSpec::parse("dev=midi").unwrap_err();
        assert_eq!(err, ChannelSpecError::MissingKey("div"));
        let err = ChannelSpec::parse("div=t32t").unwrap_err();
        assert_eq!(err, ChannelSpecError::MissingKey("dev"));
    }

    #[test]
    fn parse_rejects_dev_audio_with_audio_deferred() {
        let err = ChannelSpec::parse("dev=audio,div=t32t").unwrap_err();
        assert_eq!(err, ChannelSpecError::AudioDeferred);
    }

    #[test]
    fn parse_rejects_bad_div() {
        let err = ChannelSpec::parse("dev=midi,div=t3").unwrap_err();
        match err {
            ChannelSpecError::BadValue(key, _) => assert_eq!(key, "div"),
            other => panic!("unexpected: {:?}", other),
        }
    }

    #[test]
    fn parse_rejects_non_binary_swing_res() {
        // `swing-res` is `TBase` (binary chain only) — quintuplet /
        // triplet names must fail at parse time.
        let err = ChannelSpec::parse("dev=midi,div=t16,swing-res=t8q").unwrap_err();
        match err {
            ChannelSpecError::BadValue(key, _) => assert_eq!(key, "swing-res"),
            other => panic!("unexpected: {:?}", other),
        }
    }

    #[test]
    fn into_channel_clamps_shift_ms() {
        // 500 ms exceeds MAX_SHIFT (300 ms); clamped to 300 ms.
        let spec = ChannelSpec::parse("dev=midi,div=t32t,shift-ms=500").unwrap();
        let ch = spec.into_channel().unwrap();
        assert_eq!(ch.shift, MAX_SHIFT);
    }

    #[test]
    fn into_channel_negative_shift_clamps_to_zero() {
        let spec = ChannelSpec::parse("dev=midi,div=t32t,shift-ms=-50").unwrap();
        let ch = spec.into_channel().unwrap();
        assert_eq!(ch.shift, Micro(0));
    }

    #[test]
    fn display_round_trip_minimal() {
        let spec = ChannelSpec::parse("dev=midi,div=t32t").unwrap();
        let s = spec.to_string();
        let reparsed = ChannelSpec::parse(&s).unwrap();
        assert_eq!(spec, reparsed);
    }

    #[test]
    fn display_quotes_values_with_spaces() {
        let spec = ChannelSpec::parse(r#"dev=midi,div=t32t,out="IAC Bus 1""#).unwrap();
        let s = spec.to_string();
        assert!(s.contains(r#"out="IAC Bus 1""#), "got: {s}");
        let reparsed = ChannelSpec::parse(&s).unwrap();
        assert_eq!(spec, reparsed);
    }

    #[test]
    fn display_quotes_values_with_commas() {
        let spec =
            ChannelSpec::parse(r#"dev=midi,div=t32t,out="port,with,commas""#).unwrap();
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

    fn arb_tbase() -> impl Strategy<Value = TBase> {
        prop::sample::select(TBase::ALL.as_slice())
    }

    fn arb_spec() -> impl Strategy<Value = ChannelSpec> {
        let ident = r#"[a-zA-Z0-9 ,=_]{1,15}"#;
        (
            arb_dev(),
            arb_grid(),
            prop::option::of(ident),
            prop::option::of(ident),
            any::<i8>(),
            arb_tbase(),
            (0u32..=300).prop_map(|n| n as f64),
            (-100i32..=100).prop_map(|n| n as f64),
            prop::option::of(any::<i32>().prop_map(|n| n as i64)),
        )
            .prop_map(
                |(
                    dev,
                    div,
                    id,
                    out,
                    swing,
                    swing_res,
                    shift_ms,
                    offset_ms,
                    snap_to_quantum_micro,
                )| ChannelSpec {
                    id,
                    dev,
                    out,
                    div,
                    swing,
                    swing_res,
                    shift_ms,
                    offset_ms,
                    snap_to_quantum_micro,
                },
            )
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
