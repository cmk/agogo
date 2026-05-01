//! Trace subcommands and shared trace plumbing.

pub mod channel;
pub mod midi;
pub mod sync;

use agogo_core::channel::ChannelCommon;
use agogo_core::conn::fixed::Micro;
use agogo_core::time::grid::Grid;
use agogo_core::time::swing::SwingConfig;
use agogo_core::time::tbase::TBase;
use bpaf::Bpaf;

use crate::parsers::{
    parse_bpm_to_tempo, parse_jitter_us_to_pico, parse_ms_to_micro, parse_positive_u32,
};

#[derive(Debug, Clone, Bpaf)]
pub enum SyncSub {
    /// Synthesise a pulse train and trace the detector + PLL output as CSV.
    ///
    /// One row per detected peak: `sample_index,bpm_estimate,phase_estimate`.
    #[bpaf(command("trace"))]
    Trace {
        #[bpaf(long, argument::<String>("BPM"), parse(parse_bpm_to_tempo))]
        bpm: agogo_core::conn::tempo::Tempo,
        #[bpaf(long, argument("SR"), parse(parse_positive_u32))]
        sr: u32,
        #[bpaf(long, argument("PPQ"), parse(parse_positive_u32))]
        ppq: u32,
        #[bpaf(long, argument::<String>("JITTER_US"), parse(parse_jitter_us_to_pico), fallback(agogo_core::conn::fixed::Pico::ZERO))]
        jitter_us: agogo_core::conn::fixed::Pico,
        #[bpaf(long, argument("PULSES"), parse(parse_positive_u32))]
        pulses: u32,
        #[bpaf(long, argument("SEED"), fallback(1))]
        seed: u64,
    },
}

#[derive(Debug, Clone, Bpaf)]
pub enum ChannelSub {
    /// Run the per-channel scheduler over a sequence of audio buffers
    /// and print the resulting events as CSV:
    /// `buffer_index,sample_index,tick`.
    #[bpaf(command("trace"))]
    Trace {
        /// Tempo in beats per minute.
        #[bpaf(long, argument::<String>("BPM"), parse(parse_bpm_to_tempo))]
        bpm: agogo_core::conn::tempo::Tempo,
        /// Sample rate in Hz.
        #[bpaf(long, argument("SR"), parse(parse_positive_u32))]
        sr: u32,
        /// Grid name (e.g. `t4`, `t16`, `t8t`, `t8q`, `t2p`).
        #[bpaf(long, argument("EXPR"))]
        grid: String,
        /// Positive delay compensation in ms; clamped to `[0, 300]`
        /// inside the transform. Non-finite or negative values
        /// rejected at the CLI boundary.
        #[bpaf(long, argument::<String>("MS"), parse(parse_ms_to_micro), fallback(agogo_core::conn::fixed::Micro::ZERO))]
        delay: agogo_core::conn::fixed::Micro,
        /// Audio buffer length in samples.
        #[bpaf(long, argument("FRAMES"))]
        frames: usize,
        /// Number of consecutive buffers to schedule.
        #[bpaf(long, argument("BUFFERS"), parse(parse_positive_u32))]
        buffers: u32,
    },
}

#[derive(Debug, Clone, Bpaf)]
pub enum MidiSub {
    /// Run the MIDI-clock renderer over a sequence of audio buffers
    /// against a synthetic sink and print the emitted bytes as CSV:
    /// `at_sample,byte`.
    ///
    /// MIDI 1.0 pins clock at 24 PPQN — one `0xF8` every `PPQN/24`
    /// master ticks. At agogo's 960 PPQN master that's every 40
    /// master ticks, which is `Grid::T64T` (64th-note triplet).
    /// Pick `--grid t4` for one byte per beat (human-readable);
    /// pick `--grid t64t` for a spec-compliant 24 PPQN stream.
    #[bpaf(command("trace"))]
    Trace {
        /// Tempo in beats per minute.
        #[bpaf(long, argument::<String>("BPM"), parse(parse_bpm_to_tempo))]
        bpm: agogo_core::conn::tempo::Tempo,
        /// Sample rate in Hz.
        #[bpaf(long, argument("SR"), parse(parse_positive_u32))]
        sr: u32,
        /// Per-channel grid (e.g. `t4`, `t16`, `t32t`, `t8q`, `t2p`).
        #[bpaf(long, argument("GRID"))]
        grid: String,
        /// Audio buffer length in samples.
        #[bpaf(long, argument("FRAMES"))]
        frames: usize,
        /// Number of consecutive buffers to render.
        #[bpaf(long, argument("BUFFERS"), parse(parse_positive_u32))]
        buffers: u32,
        /// Inject `MidiRtByte::Start` (0xFA) at sample 0 of buffer 0.
        #[bpaf(long)]
        start: bool,
        /// Inject `MidiRtByte::Stop` (0xFC) at the first sample of
        /// the final buffer.
        #[bpaf(long)]
        stop_on_exit: bool,
    },
}

