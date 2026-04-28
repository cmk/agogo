#![forbid(unsafe_code)]

#[cfg(feature = "run")]
mod run;

use bpaf::Bpaf;
use time_sched::schedule_args;

/// agogo workspace CLI
#[derive(Debug, Clone, Bpaf)]
#[bpaf(options)]
struct Cli {
    #[bpaf(external(command), optional)]
    command: Option<Command>,
}

#[derive(Debug, Clone, Bpaf)]
enum Command {
    /// Audio-clock sync utilities.
    #[bpaf(command("sync"))]
    Sync {
        #[bpaf(external(sync_sub))]
        sub: SyncSub,
    },
    /// Musical-time operations (Cirklon grid algebra).
    #[bpaf(command("time"))]
    Time {
        #[bpaf(external(time_op))]
        op: TimeOp,
    },
    /// Per-channel scheduler utilities.
    #[bpaf(command("channel"))]
    Channel {
        #[bpaf(external(channel_sub))]
        sub: ChannelSub,
    },
    /// MIDI output utilities.
    #[bpaf(command("midi"))]
    Midi {
        #[bpaf(external(midi_sub))]
        sub: MidiSub,
    },
    /// Ableton Link integration utilities.
    #[cfg(feature = "link")]
    #[bpaf(command("link"))]
    Link {
        #[bpaf(external(link_sub))]
        sub: LinkSub,
    },
    /// End-to-end demo: cpal audio in → PLL / Internal clock →
    /// scheduler → renderer → SPSC drain → midir MIDI out.
    #[cfg(feature = "demo")]
    #[bpaf(command("demo"))]
    Demo {
        #[bpaf(external(demo_sub))]
        sub: DemoSub,
    },
    /// Plan 14's end-to-end runner. N-channel Machine, six-rate
    /// dispatch, internal/external/link sources, Ctrl-C teardown.
    #[cfg(feature = "run")]
    #[bpaf(command("run"))]
    Run {
        #[bpaf(external(run::run_args))]
        args: run::RunArgs,
    },
}

#[cfg(feature = "demo")]
#[derive(Debug, Clone, Bpaf)]
enum DemoSub {
    /// Run the demo pipeline. Connects cpal input + midir output,
    /// constructs a single MidiClock channel, and pumps the
    /// scheduler/renderer through the SPSC drain thread for
    /// `--duration-ms` ms. (Plan 13 has no Ctrl-C handler; that lands
    /// with `agogo run` in Plan 14. Use `kill -9 $(pgrep agogo-cli)`
    /// for a hard exit before the duration elapses.)
    #[bpaf(command("run"))]
    Run {
        /// cpal input device name; pass `default` for the host's
        /// default input.
        #[bpaf(long, argument("DEVICE"))]
        audio_in: String,
        /// midir output port name; pass `default` for the first
        /// available output port.
        #[bpaf(long, argument("PORT"))]
        midi_out: String,
        /// Phase source: `internal` runs from `--bpm`,
        /// `external` drives a PLL from the audio input pulse
        /// train.
        #[bpaf(long, argument("SOURCE"))]
        source: String,
        /// Tempo in BPM.
        #[bpaf(long, argument::<String>("BPM"), parse(parse_bpm_to_tempo))]
        bpm: agogo_core::time::tempo::Tempo,
        /// Sample rate in Hz. Plan 13 T5 supports 48000 only;
        /// other rates from the channel pipeline's allowlist
        /// arrive when `agogo run` lands in Plan 14.
        #[bpaf(long, argument("SR"), parse(parse_positive_u32), fallback(48_000))]
        sr: u32,
        /// Per-channel grid — `t64t` for spec-compliant 24
        /// PPQN MIDI clock at 960 PPQN master.
        #[bpaf(long, argument("GRID"))]
        grid: String,
        /// cpal buffer size in frames.
        #[bpaf(long, argument("FRAMES"), parse(parse_positive_u32), fallback(1024))]
        buffer_frames: u32,
        /// How long to run before exiting.
        #[bpaf(long, argument("MS"), parse(parse_positive_u32), fallback(5_000))]
        duration_ms: u32,
    },
    /// Print the names of cpal input devices visible to the host.
    /// One per line.
    #[bpaf(command("list-audio-inputs"))]
    ListAudioInputs,
    /// Print the names of midir output ports visible to the host.
    /// One per line.
    #[bpaf(command("list-midi-outputs"))]
    ListMidiOutputs,
}

