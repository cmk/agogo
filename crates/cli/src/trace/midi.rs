//! `agogo midi trace` — pure MIDI-clock trace via `TestSink`.
//!
//! Runs the scheduler + renderer for `buffers` buffers of `frames`
//! samples each, emits every `(at_sample, byte)` tuple in FIFO
//! order. No real MIDI I/O — useful for testing without capturing
//! stdout.
//!
//! Plan 2026-04-28-05 T6: extracted from `cli/main.rs`.

use agogo::core::channel::MidiRole;
use agogo::core::conn::fixed::Micro;
use agogo::core::conn::tempo::Tempo;
use agogo::core::control::event::tick_stream;
use agogo::core::sink::midi::{MidiRtByte, TestSink, render_midi_channel};

use super::{checked_trace_frames, parse_grid_arg, straight_common, validate_audio_rate};

#[derive(Debug, Clone)]
pub struct TraceArgs {
    pub bpm: Tempo,
    pub sr: u32,
    pub grid: String,
    pub frames: usize,
    pub buffers: u32,
    pub start: bool,
    pub stop_on_exit: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct TraceRow {
    pub at_sample: u64,
    pub byte: u8,
}

/// Pure MIDI-clock trace — runs the scheduler + renderer against
/// a `TestSink` for `buffers` buffers of `frames` samples each
/// and returns every emitted `(at_sample, byte)` in FIFO order.
pub fn trace(args: &TraceArgs) -> Result<Vec<TraceRow>, String> {
    let grid = parse_grid_arg(&args.grid)?;
    validate_audio_rate(args.sr)?;
    // MIDI trace dispatches the MIDI clock renderer directly —
    // no need to wrap in a full Channel::Midi variant.
    let common = straight_common(grid, Micro::ZERO);
    let role = MidiRole::Clock;
    let frames_u64 = checked_trace_frames(args.frames, args.buffers)?;

    let sink = TestSink::new();
    let last = args.buffers.saturating_sub(1);
    for b in 0..args.buffers {
        let start_sample = u64::from(b).checked_mul(frames_u64).expect("checked above");
        // Precedence when both `--start` and `--stop-on-exit`
        // target the same buffer (happens only with
        // `--buffers 1`): Start wins. `render_buffer` emits at
        // most one transport byte per call, and "stop before
        // start" is not musically meaningful — the caller is
        // expected to run a separate tracer for the stop side
        // if both bytes are required.
        let transport = match (args.start && b == 0, args.stop_on_exit && b == last) {
            (true, _) => Some(MidiRtByte::Start),
            (_, true) => Some(MidiRtByte::Stop),
            _ => None,
        };
        let evs = tick_stream(&common, args.sr, args.bpm, start_sample, args.frames)
            .map_err(|e| format!("invalid scheduling parameters: {e}"))?;
        render_midi_channel(&common, &role, &evs, transport, start_sample, None, &sink);
    }
    Ok(sink
        .records()
        .into_iter()
        .map(|r| TraceRow {
            at_sample: r.at_sample,
            // Plan 12 only emits single-byte System Real-Time
            // messages (0xF8/0xFA/0xFB/0xFC); longer messages
            // arrive with MIDI CC output work and the CSV
            // schema widens then.
            byte: r.bytes[0],
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use agogo::core::sink::midi::{MIDI_CLOCK, MIDI_START, MIDI_STOP};

    fn base_args() -> TraceArgs {
        TraceArgs {
            bpm: agogo::core::conn::tempo::Tempo::from_bpm_integer(120),
            sr: 48_000,
            grid: "t4".to_string(),
            frames: 24_000,
            buffers: 4,
            start: false,
            stop_on_exit: false,
        }
    }

    #[test]
    fn t4_120_48k_emits_quarter_notes() {
        // At 120 BPM / 48 kHz, one quarter note = 24 000 samples.
        // 4 buffers of 24 000 frames = 4 quarter notes at
        // 0, 24 000, 48 000, 72 000.
        let rows = trace(&base_args()).unwrap();
        assert_eq!(rows.len(), 4);
        let samples: Vec<u64> = rows.iter().map(|r| r.at_sample).collect();
        assert_eq!(samples, vec![0, 24_000, 48_000, 72_000]);
        for r in &rows {
            assert_eq!(r.byte, MIDI_CLOCK);
        }
    }

    #[test]
    fn start_flag_prepends_fa_at_sample_zero() {
        let args = TraceArgs {
            start: true,
            ..base_args()
        };
        let rows = trace(&args).unwrap();
        assert_eq!(rows[0].at_sample, 0);
        assert_eq!(rows[0].byte, MIDI_START);
        // First clock event follows immediately, also at sample 0.
        assert_eq!(rows[1].at_sample, 0);
        assert_eq!(rows[1].byte, MIDI_CLOCK);
    }

    #[test]
    fn stop_on_exit_flag_injects_fc_on_last_buffer() {
        let args = TraceArgs {
            stop_on_exit: true,
            ..base_args()
        };
        let rows = trace(&args).unwrap();
        // Final buffer starts at sample 3 × 24 000 = 72 000. The
        // Stop byte lands there ahead of that buffer's clock event.
        let stop_row = rows
            .iter()
            .find(|r| r.byte == MIDI_STOP)
            .expect("Stop byte");
        assert_eq!(stop_row.at_sample, 72_000);
    }

    #[test]
    fn unsupported_sr_errors() {
        let args = TraceArgs {
            sr: 45_000,
            ..base_args()
        };
        let err = trace(&args).unwrap_err();
        assert!(
            err.contains("unsupported"),
            "expected sr-range error, got: {err}"
        );
    }

    #[test]
    fn bad_grid_errors() {
        let args = TraceArgs {
            grid: "notatbase".to_string(),
            ..base_args()
        };
        assert!(trace(&args).is_err());
    }
}
