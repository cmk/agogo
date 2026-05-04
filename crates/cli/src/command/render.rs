//! `agogo render` — deterministic hardware-free render report.

use agogo::chan::channel::Channel;
use agogo::chan::channel::time::validate_schedule_params;
use agogo::chan::conn::tempo::Tempo;
use agogo::chan::time::conn::tick_to_whole_samples;
use agogo::chan::time::grid::Grid;
use agogo::chan::time::tick::Tick;
use agogo::core::{OfflineRenderConfig, render_offline};
use bpaf::Bpaf;
use serde_json::json;

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
    /// Per-channel spec, repeatable. Uses the same parser as `agogo run`.
    #[bpaf(long, argument("SPEC"), many)]
    pub ch: Vec<String>,
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

    let channels = parse_channels(&args.ch)?;
    let total_frames = frames_for_bars(args.duration_bars, args.bpm, args.sr)?;
    let report = render_offline(OfflineRenderConfig {
        channels,
        bpm: args.bpm,
        sample_rate: args.sr,
        buffer_frames: args.buffer_frames,
        total_frames,
    })
    .map_err(|e| e.to_string())?;

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

    named
        .into_iter()
        .map(|(id, spec)| spec.into_channel().map_err(|e| format!("--ch {id}: {e}")))
        .collect()
}

fn frames_for_bars(bars: u32, bpm: Tempo, sr: u32) -> Result<u64, String> {
    let ticks_per_bar = u64::from(Grid::T1.tick_count());
    let ticks = ticks_per_bar
        .checked_mul(u64::from(bars))
        .ok_or_else(|| format!("--duration-bars {bars} overflows tick range"))?;
    tick_to_whole_samples(Tick(ticks), bpm, sr)
        .ok_or_else(|| format!("could not convert {bars} bars to samples at --sr {sr}"))
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