#[derive(Debug, Clone, Bpaf)]
enum MidiSub {
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
        bpm: agogo_core::time::tempo::Tempo,
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

#[cfg(feature = "link")]
#[derive(Debug, Clone, Bpaf)]
enum LinkSub {
    /// Probe a live Ableton Link session: emit CSV
    /// `t_ms,peers,tempo_bpm,phase` at a chosen period for a chosen
    /// duration. `phase` is the session's beat-phase at sample
    /// `t_ms × sr / 1000` mapped through a static `HostTimeAnchor`
    /// captured at probe start.
    #[bpaf(command("probe"))]
    Probe {
        /// Tempo to initialise Link with (BPM).
        #[bpaf(long, argument::<String>("BPM"), parse(parse_bpm_to_tempo), fallback(agogo_core::time::tempo::Tempo::from_bpm_integer(120)))]
        initial_bpm: agogo_core::time::tempo::Tempo,
        /// Sample rate for the sample-index ↔ host-time mapping.
        #[bpaf(long, argument("SR"), parse(parse_positive_u32), fallback(48_000))]
        sr: u32,
        /// Total probe duration in ms.
        #[bpaf(
            long,
            argument("DURATION_MS"),
            parse(parse_positive_u32),
            fallback(3_000)
        )]
        duration_ms: u32,
        /// Sampling period in ms.
        #[bpaf(long, argument("PERIOD_MS"), parse(parse_positive_u32), fallback(100))]
        period_ms: u32,
    },
    /// One-shot tempo push: connect to the Link network, set the
    /// session tempo, wait briefly so peers can capture the
    /// committed state, then exit. Useful for scripted tempo
    /// changes and for the manual E2E smoke test.
    #[bpaf(command("push-tempo"))]
    PushTempo {
        /// New tempo in BPM.
        #[bpaf(long, argument::<String>("BPM"), parse(parse_bpm_to_tempo))]
        bpm: agogo_core::time::tempo::Tempo,
        /// How long to keep the network session alive after
        /// committing, so peers see the change. Typical = 200 ms.
        #[bpaf(long, argument("SETTLE_MS"), parse(parse_positive_u32), fallback(200))]
        settle_ms: u32,
    },
    /// Headless transport FSM runner: subscribes to Link's
    /// `is_playing`, optionally publishes one-shot `UserStart` /
    /// `UserStop`, and prints transport-state transitions to
    /// stdout. Scaffolding for the v0.5 Sprint 01 forerun FSM and
    /// Sprint 02 PID sync.
    #[bpaf(command("transport"))]
    Transport {
        /// Initial BPM.
        #[bpaf(long, argument::<String>("BPM"), parse(parse_bpm_to_tempo), fallback(agogo_core::time::tempo::Tempo::from_bpm_integer(120)))]
        bpm: agogo_core::time::tempo::Tempo,
        /// Quantum in bars (consumed by the future orchestrator
        /// snap-arming path via `ChannelSpec::snap_intent` +
        /// `LinkSession::snap_offset_for`; ignored by the bare
        /// `link transport` runner).
        #[bpaf(long, argument::<String>("QUANTUM"), parse(parse_quantum_from_beats), fallback(agogo_host_link::Quantum::from_bars(4)))]
        quantum: agogo_host_link::Quantum,
        /// Sample rate (bound for the anchor; transport path itself
        /// doesn't use it, but the anchor is non-optional).
        #[bpaf(long, argument("SR"), parse(parse_positive_u32), fallback(48_000))]
        sr: u32,
        /// Total run duration, ms.
        #[bpaf(
            long,
            argument("DURATION_MS"),
            parse(parse_positive_u32),
            fallback(5_000)
        )]
        duration_ms: u32,
        /// Drive a `UserStart` at session start.
        #[bpaf(long)]
        start: bool,
        /// Drive a `UserStop` just before exit.
        #[bpaf(long)]
        stop_on_exit: bool,
    },
    /// Print a single line summary of the current Link session
    /// state: `peers,tempo_bpm,is_playing`. Useful from scripts.
    #[bpaf(command("diag"))]
    Diag {
        #[bpaf(long, argument::<String>("BPM"), parse(parse_bpm_to_tempo), fallback(agogo_core::time::tempo::Tempo::from_bpm_integer(120)))]
        bpm: agogo_core::time::tempo::Tempo,
        #[bpaf(long, argument("SR"), parse(parse_positive_u32), fallback(48_000))]
        sr: u32,
        /// How long to join the network before reading state. Too
        /// short and `peers` under-reports.
        #[bpaf(long, argument("SETTLE_MS"), parse(parse_positive_u32), fallback(500))]
        settle_ms: u32,
    },
}

