//! `agogo render` — deterministic hardware-free render report.

use agogo::chan::channel::Channel;
use agogo::chan::channel::time::validate_schedule_params;
use agogo::chan::conn::tempo::Tempo;
use agogo::chan::time::conn::tick_to_whole_samples;
use agogo::chan::time::grid::Grid;
use agogo::chan::time::tick::Tick;
use agogo::core::{
    MAX_OUTPUT_CHANNELS, OfflinePcm, OfflineRenderConfig, OfflineRenderReport, render_offline,
    render_offline_capture,
};
use bpaf::Bpaf;
use hound::{SampleFormat, WavSpec, WavWriter};
use serde_json::json;
use std::path::Path;

use crate::parse::{parse_bpm_to_tempo, parse_positive_u32};

const SUPPORTED_SAMPLE_RATES: &str = "44100, 48000, 88200, 96000, 176400, 192000";

#[derive(Debug, Clone, Bpaf)]
pub struct RenderArgs {
    /// Phase source. Only `internal` is supported for offline render.
    #[bpaf(long, argument("SOURCE"), fallback("internal".to_string()))]
    pub source: String,
    /// Tempo in beats per minute. Applies to all channels.
    #[bpaf(long, argument::<String>("BPM"), parse(parse_bpm_to_tempo))]
    pub bpm: Tempo,
    /// Sample rate in Hz. Six rates supported: 44100 / 48000 /
    /// 88200 / 96000 / 176400 / 192000.
    #[bpaf(long, argument("SR"), parse(parse_positive_u32), fallback(48_000))]
    pub sr: u32,
    /// Offline render buffer size in frames.
    #[bpaf(long, argument("FRAMES"), parse(parse_positive_u32), fallback(1024))]
    pub buffer_frames: u32,
    /// Render duration in 4/4 bars.
    #[bpaf(long, argument("BARS"), parse(parse_positive_u32), fallback(1))]
    pub duration_bars: u32,
    /// Number of interleaved output channels in the rendered audio
    /// buffer. Each `dev=audio` channel must declare a lane via
    /// `out=N` with `0 <= N < output_channels`; lanes must be
    /// unique (no mix bus). Range: 1..=16.
    #[bpaf(long, argument("CHANNELS"), parse(parse_positive_u32), fallback(2))]
    pub output_channels: u32,
    /// Per-channel spec, repeatable. Audio channels must spell
    /// `out=N` (lane index); MIDI/CV channels may use `out=diag`
    /// for offline diagnostics.
    #[bpaf(long, argument("SPEC"), many)]
    pub ch: Vec<String>,
    /// Optional path to write a 32-bit float multi-channel WAV file
    /// alongside the JSON report. The WAV's channel count matches
    /// `--output-channels`; each `out=N` audio channel writes to
    /// lane `N` and unrouted lanes are silence. When omitted, no
    /// file is written and the JSON report is the only output
    /// (back-compat with existing callers).
    #[bpaf(long, argument("FILE"))]
    pub wav_out: Option<String>,
}

pub fn render(args: &RenderArgs) -> Result<(), String> {
    if args.source != "internal" {
        return Err(format!(
            "--source {} unsupported for offline render (expected internal)",
            args.source
        ));
    }
    if args.ch.is_empty() {
        return Err("at least one --ch <spec> is required".to_string());
    }

    validate_schedule_params(args.sr, args.bpm).map_err(|e| match e {
        agogo::chan::channel::time::ScheduleError::UnsupportedSampleRate(sr) => {
            format!("--sr {sr} not supported (allowed: {SUPPORTED_SAMPLE_RATES})")
        }
        other => format!("invalid scheduling parameters: {other}"),
    })?;

    let output_channels: u16 = u16::try_from(args.output_channels)
        .ok()
        .filter(|&n| (1..=MAX_OUTPUT_CHANNELS).contains(&n))
        .ok_or_else(|| {
            format!(
                "--output-channels {} not supported (must be 1..={MAX_OUTPUT_CHANNELS})",
                args.output_channels
            )
        })?;

    let channels = parse_channels(&args.ch)?;
    let total_frames = frames_for_bars(args.duration_bars, args.bpm, args.sr)?;
    let cfg = OfflineRenderConfig {
        channels,
        bpm: args.bpm,
        sample_rate: args.sr,
        buffer_frames: args.buffer_frames,
        total_frames,
        output_channels,
    };
    let report: OfflineRenderReport = if let Some(wav_path) = args.wav_out.as_deref() {
        let (report, pcm) = render_offline_capture(cfg).map_err(|e| e.to_string())?;
        write_wav(wav_path, &pcm)?;
        report
    } else {
        render_offline(cfg).map_err(|e| e.to_string())?
    };

    let midi: Vec<_> = report
        .midi
        .iter()
        .map(|record| {
            json!({
                "sample": record.at_sample,
                "bytes": record.bytes,
            })
        })
        .collect();

    let doc = json!({
        "source": "internal",
        "sample_rate": report.sample_rate,
        "bpm": tempo_decimal(args.bpm),
        "duration": {
            "bars": args.duration_bars,
            "frames": report.total_frames,
        },
        "buffer_frames": report.buffer_frames,
        "buffers": report.buffers_rendered,
        "dropped": report.dropped,
        "timing": {
            "backend": "offline",
            "capability": "best_effort",
        },
        "midi": midi,
        "audio": {
            "nonzero_samples": report.audio_nonzero_samples,
            "peak_q15": report.audio_peak_q15,
            "positive_peak_q15": report.audio_positive_peak_q15,
            "negative_peak_q15": report.audio_negative_peak_q15,
        },
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&doc).map_err(|e| format!("render JSON failed: {e}"))?
    );
    Ok(())
}

