//! layer: parse
//! depends-on:
//!
//! Shared bpaf parser adapters.

#[cfg(feature = "core")]
pub(crate) fn parse_positive_u32(v: u32) -> Result<u32, String> {
    if v == 0 {
        Err("must be >= 1, got 0".to_string())
    } else {
        Ok(v)
    }
}

/// bpaf parser: `--delay <ms-as-f64>` -> Micro at the argv-handler
/// boundary. Open-codes the ms->s shift inside the parser body:
/// `F064FD06` interprets f64 as canonical seconds, and there is no
/// `Conn<f64-as-ms, FD06>` rung.
#[cfg(feature = "core")]
pub(crate) fn parse_ms_to_micro(s: String) -> Result<agogo::chan::conn::fixed::Micro, String> {
    use agogo::chan::conn::float::Extended;
    use agogo::chan::conn::float::ExtendedFloat;
    use agogo::chan::conn::float::F064FD06;
    let ms: f64 = s
        .parse()
        .map_err(|e| format!("--delay {s}: not a number ({e})"))?;
    if !ms.is_finite() || ms < 0.0 {
        return Err(format!(
            "--delay {ms} invalid (expected non-negative finite ms)"
        ));
    }
    match F064FD06.ceil(ExtendedFloat::Extend(ms * 1.0e-3)) {
        Extended::Finite(m) => Ok(m),
        Extended::PosInf | Extended::NegInf => Err(format!("--delay {ms} out of range")),
    }
}

/// bpaf parser: `--jitter-us <us-as-f64>` -> Pico at the argv-handler
/// boundary. Same argv-boundary rationale as `parse_ms_to_micro`:
/// `F064FD12` interprets f64 as seconds, so the us->s shift is
/// open-coded inside the parser body.
#[cfg(feature = "core")]
pub(crate) fn parse_jitter_us_to_pico(s: String) -> Result<agogo::chan::conn::fixed::Pico, String> {
    use agogo::chan::conn::float::Extended;
    use agogo::chan::conn::float::ExtendedFloat;
    use agogo::chan::conn::float::F064FD12;
    let us: f64 = s
        .parse()
        .map_err(|e| format!("--jitter-us {s}: not a number ({e})"))?;
    if !us.is_finite() || us < 0.0 {
        return Err(format!(
            "--jitter-us {us} invalid (expected non-negative finite us)"
        ));
    }
    match F064FD12.ceil(ExtendedFloat::Extend(us * 1.0e-6)) {
        Extended::Finite(p) => Ok(p),
        Extended::PosInf | Extended::NegInf => Err(format!("--jitter-us {us} out of range")),
    }
}

/// bpaf parser: BPM `<f64>` -> `Tempo` at the argv-handler boundary.
/// Used by every `--bpm` / `--initial-bpm` flag across the CLI.
#[cfg(feature = "core")]
pub(crate) fn parse_bpm_to_tempo(s: String) -> Result<agogo::chan::conn::tempo::Tempo, String> {
    use agogo::chan::conn::float::f64_bpm_to_tempo;
    let f: f64 = s
        .parse()
        .map_err(|e| format!("BPM value {s}: not a number ({e})"))?;
    if !f.is_finite() || f <= 0.0 || f > agogo::chan::conn::float::MAX_BPM_F64 {
        return Err(format!(
            "BPM value {f} out of range (expected (0, {}] BPM)",
            agogo::chan::conn::float::MAX_BPM_F64
        ));
    }
    Ok(f64_bpm_to_tempo(f))
}

/// Validate `--output-channels N` against the offline path's
/// `MAX_OUTPUT_CHANNELS` cap. Shared by `agogo render` and `agogo
/// run` so the offline and live paths can't drift on the channel
/// count rule.
#[cfg(feature = "core")]
pub(crate) fn validate_output_channels(value: u32) -> Result<u16, String> {
    use agogo::core::MAX_OUTPUT_CHANNELS;
    u16::try_from(value)
        .ok()
        .filter(|&n| (1..=MAX_OUTPUT_CHANNELS).contains(&n))
        .ok_or_else(|| {
            format!("--output-channels {value} not supported (must be 1..={MAX_OUTPUT_CHANNELS})")
        })
}

#[cfg(feature = "link")]
pub(crate) use agogo::host::link::parse_quantum_from_beats;