#[derive(Debug, Clone, Bpaf)]
enum SyncSub {
    /// Synthesise a pulse train and trace the detector + PLL output as CSV.
    ///
    /// One row per detected peak: `sample_index,bpm_estimate,phase_estimate`.
    #[bpaf(command("trace"))]
    Trace {
        #[bpaf(long, argument::<String>("BPM"), parse(parse_bpm_to_tempo))]
        bpm: agogo_core::time::tempo::Tempo,
        #[bpaf(long, argument("SR"), parse(parse_positive_u32))]
        sr: u32,
        #[bpaf(long, argument("PPQ"), parse(parse_positive_u32))]
        ppq: u32,
        #[bpaf(long, argument::<String>("JITTER_US"), parse(parse_jitter_us_to_pico), fallback(agogo_core::time::decimal::Pico::ZERO))]
        jitter_us: agogo_core::time::decimal::Pico,
        #[bpaf(long, argument("PULSES"), parse(parse_positive_u32))]
        pulses: u32,
        #[bpaf(long, argument("SEED"), fallback(1))]
        seed: u64,
    },
}

#[derive(Debug, Clone, Bpaf)]
enum TimeOp {
    /// Print absolute tick positions for a schedule at a given TBase.
    /// On off-beat 16th-note steps the swing shift (if any) is
    /// applied before printing, so odd steps come out earlier than
    /// their nominal grid position.
    #[bpaf(command("schedule"))]
    Schedule(#[bpaf(external(schedule_args))] time_sched::ScheduleArgs),
}

#[derive(Debug, Clone, Bpaf)]
enum ChannelSub {
    /// Run the per-channel scheduler over a sequence of audio buffers
    /// and print the resulting events as CSV:
    /// `buffer_index,sample_index,tick`.
    #[bpaf(command("trace"))]
    Trace {
        /// Tempo in beats per minute.
        #[bpaf(long, argument::<String>("BPM"), parse(parse_bpm_to_tempo))]
        bpm: agogo_core::time::tempo::Tempo,
        /// Sample rate in Hz.
        #[bpaf(long, argument("SR"), parse(parse_positive_u32))]
        sr: u32,
        /// Grid name (e.g. `t4`, `t16`, `t8t`, `t8q`, `t2p`).
        #[bpaf(long, argument("EXPR"))]
        grid: String,
        /// Positive delay compensation in ms; clamped to `[0, 300]`
        /// inside the transform. Non-finite or negative values
        /// rejected at the CLI boundary.
        #[bpaf(long, argument::<String>("MS"), parse(parse_ms_to_micro), fallback(agogo_core::time::decimal::Micro::ZERO))]
        delay: agogo_core::time::decimal::Micro,
        /// Audio buffer length in samples.
        #[bpaf(long, argument("FRAMES"))]
        frames: usize,
        /// Number of consecutive buffers to schedule.
        #[bpaf(long, argument("BUFFERS"), parse(parse_positive_u32))]
        buffers: u32,
    },
}

fn parse_positive_u32(v: u32) -> Result<u32, String> {
    if v == 0 {
        Err("must be ≥ 1, got 0".to_string())
    } else {
        Ok(v)
    }
}

/// bpaf parser: --delay <ms-as-f64> → Micro at the argv-handler
/// boundary. Open-codes the ms→s shift inside the parser body —
/// `F064FD06` interprets f64 as canonical seconds, and there is no
/// `Conn<f64-as-ms, FD06>` rung. This is the documented
/// argv-boundary unit-shift exception (CLAUDE.md exception 4: f64
/// dies inside the handler body).
pub(crate) fn parse_ms_to_micro(s: String) -> Result<agogo_core::time::decimal::Micro, String> {
    use agogo_core::time::float::Extended;
    use agogo_core::time::float::ExtendedFloat;
    use agogo_core::time::float::F064FD06;
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

/// bpaf parser: --jitter-us <µs-as-f64> → Pico at the argv-handler
/// boundary. Same argv-boundary rationale as `parse_ms_to_micro`:
/// `F064FD12` interprets f64 as seconds, so the µs→s shift is
/// open-coded inside the parser body.
pub(crate) fn parse_jitter_us_to_pico(
    s: String,
) -> Result<agogo_core::time::decimal::Pico, String> {
    use agogo_core::time::float::Extended;
    use agogo_core::time::float::ExtendedFloat;
    use agogo_core::time::float::F064FD12;
    let us: f64 = s
        .parse()
        .map_err(|e| format!("--jitter-us {s}: not a number ({e})"))?;
    if !us.is_finite() || us < 0.0 {
        return Err(format!(
            "--jitter-us {us} invalid (expected non-negative finite µs)"
        ));
    }
    match F064FD12.ceil(ExtendedFloat::Extend(us * 1.0e-6)) {
        Extended::Finite(p) => Ok(p),
        Extended::PosInf | Extended::NegInf => Err(format!("--jitter-us {us} out of range")),
    }
}

/// bpaf parser: BPM `<f64>` → `Tempo` at the argv-handler boundary.
/// Used by every `--bpm` / `--initial-bpm` flag across the CLI;
/// errors avoid hardcoding a flag name so the message reads
/// correctly regardless of which option triggered the parse.
/// String dies inside the FromStr call; f64 dies on the last line.
pub(crate) fn parse_bpm_to_tempo(s: String) -> Result<agogo_core::time::tempo::Tempo, String> {
    use agogo_core::boundary::f64_bpm_to_tempo;
    let f: f64 = s
        .parse()
        .map_err(|e| format!("BPM value {s}: not a number ({e})"))?;
    if !f.is_finite() || f <= 0.0 || f > agogo_core::boundary::MAX_BPM_F64 {
        return Err(format!(
            "BPM value {f} out of range (expected (0, {}] BPM)",
            agogo_core::boundary::MAX_BPM_F64
        ));
    }
    Ok(f64_bpm_to_tempo(f))
}

/// bpaf parser: beats `<f64>` → `Quantum` at the argv-handler
/// boundary. Used by both `--link-quantum` (run) and `--quantum`
/// (link transport).
///
/// Plan 2026-04-28-03 T4 moved the implementation to
/// `agogo_host_link::quantum::parse_quantum_from_beats` (the parser
/// belongs alongside the type it produces). Re-exported here under
/// `feature = "link"` so existing bpaf attributes
/// (`parse(parse_quantum_from_beats)`) keep resolving.
#[cfg(feature = "link")]
pub(crate) use agogo_host_link::parse_quantum_from_beats;

fn main() {
    let cli = cli().run();
    match cli.command {
        Some(Command::Sync {
            sub:
                SyncSub::Trace {
                    bpm,
                    sr,
                    ppq,
                    jitter_us,
                    pulses,
                    seed,
                },
        }) => {
            #[cfg(feature = "core")]
            {
                if sr
                    != <agogo_core::time::sample::S048 as agogo_core::time::sample::SampleRate>::HZ
                {
                    eprintln!(
                        "error: sync trace is pinned to 48 kHz this sprint (got --sr {sr}); \
                         multi-rate support deferred"
                    );
                    std::process::exit(2);
                }
                let rows = sync_trace::trace(bpm, ppq, jitter_us, pulses, seed);
                println!("bits_q48_16,tempo_ubpm,phase_q32");
                for r in rows {
                    println!("{},{},{}", r.bits_q48_16, r.tempo_ubpm, r.phase_q32);
                }
            }
            #[cfg(not(feature = "core"))]
            {
                let _ = (bpm, sr, ppq, jitter_us, pulses, seed);
                eprintln!("error: build with --features core to enable `sync trace`");
                std::process::exit(2);
            }
        }
        Some(Command::Channel {
            sub:
                ChannelSub::Trace {
                    bpm,
                    sr,
                    grid,
                    delay,
                    frames,
                    buffers,
                },
        }) => {
            #[cfg(feature = "core")]
            {
                let args = channel_trace::TraceArgs {
                    bpm,
                    sr,
                    grid,
                    delay,
                    frames,
                    buffers,
                };
                let rows = match channel_trace::trace(&args) {
                    Ok(rows) => rows,
                    Err(e) => {
                        eprintln!("error: {e}");
                        std::process::exit(2);
                    }
                };
                println!("buffer_index,sample_index,tick");
                for row in rows {
                    println!("{},{},{}", row.buffer_index, row.sample_index, row.tick);
                }
            }
            #[cfg(not(feature = "core"))]
            {
                let _ = (bpm, sr, grid, delay, frames, buffers);
                eprintln!("error: build with --features core to enable `channel trace`");
                std::process::exit(2);
            }
        }
        Some(Command::Midi {
            sub:
                MidiSub::Trace {
                    bpm,
                    sr,
                    grid,
                    frames,
                    buffers,
                    start,
                    stop_on_exit,
                },
        }) => {
            #[cfg(feature = "core")]
            {
                let args = midi_trace::TraceArgs {
                    bpm,
                    sr,
                    grid,
                    frames,
                    buffers,
                    start,
                    stop_on_exit,
                };
                let rows = match midi_trace::trace(&args) {
                    Ok(rows) => rows,
                    Err(e) => {
                        eprintln!("error: {e}");
                        std::process::exit(2);
                    }
                };
                println!("at_sample,byte");
                for row in rows {
                    println!("{},0x{:02X}", row.at_sample, row.byte);
                }
            }
            #[cfg(not(feature = "core"))]
            {
                let _ = (bpm, sr, grid, frames, buffers, start, stop_on_exit);
                eprintln!("error: build with --features core to enable `midi trace`");
                std::process::exit(2);
            }
        }
        Some(Command::Time {
            op: TimeOp::Schedule(args),
        }) => {
            #[cfg(feature = "core")]
            {
                // Header to stderr (stdout reserved for the schedule
                // itself). Emits the parsed inputs so the reader can
                // correlate against the tick stream.
                eprintln!(
                    "# schedule: {} bars, grid={}, swing={:.3}",
                    args.bars, args.grid, args.swing
                );
                for t in time_sched::schedule_ticks(&args) {
                    println!("{}", t.0);
                }
            }
            #[cfg(not(feature = "core"))]
            {
                let _ = args;
                eprintln!("error: build with --features core to enable `time schedule`");
                std::process::exit(2);
            }
        }
        #[cfg(feature = "link")]
        Some(Command::Link {
            sub:
                LinkSub::Probe {
                    initial_bpm,
                    sr,
                    duration_ms,
                    period_ms,
                },
        }) => {
            println!("t_ms,peers,tempo_bpm,phase");
            link_probe::probe(initial_bpm, sr, duration_ms, period_ms, |row| {
                // Display-only conversion: fxp → f64 at println! time,
                // never stored in `ProbeRow`. f64 dies inside this
                // format string.
                let tempo_bpm = agogo_core::boundary::tempo_to_f64_bpm(row.tempo);
                let phase = f64::from(row.phase.0) / (1u64 << 32) as f64;
                println!("{},{},{:.4},{:.6}", row.t_ms, row.peers, tempo_bpm, phase);
            });
        }
        #[cfg(feature = "link")]
        Some(Command::Link {
            sub: LinkSub::PushTempo { bpm, settle_ms },
        }) => {
            link_commands::push_tempo(bpm, settle_ms);
        }
        #[cfg(feature = "link")]
        Some(Command::Link {
            sub:
                LinkSub::Transport {
                    bpm,
                    quantum,
                    sr,
                    duration_ms,
                    start,
                    stop_on_exit,
                },
        }) => {
            link_commands::transport(bpm, quantum, sr, duration_ms, start, stop_on_exit);
        }
        #[cfg(feature = "link")]
        Some(Command::Link {
            sub: LinkSub::Diag { bpm, sr, settle_ms },
        }) => {
            link_commands::diag(bpm, sr, settle_ms);
        }
        #[cfg(feature = "demo")]
        Some(Command::Demo {
            sub:
                DemoSub::Run {
                    audio_in,
                    midi_out,
                    source,
                    bpm,
                    sr,
                    grid,
                    buffer_frames,
                    duration_ms,
                },
        }) => {
            let args = demo::DemoArgs {
                audio_in,
                midi_out,
                source,
                bpm,
                sr,
                grid,
                buffer_frames,
                duration_ms,
            };
            if let Err(e) = demo::run(&args) {
                eprintln!("error: {e}");
                std::process::exit(2);
            }
        }
        #[cfg(feature = "demo")]
        Some(Command::Demo {
            sub: DemoSub::ListAudioInputs,
        }) => {
            for name in demo::list_audio_inputs() {
                println!("{name}");
            }
        }
        #[cfg(feature = "demo")]
        Some(Command::Demo {
            sub: DemoSub::ListMidiOutputs,
        }) => match demo::list_midi_outputs() {
            Ok(names) => {
                for name in names {
                    println!("{name}");
                }
            }
            Err(e) => {
                eprintln!("error: {e}");
                std::process::exit(2);
            }
        },
        #[cfg(feature = "run")]
        Some(Command::Run { args }) => {
            if let Err(e) = run::run(&args) {
                eprintln!("error: {e}");
                std::process::exit(2);
            }
        }
        None => {
            #[cfg(feature = "core")]
            let tag = "with core";
            #[cfg(not(feature = "core"))]
            let tag = "core disabled";
            println!("agogo-cli ({tag})");
        }
    }
}

#[cfg(feature = "link")]
pub mod link_probe;

#[cfg(feature = "link")]
pub mod link_commands;

#[cfg(feature = "core")]
mod sync_trace;

#[cfg(feature = "core")]
pub mod channel_trace;

pub mod midi_trace;

pub mod time_sched;

#[cfg(feature = "demo")]
pub mod demo {
    //! `agogo demo run` end-to-end pipeline (Plan 13 T5).
    //!
    //! Wires together every Plan 13 piece: cpal audio in via
    //! `host-cpal::CpalHost`, the `CallbackState` hot loop, the
    //! rtrb SPSC + drain thread, and midir output via
    //! `host-midi::MidirSink`. Single-channel `MidiClock` for v0.1;
    //! Plan 14's `agogo run` generalises to N channels via
    //! `Machine`.

    use agogo_core::channel::{Channel, ChannelCommon, MidiRole};
    use agogo_core::host::{AudioHost, Config};
    use agogo_core::machine::{Machine, TransportPolicy};
    use agogo_core::sync::{DetectorConfig, PeakDetector, PhaseSource, Pll, PllSettings};
    use agogo_core::time::decimal::Micro;
    use agogo_core::time::grid::Grid;
    use agogo_core::time::sample::{S048, SampleRate};
    use agogo_core::time::swing::SwingConfig;
    use agogo_core::time::tbase::TBase;
    use agogo_core::time::tempo::Tempo;
    use agogo_core::time::tick::PPQN;
    use agogo_host_cpal::CpalHost;
    use agogo_host_cpal::cpal::callback::CallbackState;
    use agogo_host_cpal::cpal::control::spsc;
    use agogo_host_midi::MidirSink;
    use std::collections::VecDeque;
    use std::sync::Arc;
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    /// `--ppq 24` — MIDI clock baseline. Hard-coded for the demo;
    /// the user picks the *output* PPQN via `--grid` (`t64t` =
    /// 24 PPQN at 960 PPQN master).
    const DEMO_PPQ: u32 = 24;

    pub struct DemoArgs {
        pub audio_in: String,
        pub midi_out: String,
        pub source: String,
        pub bpm: Tempo,
        pub sr: u32,
        pub grid: String,
        pub buffer_frames: u32,
        pub duration_ms: u32,
    }

    /// Run the demo for `args.duration_ms` ms, then drop the audio
    /// stream + drain handle. Returns the dropped-message count
    /// observed at shutdown via `eprintln!` and an exit-2 path if
    /// non-zero.
    pub fn run(args: &DemoArgs) -> Result<(), String> {
        let grid: Grid = args
            .grid
            .parse()
            .map_err(|e| format!("invalid --grid {}: {e}", args.grid))?;
        let bpm = args.bpm;
        // SR validation matches the channel pipeline's allowlist.
        match args.sr {
            44_100 | 48_000 | 88_200 | 96_000 | 176_400 | 192_000 => {}
            _ => {
                return Err(format!(
                    "--sr {} unsupported; expected one of 44_100 / 48_000 / 88_200 / 96_000 / 176_400 / 192_000",
                    args.sr
                ));
            }
        }
        // Plan 13 T5 instantiates `CallbackState<S048>` only —
        // multi-rate dispatch via a static `match args.sr { ... }`
        // arrives with `agogo run` in Plan 14.
        if args.sr != S048::HZ {
            return Err(format!(
                "--sr {} not yet supported by `agogo demo` (only 48000 in Plan 13 T5; \
                 wider rate dispatch lands with `agogo run` in Plan 14)",
                args.sr
            ));
        }

        // PhaseSource: Internal | External(Pll).
        let phase_source: PhaseSource<S048> = match args.source.as_str() {
            "internal" => PhaseSource::Internal { bpm },
            "external" => {
                // Detector + PLL defaults — calibrated for click-track
                // input at 48 kHz. The CLI doesn't expose every PLL
                // knob; `agogo sync trace` is the debugging surface
                // for tuning. `hold_samples = sr / 4` allows up to
                // ~240 BPM clicks without spurious double-detections.
                let detector = PeakDetector::<S048>::new(DetectorConfig {
                    threshold_q15: 16_384, // 0.5 in Q0.15
                    hold_samples: args.sr / 4,
                });
                let pll = Pll::<S048>::new(PllSettings::DEFAULT, bpm, DEMO_PPQ);
                PhaseSource::External { detector, pll }
            }
            other => {
                return Err(format!(
                    "--source {other} unknown (try `internal` or `external`)"
                ));
            }
        };

        // Open MIDI sink.
        let midi_port_name = if args.midi_out == "default" {
            MidirSink::list_output_ports()
                .map_err(|e| format!("midi enumeration: {e}"))?
                .into_iter()
                .next()
                .ok_or_else(|| {
                    "no MIDI output ports available (try `agogo demo \
                     list-midi-outputs`)"
                        .to_string()
                })?
        } else {
            args.midi_out.clone()
        };
        let sink = Arc::new(
            MidirSink::open(&midi_port_name)
                .map_err(|e| format!("midi open `{midi_port_name}`: {e}"))?,
        );

        // SPSC + drain.
        let (producer, consumer) = spsc(1024);
        let dropped_handle = producer.dropped_handle();
        let drain_sink: Arc<dyn agogo_core::out::midi::MidiSink + Send + Sync> = sink;
        let drain = consumer.spawn_drain(drain_sink);

        // Machine + CallbackState. Plan 14 generalises Plan 13's
        // single-channel state to N channels; the demo keeps its
        // single-channel CLI surface by building a one-channel
        // Machine with `TransportPolicy::Scripted { empty }` so the
        // emitted byte stream stays byte-identical to Plan 13's
        // (no Start / Stop / Continue, just clock).
        let channel = Channel::Midi {
            common: ChannelCommon {
                divider: grid,
                shuffle: SwingConfig {
                    resolution: TBase::T16,
                    amount: 0,
                },
                delay: Micro::ZERO,
                offset: Micro::ZERO,
                bar_multiplier: None,
            },
            role: MidiRole::Clock,
        };
        let machine = Machine::<S048>::new(
            vec![channel],
            phase_source,
            args.sr,
            bpm,
            PPQN,
            TransportPolicy::Scripted {
                schedule: VecDeque::new(),
            },
            args.buffer_frames as usize,
        );
        let mut state = CallbackState::<S048> { machine, producer };

        // Open audio host.
        let host = if args.audio_in == "default" {
            CpalHost::default_input()
        } else {
            CpalHost::with_input_name(&args.audio_in)
        }
        .map_err(|e| format!("cpal open: {e}"))?;
        let cfg = Config {
            input_device: None,
            output_device: None,
            sample_rate: args.sr,
            buffer_frames: args.buffer_frames,
            input_channels: 1,
            output_channels: 0,
        };

        // Move state into the data callback.
        let cb = Box::new(move |io: &mut agogo_core::host::AudioIo| {
            state.on_buffer(io);
        });

        let stream_handle = host.run(cfg, cb).map_err(|e| format!("cpal run: {e}"))?;

        eprintln!(
            "agogo demo: running for {} ms, --bpm {:.2} --sr {} --grid {} \
             --source {} --audio-in {} --midi-out {}",
            args.duration_ms,
            agogo_core::boundary::tempo_to_f64_bpm(args.bpm),
            args.sr,
            args.grid,
            args.source,
            args.audio_in,
            midi_port_name,
        );

        // Block the main thread for the requested duration. Ctrl-C
        // handling lands with `agogo run` (Plan 14); for the demo
        // a fixed duration is sufficient.
        std::thread::sleep(Duration::from_millis(u64::from(args.duration_ms)));

        // Tear down: stream first (stops the producer), then drain
        // (flushes the ring + joins the drain thread).
        drop(stream_handle);
        drop(drain);

        let dropped = dropped_handle.load(Ordering::Relaxed);
        if dropped > 0 {
            eprintln!("agogo demo: {dropped} messages dropped on overrun");
            return Err(format!("{dropped} messages dropped"));
        }
        eprintln!("agogo demo: clean exit, 0 dropped");
        Ok(())
    }

    /// Enumerate cpal input device names. Infallible — returns an
    /// empty `Vec` on a host with no input devices.
    pub fn list_audio_inputs() -> Vec<String> {
        CpalHost::list_input_devices()
    }

    /// Enumerate midir output port names. Returns the underlying
    /// midir init error if the platform's MIDI subsystem can't be
    /// queried at all.
    pub fn list_midi_outputs() -> Result<Vec<String>, String> {
        MidirSink::list_output_ports().map_err(|e| format!("midi enumeration: {e}"))
    }
}

