//! Parser for the `--ch <spec>` key=val[,key=val]* mini-language.
//!
//! Holds `ChannelSpec::parse` (the per-spec parser), `parse_channels`
//! (the public top-level entry that resolves a sequence of specs in
//! order while building the variable environment), plus the
//! private helpers `parse_swing`, `micro_from_user_ms`, and the
//! `tokenize` lexer.
//!
//! Plan 2026-04-28-06 T3: extracted from `machine/spec.rs`.

use core::num::{NonZeroU16, NonZeroU32};

use crate::channel::dsl;
use crate::channel::role::{AudioRole, CvRole, MidiClickAccent, MidiClickConfig, MidiRole};
use crate::conn::fixed::Micro;
use crate::conn::float::F064FD06;
use crate::conn::midi::{U4, U7};
use crate::time::grid::Grid;
use crate::time::swing::SwingConfig;
use crate::time::tbase::TBase;
use connections::extended::Extended;
use connections::float::ExtendedFloat;

use super::error::ChannelSpecError;
use super::{ChannelSpec, ChannelSpecRole};

impl ChannelSpec {
    /// Parse a `key=val[,key=val]*` spec into a [`ChannelSpec`].
    ///
    /// The `grid=` value is resolved via `dsl::parse` using `env` for
    /// variable references. Pass `&[]` for the first channel.
    pub fn parse(s: &str, env: &[(String, Grid)]) -> Result<Self, ChannelSpecError> {
        let pairs = tokenize(s)?;

        let mut id: Option<String> = None;
        // The `dev=` key is required. MIDI remains the default
        // production target; audio and CV are generated-output
        // surfaces with fixed MVP renderer settings.
        let mut dev_kw: Option<&'static str> = None; // "midi" | "audio" | "cv"
        let mut out: Option<String> = None;
        let mut grid_str: Option<String> = None;
        let mut swing: SwingConfig = SwingConfig {
            resolution: TBase::T8,
            amount: 0,
        };
        let mut offset_ticks: i32 = 0;
        let mut delay: Micro = Micro::ZERO;
        let mut snap_to_quantum_micro: Option<i64> = None;

        // Plan 2026-04-25-03 keys. Defer mode/click validation until
        // after the loop: cross-key constraints (mode=click requires
        // note+vel; clock rejects click keys) are easier to enforce
        // in one pass at the end.
        let mut mode_kw: Option<&'static str> = None; // "clock" | "click" | "pulse" | "lfo"
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
                    dev_kw = Some(match v.as_str() {
                        "midi" => "midi",
                        "audio" => "audio",
                        "cv" => "cv",
                        other => {
                            return Err(ChannelSpecError::BadValue("dev", other.to_string()));
                        }
                    });
                }
                "out" => out = Some(v),
                "grid" => {
                    grid_str = Some(v);
                }
                "mode" => {
                    mode_kw = Some(match v.as_str() {
                        "clock" => "clock",
                        "click" => "click",
                        "pulse" => "pulse",
                        "lfo" => "lfo",
                        other => {
                            return Err(ChannelSpecError::BadValue(
                                "mode",
                                format!(
                                    "expected `clock`, `click`, `pulse`, or `lfo`, got `{other}`"
                                ),
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
                    bars =
                        Some(NonZeroU16::new(n).ok_or_else(|| {
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
                    // argv boundary: ms (string) → Micro at this
                    // line. Three paths in falling priority:
                    //   1. Bare i64 ms ("51")            — exact.
                    //   2. Decimal `int.frac` ("51.123") — exact via
                    //      string split + integer parse, sidesteps
                    //      f64's inability to represent decimal
                    //      fractions exactly. This is the form
                    //      `Display for ChannelSpec` emits, so the
                    //      Display→parse round-trip lives here.
                    //   3. f64 fallback                 — covers
                    //      scientific notation ("1e-3") and other
                    //      non-canonical user input. Negative delay
                    //      is rejected here rather than clamped at
                    //      into_channel — the user almost certainly
                    //      typed it by mistake.
                    let candidate = if let Ok(ms_int) = v.parse::<i64>() {
                        let us = ms_int.checked_mul(1_000).ok_or_else(|| {
                            ChannelSpecError::BadValue(
                                "delay",
                                format!("{ms_int} ms overflows Micro"),
                            )
                        })?;
                        Micro(us)
                    } else if let Some(micro) = parse_decimal_ms(&v) {
                        micro
                    } else {
                        let ms_f64 = v
                            .parse::<f64>()
                            .map_err(|e| ChannelSpecError::BadValue("delay", e.to_string()))?;
                        micro_from_user_ms(ms_f64).ok_or_else(|| {
                            ChannelSpecError::BadValue(
                                "delay",
                                format!("{ms_f64} out of range or non-finite"),
                            )
                        })?
                    };
                    if candidate.0 < 0 {
                        return Err(ChannelSpecError::BadValue(
                            "delay",
                            format!("{} ms negative (delay must be ≥ 0)", v),
                        ));
                    }
                    delay = candidate;
                }
                "snap-quantum-us" => {
                    snap_to_quantum_micro = Some(v.parse::<i64>().map_err(|e| {
                        ChannelSpecError::BadValue("snap-quantum-us", e.to_string())
                    })?);
                }
                other => return Err(ChannelSpecError::UnknownKey(other.to_string())),
            }
        }

        // Resolve grid expression via DSL parser.
        let grid_expr = grid_str.as_deref().unwrap_or("T4");
        let grid = dsl::parse(grid_expr, env)
            .map_err(|e| ChannelSpecError::BadValue("grid", e.to_string()))?;

        let dev = dev_kw.ok_or(ChannelSpecError::MissingKey("dev"))?;

        let default_mode = match dev {
            "midi" => "clock",
            "audio" => "clock",
            "cv" => "pulse",
            _ => unreachable!("dev_kw is constrained at parse"),
        };

        // Cross-key validation: assemble target-specific role from
        // `dev=` + `mode=`. `dev=midi` keeps the historical
        // default `mode=clock`; `dev=audio` must spell
        // `mode=click` so an accidental audio clock spec does not
        // silently turn into a no-op; `dev=cv` defaults to pulse.
        let role = match (dev, mode_kw.unwrap_or(default_mode)) {
            ("midi", "clock") => {
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
                ChannelSpecRole::Midi(MidiRole::Clock)
            }
            ("midi", "click") => {
                let note = note.ok_or(ChannelSpecError::MissingKey("note"))?;
                let vel = vel.ok_or(ChannelSpecError::MissingKey("vel"))?;
                // GM drum kit default = mch 10 (1-based) = U4(9).
                let ch = mch_zero_based.unwrap_or(U4(9));
                let accent = match accent_every {
                    Some(e) => {
                        let av = accent_vel.ok_or(ChannelSpecError::MissingKey("accent-vel"))?;
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
                ChannelSpecRole::Midi(MidiRole::Click(MidiClickConfig {
                    note,
                    vel,
                    ch,
                    accent,
                }))
            }
            ("midi", "pulse" | "lfo") => {
                return Err(ChannelSpecError::BadValue(
                    "mode",
                    "dev=midi supports mode=clock or mode=click only".into(),
                ));
            }
            ("audio", "click") => {
                const MIDI_ONLY_KEYS: &[&str] = &[
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
                for (i, &key) in MIDI_ONLY_KEYS.iter().enumerate() {
                    if presence[i] {
                        return Err(ChannelSpecError::BadValue(
                            key,
                            "only valid with dev=midi,mode=click".into(),
                        ));
                    }
                }
                ChannelSpecRole::Audio(AudioRole::Click)
            }
            ("audio", "clock") => {
                return Err(ChannelSpecError::BadValue(
                    "mode",
                    "dev=audio supports mode=click only".into(),
                ));
            }
            ("audio", "pulse" | "lfo") => {
                return Err(ChannelSpecError::BadValue(
                    "mode",
                    "dev=audio supports mode=click only".into(),
                ));
            }
            ("cv", "pulse") => {
                const NON_CV_KEYS: &[&str] = &[
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
                for (i, &key) in NON_CV_KEYS.iter().enumerate() {
                    if presence[i] {
                        return Err(ChannelSpecError::BadValue(
                            key,
                            "only valid with dev=midi,mode=click".into(),
                        ));
                    }
                }
                ChannelSpecRole::Cv(CvRole::Pulse)
            }
            ("cv", "lfo") => {
                return Err(ChannelSpecError::BadValue(
                    "mode",
                    "dev=cv,mode=lfo is not implemented yet".into(),
                ));
            }
            ("cv", "clock" | "click") => {
                return Err(ChannelSpecError::BadValue(
                    "mode",
                    "dev=cv supports mode=pulse only".into(),
                ));
            }
            _ => unreachable!("dev_kw and mode_kw are constrained at parse"),
        };

        // Derive `audio_lane` from `out=` when `dev=audio`. The
        // raw `out` field stays for back-compat / non-audio uses;
        // the typed `audio_lane` is what the audio renderer reads.
        // Audio without a destination is silence — `out=diag` is
        // rejected explicitly so the user gets a clear message
        // instead of "expected u16, got `diag`".
        let audio_lane = if matches!(role, ChannelSpecRole::Audio(_)) {
            let raw = out.as_deref().ok_or_else(|| {
                ChannelSpecError::BadValue(
                    "out",
                    "dev=audio requires out=N (audio output channel index, 0..)".into(),
                )
            })?;
            if matches!(raw, "diag" | "diagnostic") {
                return Err(ChannelSpecError::BadValue(
                    "out",
                    "audio without a destination is silence — use out=N (audio output channel index)".into(),
                ));
            }
            let lane = raw.parse::<u16>().map_err(|_| {
                ChannelSpecError::BadValue(
                    "out",
                    format!("dev=audio expects out=N (non-negative integer), got `{raw}`"),
                )
            })?;
            Some(lane)
        } else {
            None
        };

        Ok(Self {
            id,
            out,
            audio_lane,
            grid,
            role,
            swing,
            offset_ticks,
            delay,
            snap_to_quantum_micro,
            bars,
        })
    }
}

/// Parse a sequence of `--ch` specs in order, building the variable
/// environment as each channel's grid is resolved. Unnamed channels
/// are auto-assigned IDs (`C1`, `C2`, ...).
///
/// Returns `(id, ChannelSpec)` pairs in definition order.
pub fn parse_channels(specs: &[String]) -> Result<Vec<(String, ChannelSpec)>, ChannelSpecError> {
    let mut env: Vec<(String, Grid)> = Vec::new();
    let mut result = Vec::new();
    for (i, s) in specs.iter().enumerate() {
        let spec = ChannelSpec::parse(s, &env)?;
        let id = spec.id.clone().unwrap_or_else(|| format!("C{}", i + 1));
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
        Ok(SwingConfig { resolution, amount })
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
fn micro_from_user_ms(ms: f64) -> Option<Micro> {
    // argv boundary: ms → canonical seconds for F064FD06's input.
    let seconds = ms * 1.0e-3;
    match F064FD06.ceil(ExtendedFloat::Extend(seconds)) {
        Extended::Finite(m) => Some(m),
        Extended::PosInf | Extended::NegInf => None,
    }
}

/// Parse a `[-]int.frac` decimal-millisecond string to `Micro` exactly.
/// Returns `None` if `s` doesn't match that grammar — caller falls back
/// to the f64 path for scientific notation and other non-canonical
/// forms.
///
/// This is the inverse of `Display for ChannelSpec`'s
/// `delay={ms_int}.{frac:03}` form. f64 cannot represent decimal
/// fractions like `0.116` exactly — `0.116 × 10⁻³` round-trips
/// through `F064FD06.ceil` to `Micro(117)`, one µs above the source
/// value. Splitting on `.` and parsing both halves as integers
/// sidesteps the float entirely.
fn parse_decimal_ms(s: &str) -> Option<Micro> {
    let (int_str, frac_str) = s.split_once('.')?;
    if int_str.is_empty() || frac_str.is_empty() {
        return None;
    }
    // Both halves must be all-digits (with the integer part optionally
    // signed). Non-canonical forms are left to other parsing paths.
    let signed_int = int_str.parse::<i64>().ok()?;
    if !frac_str.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    // Convert frac_str to whole µs: `Display` always emits exactly 3
    // digits, but accept any digit count so user input like
    // `delay=1.5` and `delay=0.123456` parses sensibly. Right-pad to
    // 3 digits for shorter inputs; truncate (toward zero, the
    // user-friendly direction) for longer inputs. The empty case is
    // unreachable — `frac_str.is_empty()` returns `None` above.
    let frac_us: i64 = match frac_str.len() {
        1 => frac_str.parse::<i64>().ok()? * 100,
        2 => frac_str.parse::<i64>().ok()? * 10,
        3 => frac_str.parse::<i64>().ok()?,
        _ => frac_str[..3].parse::<i64>().ok()?,
    };
    let abs_us = signed_int
        .checked_abs()?
        .checked_mul(1_000)?
        .checked_add(frac_us)?;
    let signed_us = if int_str.starts_with('-') {
        abs_us.checked_neg()?
    } else {
        abs_us
    };
    Some(Micro(signed_us))
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

    // ── Basic parsing ────────────────────────────────────────────

    #[test]
    fn parse_minimal() {
        let spec = ChannelSpec::parse("dev=midi", &[]).unwrap();
        assert_eq!(spec.grid, Grid::T4); // default
        // dev field is gone (audit P4); the parser still requires
        // the `dev=` key but stores nothing.
        assert_eq!(
            spec.swing,
            SwingConfig {
                resolution: TBase::T8,
                amount: 0
            }
        );
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
        assert_eq!(
            spec.swing,
            SwingConfig {
                resolution: TBase::T8,
                amount: 80
            }
        );
    }

    #[test]
    fn parse_swing_with_resolution() {
        let spec = ChannelSpec::parse("dev=midi,swing=T16:80", &[]).unwrap();
        assert_eq!(
            spec.swing,
            SwingConfig {
                resolution: TBase::T16,
                amount: 80
            }
        );
    }

    #[test]
    fn parse_swing_negative() {
        let spec = ChannelSpec::parse("dev=midi,swing=-40", &[]).unwrap();
        assert_eq!(
            spec.swing,
            SwingConfig {
                resolution: TBase::T8,
                amount: -40
            }
        );
    }

    #[test]
    fn parse_swing_explicit_negative() {
        let spec = ChannelSpec::parse("dev=midi,swing=T16:-40", &[]).unwrap();
        assert_eq!(
            spec.swing,
            SwingConfig {
                resolution: TBase::T16,
                amount: -40
            }
        );
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
    fn parse_accepts_audio_click() {
        let spec = ChannelSpec::parse("dev=audio,mode=click,grid=t4,out=0", &[]).unwrap();
        assert_eq!(spec.role, ChannelSpecRole::Audio(AudioRole::Click));
        assert_eq!(spec.audio_lane, Some(0));
    }

    #[test]
    fn parse_audio_click_requires_out() {
        let err = ChannelSpec::parse("dev=audio,mode=click,grid=t4", &[]).unwrap_err();
        match err {
            ChannelSpecError::BadValue(key, msg) => {
                assert_eq!(key, "out");
                assert!(msg.contains("requires out=N"), "got: {msg}");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parse_audio_click_rejects_out_diag() {
        let err = ChannelSpec::parse("dev=audio,mode=click,grid=t4,out=diag", &[]).unwrap_err();
        match err {
            ChannelSpecError::BadValue(key, msg) => {
                assert_eq!(key, "out");
                assert!(
                    msg.contains("audio without a destination is silence"),
                    "got: {msg}"
                );
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parse_audio_click_rejects_non_integer_out() {
        let err =
            ChannelSpec::parse("dev=audio,mode=click,grid=t4,out=speakers-a", &[]).unwrap_err();
        match err {
            ChannelSpecError::BadValue(key, msg) => {
                assert_eq!(key, "out");
                assert!(msg.contains("non-negative integer"), "got: {msg}");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parse_audio_click_accepts_lane_15() {
        let spec = ChannelSpec::parse("dev=audio,mode=click,grid=t4,out=15", &[]).unwrap();
        assert_eq!(spec.audio_lane, Some(15));
    }

    #[test]
    fn parse_accepts_cv_pulse() {
        let spec = ChannelSpec::parse("dev=cv,mode=pulse,grid=t4", &[]).unwrap();
        assert_eq!(spec.role, ChannelSpecRole::Cv(CvRole::Pulse));
    }

    #[test]
    fn parse_cv_defaults_to_pulse() {
        let spec = ChannelSpec::parse("dev=cv,grid=t4", &[]).unwrap();
        assert_eq!(spec.role, ChannelSpecRole::Cv(CvRole::Pulse));
    }

    #[test]
    fn parse_rejects_cv_lfo_until_renderer_exists() {
        let err = ChannelSpec::parse("dev=cv,mode=lfo,grid=t4", &[]).unwrap_err();
        match err {
            ChannelSpecError::BadValue(key, msg) => {
                assert_eq!(key, "mode");
                assert!(msg.contains("not implemented"), "got: {msg}");
            }
            other => panic!("unexpected: {:?}", other),
        }
    }

    #[test]
    fn parse_rejects_cv_pulse_with_midi_keys() {
        let err = ChannelSpec::parse("dev=cv,mode=pulse,grid=t4,note=37", &[]).unwrap_err();
        match err {
            ChannelSpecError::BadValue(key, msg) => {
                assert_eq!(key, "note");
                assert!(msg.contains("dev=midi"), "got: {msg}");
            }
            other => panic!("unexpected: {:?}", other),
        }
    }

    #[test]
    fn parse_rejects_midi_pulse_and_lfo_modes() {
        for mode in ["pulse", "lfo"] {
            let err =
                ChannelSpec::parse(&format!("dev=midi,mode={mode},grid=t4"), &[]).unwrap_err();
            match err {
                ChannelSpecError::BadValue(key, msg) => {
                    assert_eq!(key, "mode");
                    assert!(msg.contains("mode=clock or mode=click"), "got: {msg}");
                }
                other => panic!("unexpected: {:?}", other),
            }
        }
    }

    #[test]
    fn parse_rejects_audio_without_click_mode() {
        let err = ChannelSpec::parse("dev=audio,grid=t4", &[]).unwrap_err();
        assert!(matches!(err, ChannelSpecError::BadValue("mode", _)));
    }

    #[test]
    fn parse_rejects_audio_click_with_midi_keys() {
        let err =
            ChannelSpec::parse("dev=audio,mode=click,grid=t4,note=37,out=0", &[]).unwrap_err();
        match err {
            ChannelSpecError::BadValue(key, msg) => {
                assert_eq!(key, "note");
                assert!(msg.contains("dev=midi"), "got: {msg}");
            }
            other => panic!("unexpected: {:?}", other),
        }
    }

    #[test]
    fn parse_rejects_bad_grid() {
        let err = ChannelSpec::parse("dev=midi,grid=T3", &[]).unwrap_err();
        assert!(matches!(err, ChannelSpecError::BadValue("grid", _)));
    }

    // ── Delay ────────────────────────────────────────────────────

    #[test]
    fn parse_rejects_negative_delay() {
        // Q3 round-1 fix: negative delay is rejected at parse
        // time rather than silently clamped to 0 in into_channel
        // — the user almost certainly typed it by mistake, and
        // the spec's `delay` field is documented as non-negative.
        let err = ChannelSpec::parse("dev=midi,delay=-50", &[])
            .expect_err("negative delay should be rejected");
        let msg = format!("{err}");
        assert!(
            msg.contains("negative") && msg.contains("delay"),
            "expected negative-delay error, got: {msg}",
        );
    }

    #[test]
    fn parse_decimal_ms_exact() {
        // Display→parse round-trip on canonical 3-digit fractional
        // form is bit-exact (no f64 drift).
        assert_eq!(parse_decimal_ms("0.116"), Some(Micro(116)));
        assert_eq!(parse_decimal_ms("51.123"), Some(Micro(51_123)));
        assert_eq!(parse_decimal_ms("300.000"), Some(Micro(300_000)));
        // Shorter fractional inputs right-pad to 3 digits.
        assert_eq!(parse_decimal_ms("1.5"), Some(Micro(1_500)));
        assert_eq!(parse_decimal_ms("1.50"), Some(Micro(1_500)));
        // Longer inputs truncate at µs precision (toward zero).
        assert_eq!(parse_decimal_ms("0.123456"), Some(Micro(123)));
        // Non-canonical forms decline so the f64 fallback handles them.
        assert_eq!(parse_decimal_ms("1e-3"), None);
        assert_eq!(parse_decimal_ms("1."), None);
        assert_eq!(parse_decimal_ms(".5"), None);
        assert_eq!(parse_decimal_ms("abc"), None);
    }

    #[test]
    fn parse_delay_decimal_no_f64_drift() {
        // `Display for ChannelSpec` emits `delay=0.116` for
        // `Micro(116)`. The f64 path would ceil to `Micro(117)` — the
        // string-decimal fast path keeps it exact.
        let spec = ChannelSpec::parse("dev=midi,delay=0.116", &[]).unwrap();
        assert_eq!(spec.delay, Micro(116));
    }

    // Display + round-trip tests moved to super::super::display::tests
    // (Plan 2026-04-28-06 T4).

    // ── Plan 2026-04-25-03: mode=click + bars spot checks ──────

    #[test]
    fn parse_default_mode_is_clock() {
        let spec = ChannelSpec::parse("dev=midi,grid=t4", &[]).unwrap();
        assert_eq!(spec.role, ChannelSpecRole::Midi(MidiRole::Clock));
    }

    #[test]
    fn parse_full_click_spec() {
        let s = "dev=midi,mode=click,grid=t4,note=37,vel=80,mch=10,\
                 accent-every=4,accent-note=38,accent-vel=120";
        let spec = ChannelSpec::parse(s, &[]).expect("parse");
        let cfg = match spec.role {
            ChannelSpecRole::Midi(MidiRole::Click(c)) => c,
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
        let spec = ChannelSpec::parse("dev=midi,mode=click,grid=t4,note=76,vel=100", &[]).unwrap();
        let cfg = match spec.role {
            ChannelSpecRole::Midi(MidiRole::Click(c)) => c,
            _ => panic!(),
        };
        assert_eq!(cfg.ch, U4(9));
    }

    #[test]
    fn parse_click_accent_note_defaults_to_note() {
        let s = "dev=midi,mode=click,grid=t4,note=37,vel=70,accent-every=4,accent-vel=120";
        let spec = ChannelSpec::parse(s, &[]).unwrap();
        let cfg = match spec.role {
            ChannelSpecRole::Midi(MidiRole::Click(c)) => c,
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
        let err = ChannelSpec::parse("dev=midi,mode=click,grid=t4,note=37,vel=0", &[]).unwrap_err();
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
        let err = ChannelSpec::parse("dev=midi,mode=click,grid=t4,note=37,vel=80,mch=0", &[])
            .unwrap_err();
        match err {
            ChannelSpecError::BadValue(key, _) => assert_eq!(key, "mch"),
            _ => panic!("expected BadValue"),
        }
        let err = ChannelSpec::parse("dev=midi,mode=click,grid=t4,note=37,vel=80,mch=17", &[])
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

    // `into_channel_*` and `snap_intent_*` tests moved to
    // `super::validate::tests` (Plan 2026-04-28-06 T5).
}