fn parse_channels(raw: &[String]) -> Result<Vec<Channel>, String> {
    let named = match agogo::chan::channel::spec::parse_channels(raw) {
        Ok(named) => named,
        Err(e) => {
            let failing_entry = (0..raw.len()).find_map(|idx| {
                agogo::chan::channel::spec::parse_channels(&raw[..=idx])
                    .err()
                    .map(|_| (idx, raw[idx].as_str()))
            });

            return match failing_entry {
                Some((idx, spec)) => Err(format!("--ch[{idx}] `{spec}`: {e}")),
                None => Err(format!("--ch: {e}")),
            };
        }
    };

    validate_diagnostic_outputs(&named)?;

    named
        .into_iter()
        .map(|(id, spec)| spec.into_channel().map_err(|e| format!("--ch {id}: {e}")))
        .collect()
}

fn validate_diagnostic_outputs(
    named: &[(String, agogo::chan::channel::spec::ChannelSpec)],
) -> Result<(), String> {
    use agogo::chan::channel::spec::ChannelSpecRole;
    for (id, spec) in named {
        // Audio channels use `out=N` (lane index, parsed and
        // validated by the spec parser into `audio_lane`); MIDI/CV
        // channels keep the legacy device-name semantics where
        // only `diag`/`diagnostic` survives offline.
        if matches!(spec.role, ChannelSpecRole::Audio(_)) {
            continue;
        }
        match spec.out.as_deref() {
            None | Some("diag" | "diagnostic") => {}
            Some(out) => {
                return Err(format!(
                    "--ch {id}: out={out} is unsupported for offline render; use out=diag"
                ));
            }
        }
    }
    Ok(())
}

fn frames_for_bars(bars: u32, bpm: Tempo, sr: u32) -> Result<u64, String> {
    let ticks_per_bar = u64::from(Grid::T1.tick_count());
    let ticks = ticks_per_bar
        .checked_mul(u64::from(bars))
        .ok_or_else(|| format!("--duration-bars {bars} overflows tick range"))?;
    tick_to_whole_samples(Tick(ticks), bpm, sr)
        .ok_or_else(|| format!("could not convert {bars} bars to samples at --sr {sr}"))
}

/// Write the captured offline PCM as a 32-bit float WAV file.
///
/// `pcm.interleaved` is consumed in its native interleaved layout
/// (no transposition), which matches what `hound::WavWriter` wants
/// for multi-channel writes. Float-PCM is lossless against the
/// renderer's native sample format — no quantisation policy is
/// needed (separate plan).
fn write_wav(path: &str, pcm: &OfflinePcm) -> Result<(), String> {
    let spec = WavSpec {
        channels: pcm.channels,
        sample_rate: pcm.sample_rate,
        bits_per_sample: 32,
        sample_format: SampleFormat::Float,
    };
    let mut writer = WavWriter::create(Path::new(path), spec)
        .map_err(|e| format!("--wav-out {path}: create failed: {e}"))?;
    for &sample in &pcm.interleaved {
        writer
            .write_sample(sample)
            .map_err(|e| format!("--wav-out {path}: write failed: {e}"))?; // PCM ABI
    }
    writer
        .finalize()
        .map_err(|e| format!("--wav-out {path}: finalize failed: {e}"))?;
    Ok(())
}

fn tempo_decimal(tempo: Tempo) -> String {
    let whole = tempo.0 / 1_000_000;
    let frac = tempo.0 % 1_000_000;
    if frac == 0 {
        return whole.to_string();
    }
    let mut out = format!("{whole}.{frac:06}");
    while out.ends_with('0') {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tempo_decimal_trims_fraction() {
        assert_eq!(tempo_decimal(Tempo(120_000_000)), "120");
        assert_eq!(tempo_decimal(Tempo(120_500_000)), "120.5");
        assert_eq!(tempo_decimal(Tempo(120_125_000)), "120.125");
    }

    #[test]
    fn frames_for_one_bar_at_120_bpm_48k() {
        assert_eq!(
            frames_for_bars(1, Tempo::from_bpm_integer(120), 48_000).unwrap(),
            96_000
        );
    }
}