pub fn dispatch_sync(sub: SyncSub) -> Result<(), String> {
    match sub {
        SyncSub::Trace {
            bpm,
            sr,
            ppq,
            jitter_us,
            pulses,
            seed,
        } => {
            if sr != <agogo_core::conn::sample::S048 as agogo_core::conn::sample::SampleRate>::HZ {
                return Err(format!(
                    "sync trace is pinned to 48 kHz this sprint (got --sr {sr}); \
                     multi-rate support deferred"
                ));
            }
            let rows = sync::trace(bpm, ppq, jitter_us, pulses, seed);
            println!("bits_q48_16,tempo_ubpm,phase_q32");
            for r in rows {
                println!("{},{},{}", r.bits_q48_16, r.tempo_ubpm, r.phase_q32);
            }
            Ok(())
        }
    }
}

pub fn dispatch_channel(sub: ChannelSub) -> Result<(), String> {
    match sub {
        ChannelSub::Trace {
            bpm,
            sr,
            grid,
            delay,
            frames,
            buffers,
        } => {
            let args = channel::TraceArgs {
                bpm,
                sr,
                grid,
                delay,
                frames,
                buffers,
            };
            let rows = channel::trace(&args)?;
            println!("buffer_index,sample_index,tick");
            for row in rows {
                println!("{},{},{}", row.buffer_index, row.sample_index, row.tick);
            }
            Ok(())
        }
    }
}

pub fn dispatch_midi(sub: MidiSub) -> Result<(), String> {
    match sub {
        MidiSub::Trace {
            bpm,
            sr,
            grid,
            frames,
            buffers,
            start,
            stop_on_exit,
        } => {
            let args = midi::TraceArgs {
                bpm,
                sr,
                grid,
                frames,
                buffers,
                start,
                stop_on_exit,
            };
            let rows = midi::trace(&args)?;
            println!("at_sample,byte");
            for row in rows {
                println!("{},0x{:02X}", row.at_sample, row.byte);
            }
            Ok(())
        }
    }
}

pub(crate) fn parse_grid_arg(grid: &str) -> Result<Grid, String> {
    grid.parse()
        .map_err(|e| format!("invalid --grid {grid}: {e}"))
}

pub(crate) fn validate_audio_rate(sr: u32) -> Result<(), String> {
    match sr {
        44_100 | 48_000 | 88_200 | 96_000 | 176_400 | 192_000 => Ok(()),
        _ => Err(format!(
            "--sr {sr} unsupported; expected one of 44_100 / 48_000 / 88_200 / 96_000 / 176_400 / 192_000"
        )),
    }
}

pub(crate) fn straight_common(divider: Grid, delay: Micro) -> ChannelCommon {
    ChannelCommon {
        divider,
        shuffle: SwingConfig {
            resolution: TBase::T16,
            amount: 0,
        },
        delay,
        offset: Micro::ZERO,
        bar_multiplier: None,
    }
}

pub(crate) fn checked_trace_frames(frames: usize, buffers: u32) -> Result<u64, String> {
    let frames_u64 =
        u64::try_from(frames).map_err(|_| format!("trace range exceeds u64: --frames {frames}"))?;
    let _ = u64::from(buffers).checked_mul(frames_u64).ok_or_else(|| {
        format!("trace range exceeds u64: --frames {frames} × --buffers {buffers}")
    })?;
    Ok(frames_u64)
}
