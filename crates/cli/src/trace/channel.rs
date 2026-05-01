//! `agogo channel trace` handler — pure CPU scheduling trace.
//!
//! Constructs a `ChannelCommon` from CLI args and walks the
//! `tick_stream` scheduler across N buffers, emitting one CSV row
//! per scheduled event. No audio I/O — useful for testing the
//! divider/swing/delay pipeline without capturing stdout.
//!
//! Plan 2026-04-28-05 T2: extracted from `cli/main.rs`.

use agogo_core::conn::fixed::Micro;
use agogo_core::conn::tempo::Tempo;
use agogo_core::control::tick_stream;
use agogo_core::time::conn::SampleTickConn;
use agogo_core::time::tick::PPQN;

use super::{checked_trace_frames, parse_grid_arg, straight_common, validate_audio_rate};

#[derive(Debug, Clone)]
pub struct TraceArgs {
    pub bpm: Tempo,
    pub sr: u32,
    pub grid: String,
    pub delay: Micro,
    pub frames: usize,
    pub buffers: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct TraceRow {
    pub buffer_index: u32,
    pub sample_index: u64,
    pub tick: u64,
}

/// Pure CPU scheduling trace — useful for testing without capturing
/// stdout. Returns an error if `grid` isn't a valid `Grid` name.
pub fn trace(args: &TraceArgs) -> Result<Vec<TraceRow>, String> {
    let grid = parse_grid_arg(&args.grid)?;
    validate_audio_rate(args.sr)?;
    let stc = SampleTickConn::new(args.sr, args.bpm, PPQN);
    // channel trace operates only on the scheduler — it doesn't
    // construct full Channel variants, just the common field set.
    let common = straight_common(grid, args.delay);
    let frames_u64 = checked_trace_frames(args.frames, args.buffers)?;
    let mut rows = Vec::new();
    for b in 0..args.buffers {
        let start = u64::from(b).checked_mul(frames_u64).expect("checked above");
        for ev in tick_stream(&common, &stc, start, args.frames) {
            rows.push(TraceRow {
                buffer_index: b,
                sample_index: ev.sample_index,
                tick: ev.tick.0,
            });
        }
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Plan build gate: the trace command at 120 BPM / 48 kHz / T4
    /// grid / 4 096-frame buffers must produce events at
    /// samples 0, 24 000, 48 000, … (one quarter note = 24 000
    /// samples) across the first few buffers.
    #[test]
    fn channel_trace_t4_120bpm_matches_expected_samples() {
        let args = TraceArgs {
            bpm: Tempo::from_bpm_integer(120),
            sr: 48_000,
            grid: "t4".to_string(),
            delay: Micro::ZERO,
            frames: 4_096,
            buffers: 16,
        };
        let rows = trace(&args).expect("valid args");
        // 16 buffers × 4096 frames = 65 536 samples. Quarter notes at
        // 24 000 samples: 0, 24 000, 48 000 fit.
        let samples: Vec<u64> = rows.iter().map(|r| r.sample_index).collect();
        assert_eq!(samples, vec![0, 24_000, 48_000]);
    }

    #[test]
    fn channel_trace_rejects_invalid_grid() {
        let args = TraceArgs {
            bpm: Tempo::from_bpm_integer(120),
            sr: 48_000,
            grid: "nope".to_string(),
            delay: Micro::ZERO,
            frames: 4_096,
            buffers: 1,
        };
        assert!(trace(&args).is_err());
    }
}
