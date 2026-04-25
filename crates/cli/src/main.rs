#![forbid(unsafe_code)]

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
        #[bpaf(long, argument("BPM"), parse(parse_positive_f64))]
        bpm: f64,
        /// Sample rate in Hz. Plan 13 T5 supports 48000 only;
        /// other rates from the channel pipeline's allowlist
        /// arrive when `agogo run` lands in Plan 14.
        #[bpaf(long, argument("SR"), parse(parse_positive_u32), fallback(48_000))]
        sr: u32,
        /// Per-channel divider — `t32t` for spec-compliant 24
        /// PPQN MIDI clock at 192 PPQN master.
        #[bpaf(long, argument("TBASE"))]
        divider: String,
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
    /// master ticks. At agogo's 192 PPQN master that's every 8
    /// master ticks, which is `TBase::T32t` (32nd-note triplet).
    /// Pick `--divider t4` for one byte per beat (human-readable);
    /// pick `--divider t32t` for a spec-compliant 24 PPQN stream.
    #[bpaf(command("trace"))]
    Trace {
        /// Tempo in beats per minute.
        #[bpaf(long, argument("BPM"), parse(parse_positive_f64))]
        bpm: f64,
        /// Sample rate in Hz.
        #[bpaf(long, argument("SR"), parse(parse_positive_u32))]
        sr: u32,
        /// Per-channel divider (e.g. `t4`, `t16`, `t32t`).
        #[bpaf(long, argument("TBASE"))]
        divider: String,
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
        #[bpaf(long, argument("BPM"), parse(parse_positive_f64), fallback(120.0))]
        initial_bpm: f64,
        /// Sample rate for the sample-index ↔ host-time mapping.
        #[bpaf(long, argument("SR"), parse(parse_positive_u32), fallback(48_000))]
        sr: u32,
        /// Total probe duration in ms.
        #[bpaf(long, argument("DURATION_MS"), parse(parse_positive_u32), fallback(3_000))]
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
        #[bpaf(long, argument("BPM"), parse(parse_positive_f64))]
        bpm: f64,
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
        #[bpaf(long, argument("BPM"), parse(parse_positive_f64), fallback(120.0))]
        bpm: f64,
        /// Quantum in bars (used when armed channels land with
        /// `snap_to_quantum` exposure via the preset sprint; ignored
        /// by the bare `link transport` runner).
        #[bpaf(long, argument("QUANTUM"), parse(parse_positive_f64), fallback(4.0))]
        quantum: f64,
        /// Sample rate (bound for the anchor; transport path itself
        /// doesn't use it, but the anchor is non-optional).
        #[bpaf(long, argument("SR"), parse(parse_positive_u32), fallback(48_000))]
        sr: u32,
        /// Total run duration, ms.
        #[bpaf(long, argument("DURATION_MS"), parse(parse_positive_u32), fallback(5_000))]
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
        #[bpaf(long, argument("BPM"), parse(parse_positive_f64), fallback(120.0))]
        bpm: f64,
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
        #[bpaf(long, argument("BPM"), parse(parse_positive_f64))]
        bpm: f64,
        #[bpaf(long, argument("SR"), parse(parse_positive_u32))]
        sr: u32,
        #[bpaf(long, argument("PPQ"), parse(parse_positive_u32))]
        ppq: u32,
        #[bpaf(long, argument("JITTER_US"), parse(parse_non_negative_f64), fallback(0.0))]
        jitter_us: f64,
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
        #[bpaf(long, argument("BPM"), parse(parse_positive_f64))]
        bpm: f64,
        /// Sample rate in Hz.
        #[bpaf(long, argument("SR"), parse(parse_positive_u32))]
        sr: u32,
        /// Per-channel divider (e.g. `t4`, `t16`, `t8t`).
        #[bpaf(long, argument("TBASE"))]
        divider: String,
        /// `SwingConfig::amount` with `multiplier = 1`.
        #[bpaf(long, argument("AMOUNT"), fallback(0))]
        shuffle: i32,
        /// Positive latency shift in ms; clamped to `[0, 300]` inside
        /// the transform. Non-finite or negative values rejected at
        /// the CLI boundary.
        #[bpaf(long, argument("SHIFT_MS"), parse(parse_non_negative_f64), fallback(0.0))]
        shift_ms: f64,
        /// Signed calibration offset in ms. Must be finite.
        #[bpaf(long, argument("OFFSET_MS"), parse(parse_finite_f64), fallback(0.0))]
        offset_ms: f64,
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

fn parse_positive_f64(v: f64) -> Result<f64, String> {
    if v.is_finite() && v > 0.0 {
        Ok(v)
    } else {
        Err(format!("must be a positive finite number, got {v}"))
    }
}

fn parse_non_negative_f64(v: f64) -> Result<f64, String> {
    if v.is_finite() && v >= 0.0 {
        Ok(v)
    } else {
        Err(format!("must be a non-negative finite number, got {v}"))
    }
}

fn parse_finite_f64(v: f64) -> Result<f64, String> {
    if v.is_finite() {
        Ok(v)
    } else {
        Err(format!("must be a finite number, got {v}"))
    }
}

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
                if sr != <agogo_core::fxp::S48 as agogo_core::fxp::SampleRate>::HZ {
                    eprintln!(
                        "error: sync trace is pinned to 48 kHz this sprint (got --sr {sr}); \
                         multi-rate support deferred"
                    );
                    std::process::exit(2);
                }
                let rows = sync_trace::trace(bpm, ppq, jitter_us, pulses, seed);
                println!("bits_q48_16,tempo_ubpm,phase_q32");
                for r in rows {
                    println!(
                        "{},{},{}",
                        r.bits_q48_16, r.tempo_ubpm, r.phase_q32
                    );
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
                    divider,
                    shuffle,
                    shift_ms,
                    offset_ms,
                    frames,
                    buffers,
                },
        }) => {
            #[cfg(feature = "core")]
            {
                let args = channel_trace::TraceArgs {
                    bpm,
                    sr,
                    divider,
                    shuffle,
                    shift_ms,
                    offset_ms,
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
                let _ = (bpm, sr, divider, shuffle, shift_ms, offset_ms, frames, buffers);
                eprintln!("error: build with --features core to enable `channel trace`");
                std::process::exit(2);
            }
        }
        Some(Command::Midi {
            sub:
                MidiSub::Trace {
                    bpm,
                    sr,
                    divider,
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
                    divider,
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
                let _ = (bpm, sr, divider, frames, buffers, start, stop_on_exit);
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
                    "# schedule: {} bars, tbase={}, swing={:.3}",
                    args.bars, args.tbase, args.swing
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
                let tempo_bpm = f64::from(row.tempo.0) / 1.0e6;
                let phase = f64::from(row.phase.0) / (1u64 << 32) as f64;
                println!(
                    "{},{},{:.4},{:.6}",
                    row.t_ms, row.peers, tempo_bpm, phase
                );
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
                    divider,
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
                divider,
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
pub mod link_probe {
    use agogo_core::sync::PhaseSourceImpl;
    use agogo_host_link::{HostTimeAnchor, LinkClock};
    use std::num::NonZeroU32;
    use std::thread::sleep;
    use std::time::{Duration, Instant};

    #[derive(Debug, Clone, Copy)]
    pub struct ProbeRow {
        /// Milliseconds since probe start. `u64` so a
        /// `--duration-ms u32::MAX` probe (~49 days) still represents
        /// monotonically-increasing timestamps end-to-end.
        pub t_ms: u64,
        pub peers: u64,
        pub tempo: agogo_core::fxp::Tempo,
        /// Beat-phase in `[0, 1)` at sample `t_ms × sr / 1000`,
        /// mapped through the anchor captured at probe start.
        pub phase: agogo_core::fxp::Phase,
    }

    /// Run a probe loop for `duration_ms`, sampling every `period_ms`.
    /// Each sampled row is passed to `on_row` synchronously so callers
    /// can stream directly to stdout (or collect into a Vec for
    /// tests). Peer discovery is enabled for the duration of the call
    /// and disabled before return. Blocks the calling thread; intended
    /// for the CLI, not the audio callback.
    ///
    /// `period_ms` is clamped to a minimum of 1 — a zero period would
    /// turn the `sleep(Duration::ZERO)` inside the loop into a no-op
    /// and starve the row consumer if it can't keep up.
    pub fn probe<F: FnMut(ProbeRow)>(
        initial_bpm: f64,
        sr: u32,
        duration_ms: u32,
        period_ms: u32,
        mut on_row: F,
    ) {
        let period_ms = period_ms.max(1);
        // The CLI parser (`parse_positive_u32`) already enforces
        // `sr >= 1`. Preserve that invariant explicitly here so
        // non-CLI callers fail fast on `sr = 0` instead of silently
        // mapping to 1 and producing wrong sample-index math.
        let sr = NonZeroU32::new(sr).expect("probe requires a non-zero sample rate");
        // Capture Link's current host-time once and use it as the
        // anchor origin so the phase column reads as "cycles elapsed
        // since probe start" rather than against an arbitrary epoch.
        // Construct with a placeholder anchor, read `clock_micros`,
        // then `set_anchor` with the real origin — avoids the
        // two-AblLink-instance throwaway pattern.
        let initial_tempo = agogo_core::fxp::f64_bpm_to_tempo(initial_bpm);
        let mut clock = LinkClock::new(
            initial_tempo,
            HostTimeAnchor {
                host_origin_micros: 0,
                sample_rate: sr,
            },
        );
        clock.set_anchor(HostTimeAnchor {
            host_origin_micros: clock.clock_micros(),
            sample_rate: sr,
        });
        clock.enable(true);
        let start = Instant::now();
        let duration = Duration::from_millis(u64::from(duration_ms));
        let period = Duration::from_millis(u64::from(period_ms));
        loop {
            let elapsed = start.elapsed();
            if elapsed > duration {
                break;
            }
            let t_ms = elapsed.as_millis() as u64;
            // Convert t_ms → sample index using the anchor's sample
            // rate, then query phase.
            let n = t_ms * u64::from(sr.get()) / 1_000;
            let phase_u32 = clock.phase_at_sample(n).0;
            on_row(ProbeRow {
                t_ms,
                peers: clock.num_peers(),
                tempo: clock.tempo(),
                phase: agogo_core::fxp::Phase(phase_u32),
            });
            sleep(period);
        }
        clock.enable(false);
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// Smoke-test: probing for 100ms at 50ms period emits at
        /// least one row; first row has t_ms ≈ 0, peers = 0 (no LAN
        /// peer in test), tempo equal to the initial BPM, and phase
        /// in `[0, 1)`.
        ///
        /// Touches the network via `LinkClock::enable(true)` under
        /// the hood. `peers == 0` fails if a real Link peer is
        /// reachable on the test LAN; Plan 09 adds a
        /// `fixture_or_skip!`-style network gate.
        #[test]
        fn probe_emits_rows_and_keeps_initial_tempo() {
            let mut rows = Vec::new();
            probe(125.0, 48_000, 100, 50, |row| rows.push(row));
            assert!(!rows.is_empty(), "probe returned no rows");
            let first = rows[0];
            assert_eq!(first.peers, 0);
            // Tempo is integer µBPM: 125 BPM → 125_000_000.
            assert_eq!(
                first.tempo, agogo_core::fxp::Tempo(125_000_000),
                "tempo {:?} differs from initial Tempo(125_000_000)",
                first.tempo
            );
            // Phase is Q0.32 — in [0, 2^32), representing [0, 1) cycles.
            let phase_cycles = f64::from(first.phase.0) / (1u64 << 32) as f64;
            assert!(
                (0.0..1.0).contains(&phase_cycles),
                "phase {} not in [0, 1)",
                phase_cycles
            );
        }
    }
}

#[cfg(feature = "link")]
pub mod link_commands {
    //! Plan 09 link subcommands: `push-tempo`, `transport`, `diag`.
    //! All three exit after a bounded duration — none is a persistent
    //! daemon. `transport` drives the FSM headlessly; audio-callback
    //! integration (real `agogo run --link`) lands with Plan 05.

    use agogo_core::fxp::{Tempo, f64_beats_to_quantum, f64_bpm_to_tempo};
    use agogo_host_link::{HostTimeAnchor, LinkSession, LinkWriteConfig};
    use std::num::NonZeroU32;
    use std::thread::sleep;
    use std::time::{Duration, Instant};

    fn anchor_for(sr: u32) -> HostTimeAnchor {
        let sr = NonZeroU32::new(sr).expect("sr validated at argv boundary");
        HostTimeAnchor {
            host_origin_micros: 0,
            sample_rate: sr,
        }
    }

    /// One-shot tempo push. Enables the network, calls
    /// `session.set_tempo`, sleeps `settle_ms` so peers can capture,
    /// disables, and exits.
    pub fn push_tempo(bpm: f64, settle_ms: u32) {
        // argv boundary — f64 BPM → Tempo.
        let bpm_tempo: Tempo = f64_bpm_to_tempo(bpm);
        let mut session = LinkSession::new(
            bpm_tempo,
            anchor_for(48_000),
            LinkWriteConfig::default(),
        );
        session.enable(true);
        session.set_tempo(bpm_tempo);
        sleep(Duration::from_millis(u64::from(settle_ms)));
        session.enable(false);
        // stdout for scripts: single line with the pushed BPM.
        println!("pushed_bpm={bpm}");
    }

    /// Headless transport runner. Subscribes to Link's `is_playing`
    /// via `poll_transport` every 10 ms, prints state transitions,
    /// and optionally drives `UserStart` / `UserStop` at the bounds.
    pub fn transport(
        bpm: f64,
        quantum: f64,
        sr: u32,
        duration_ms: u32,
        start: bool,
        stop_on_exit: bool,
    ) {
        // argv boundary — f64 BPM / quantum dies here.
        let bpm_tempo: Tempo = f64_bpm_to_tempo(bpm);
        let quantum = f64_beats_to_quantum(quantum);
        let config = LinkWriteConfig {
            default_quantum: quantum,
            ..LinkWriteConfig::default()
        };
        let mut session = LinkSession::new(bpm_tempo, anchor_for(sr), config);
        session.enable(true);
        if start {
            session.user_start();
        }
        let deadline = Instant::now() + Duration::from_millis(u64::from(duration_ms));
        let poll_period = Duration::from_millis(10);
        let mut last_playing = session.is_playing();
        let mut last_peers = session.num_peers();
        let mut last_tempo = session.tempo();
        println!("t_ms,peers,tempo_bpm,is_playing");
        let start_instant = Instant::now();
        println!(
            "{},{},{:.4},{}",
            0, last_peers,
            f64::from(last_tempo.0) / 1_000_000.0,
            last_playing as u8,
        );
        while Instant::now() < deadline {
            sleep(poll_period);
            session.poll_transport();
            let playing = session.is_playing();
            let peers = session.num_peers();
            let tempo = session.tempo();
            if playing != last_playing || peers != last_peers || tempo != last_tempo {
                let t_ms = start_instant.elapsed().as_millis() as u64;
                println!(
                    "{},{},{:.4},{}",
                    t_ms, peers,
                    f64::from(tempo.0) / 1_000_000.0,
                    playing as u8,
                );
                last_playing = playing;
                last_peers = peers;
                last_tempo = tempo;
            }
        }
        if stop_on_exit {
            session.user_stop();
        }
        session.enable(false);
    }

    /// Single-line diagnostic summary.
    pub fn diag(bpm: f64, sr: u32, settle_ms: u32) {
        let bpm_tempo: Tempo = f64_bpm_to_tempo(bpm);
        let mut session = LinkSession::new(
            bpm_tempo,
            anchor_for(sr),
            LinkWriteConfig::default(),
        );
        session.enable(true);
        sleep(Duration::from_millis(u64::from(settle_ms)));
        session.poll_transport();
        let peers = session.num_peers();
        let tempo = session.tempo();
        let playing = session.is_playing();
        session.enable(false);
        println!(
            "peers={peers} tempo_bpm={:.4} is_playing={}",
            f64::from(tempo.0) / 1_000_000.0,
            playing as u8,
        );
    }
}

#[cfg(feature = "core")]
mod sync_trace {
    use agogo_core::arb::pulse_train;
    use agogo_core::fxp::{
        Extended, F64F12, ExtendedFloat, Pico, S48, SampleRate, SampleTime, Tempo,
        f64_bpm_to_tempo,
    };
    use agogo_core::sync::{DetectorConfig, PeakDetector, Pll, PllSettings};

    /// CSV row — integer fields throughout. Peak position is emitted
    /// as a single Q48.16 `bits_q48_16` value rather than split
    /// integer/fractional parts; splitting with signed fractional bits
    /// is inconsistent for negative sample positions (the integer part
    /// borrows from the fractional, so a Q48.16 value just below zero
    /// decomposes to `(-1, 0xFFFF)` with `0xFFFF as i16 = -1`, which
    /// doesn't reconstruct the original). The single-column form
    /// sidesteps the sign-convention question; consumers decode with
    /// `sample = bits >> 16`, `frac = bits & 0xFFFF` as needed.
    #[derive(Debug, Clone, Copy)]
    pub struct TraceRow {
        /// Peak position as raw Q48.16 bits at S48's 48 kHz.
        pub bits_q48_16: i64,
        /// PLL smoothed BPM × 10⁶.
        pub tempo_ubpm: u32,
        /// PLL phase, Q0.32 cycles.
        pub phase_q32: u32,
    }

    pub fn trace(
        bpm_f64: f64,
        ppq: u32,
        jitter_us: f64,
        pulses: u32,
        seed: u64,
    ) -> Vec<TraceRow> {
        // argv-boundary conversions. f64 dies on these two lines.
        let bpm: Tempo = f64_bpm_to_tempo(bpm_f64);
        // µs → seconds → Pico via upstream `F64F12` (lawful conn over
        // `ExtendedFloat<f64>`). `parse_non_negative_f64` at the bpaf
        // layer already rejected NaN / ±∞, so a finite-wrap here is
        // safe; the `PosInf` match arm catches out-of-range values.
        let jitter_s = jitter_us * 1.0e-6;
        let jitter: Pico = match F64F12.ceil(ExtendedFloat::Finite(jitter_s)) {
            Extended::Finite(p) => p,
            Extended::NegInf | Extended::PosInf => Pico(0),
        };

        let (samples, _truth): (Vec<f32>, Vec<S48>) =
            pulse_train::<S48>(bpm, ppq, jitter, pulses, seed);
        let pulse_rate_hz = agogo_core::fxp::tempo_to_hz(bpm, ppq);
        let spacing_samples = (S48::HZ as f64 / pulse_rate_hz) as u32;
        let mut detector = PeakDetector::<S48>::new(DetectorConfig {
            threshold_q15: 16_384, // 0.5 Q0.15
            hold_samples: spacing_samples / 2,
        });
        let mut pll = Pll::<S48>::new(PllSettings::DEFAULT, bpm, ppq);
        let peaks = detector.process(&samples, 0);
        peaks
            .into_iter()
            .map(|p| {
                let out = pll.step(Some(p.sample_index));
                TraceRow {
                    bits_q48_16: p.sample_index.to_bits_q48_16(),
                    tempo_ubpm: out.bpm.0,
                    phase_q32: out.phase.0,
                }
            })
            .collect()
    }
}

#[cfg(feature = "core")]
pub mod channel_trace {
    use agogo_core::channel::{Channel, ChannelMode, tick_stream};
    use agogo_core::fxp::{Extended, ExtendedFloat, F64F06, Micro, Tempo};
    use agogo_core::time::conn::SampleTickConn;
    use agogo_core::time::swing::SwingConfig;
    use agogo_core::time::tbase::TBase;
    use agogo_core::time::tick::PPQN;

    #[derive(Debug, Clone)]
    pub struct TraceArgs {
        pub bpm: f64,
        pub sr: u32,
        pub divider: String,
        pub shuffle: i32,
        pub shift_ms: f64,
        pub offset_ms: f64,
        pub frames: usize,
        pub buffers: u32,
    }


    #[derive(Debug, Clone, Copy)]
    pub struct TraceRow {
        pub buffer_index: u32,
        pub sample_index: u64,
        pub tick: u32,
    }

    /// Pure CPU scheduling trace — useful for testing without capturing
    /// stdout. Returns an error if `divider` isn't a valid `TBase`.
    pub fn trace(args: &TraceArgs) -> Result<Vec<TraceRow>, String> {
        let divider: TBase = args
            .divider
            .parse()
            .map_err(|e| format!("invalid --divider {}: {e}", args.divider))?;
        // argv-boundary: f64 BPM → µBPM. f64 dies right here.
        let bpm = {
            let scaled = (args.bpm * 1.0e6).round();
            if !(0.0..u32::MAX as f64).contains(&scaled) {
                return Err(format!(
                    "--bpm {} out of range (expected (0, {}] BPM)",
                    args.bpm,
                    u32::MAX as f64 / 1.0e6
                ));
            }
            Tempo(scaled as u32)
        };
        // Channel pipeline requires one of the six audio sample rates
        // supported by `fxp::pico_to_samples` (the downstream Pico →
        // Sample dispatch). Validate here rather than letting
        // `micro_to_samples` panic deep inside the transform.
        match args.sr {
            44_100 | 48_000 | 88_200 | 96_000 | 176_400 | 192_000 => {}
            _ => {
                return Err(format!(
                    "--sr {} unsupported; expected one of 44_100 / 48_000 / 88_200 / 96_000 / 176_400 / 192_000",
                    args.sr
                ));
            }
        }
        let stc = SampleTickConn::new(args.sr, bpm, PPQN);
        // argv-boundary: ms (f64) → Micro via the upstream `F64F06`
        // lawful conn. Out-of-range saturations are user errors, not
        // silent defaults — `parse_non_negative_f64` / `parse_finite_f64`
        // already validated finiteness, so an `Extended::PosInf` /
        // `Extended::NegInf` result means the user asked for a value
        // outside `Micro`'s ±i64 range (billions of years). Surface
        // that as an error rather than silently mapping to zero.
        let ms_to_micro = |flag: &str, ms: f64| -> Result<Micro, String> {
            match F64F06.ceil(ExtendedFloat::Finite(ms * 1.0e-3)) {
                Extended::Finite(m) => Ok(m),
                Extended::NegInf | Extended::PosInf => {
                    Err(format!("{flag} {ms} out of range"))
                }
            }
        };
        let shift = ms_to_micro("--shift-ms", args.shift_ms)?;
        let offset = ms_to_micro("--offset-ms", args.offset_ms)?;
        let channel = Channel {
            mode: ChannelMode::MidiClock,
            divider,
            shuffle: SwingConfig {
                amount: args.shuffle,
                multiplier: 1,
            },
            shift,
            offset,
            snap_to_quantum: None,
        };
        // Pre-flight: reject ranges where `buffers × frames` would
        // overflow `u64`. Silent wrap in release builds would produce
        // garbage sample indices.
        let frames_u64 = u64::try_from(args.frames)
            .map_err(|_| format!("trace range exceeds u64: --frames {}", args.frames))?;
        let total = u64::from(args.buffers)
            .checked_mul(frames_u64)
            .ok_or_else(|| {
                format!(
                    "trace range exceeds u64: --frames {} × --buffers {}",
                    args.frames, args.buffers
                )
            })?;
        let _ = total; // only needed for the overflow check above
        let mut rows = Vec::new();
        for b in 0..args.buffers {
            let start = u64::from(b)
                .checked_mul(frames_u64)
                .expect("checked above");
            for ev in tick_stream(&channel, &stc, start, args.frames) {
                rows.push(TraceRow {
                    buffer_index: b,
                    sample_index: ev.sample_index,
                    tick: ev.tick.0,
                });
            }
        }
        Ok(rows)
    }
}

pub mod midi_trace {
    use agogo_core::channel::{Channel, ChannelMode, scheduler::tick_stream};
    use agogo_core::fxp::{Micro, Tempo};
    use agogo_core::out::midi::{MidiRtByte, TestSink, render_channel_block};
    use agogo_core::time::conn::SampleTickConn;
    use agogo_core::time::swing::SwingConfig;
    use agogo_core::time::tbase::TBase;
    use agogo_core::time::tick::PPQN;

    #[derive(Debug, Clone)]
    pub struct TraceArgs {
        pub bpm: f64,
        pub sr: u32,
        pub divider: String,
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
        let divider: TBase = args
            .divider
            .parse()
            .map_err(|e| format!("invalid --divider {}: {e}", args.divider))?;
        // argv-boundary: f64 BPM → µBPM. f64 dies right here.
        let bpm = {
            let scaled = (args.bpm * 1.0e6).round();
            if !(0.0..u32::MAX as f64).contains(&scaled) {
                return Err(format!(
                    "--bpm {} out of range (expected (0, {}) BPM)",
                    args.bpm,
                    u32::MAX as f64 / 1.0e6
                ));
            }
            Tempo(scaled as u32)
        };
        // Match `channel_trace`'s sr gate: the transform pipeline's
        // `pico_to_samples` dispatch supports only these six rates and
        // panics deep inside otherwise. Plan 12 never hits that path
        // with zero shift/offset, but validating here surfaces bad argv
        // as a clean error instead of depending on that internal.
        match args.sr {
            44_100 | 48_000 | 88_200 | 96_000 | 176_400 | 192_000 => {}
            _ => {
                return Err(format!(
                    "--sr {} unsupported; expected one of 44_100 / 48_000 / 88_200 / 96_000 / 176_400 / 192_000",
                    args.sr
                ));
            }
        }
        let stc = SampleTickConn::new(args.sr, bpm, PPQN);
        let channel = Channel {
            mode: ChannelMode::MidiClock,
            divider,
            shuffle: SwingConfig {
                amount: 0,
                multiplier: 1,
            },
            shift: Micro::ZERO,
            offset: Micro::ZERO,
            snap_to_quantum: None,
        };
        // Overflow pre-flight matches channel_trace's shape.
        let frames_u64 = u64::try_from(args.frames)
            .map_err(|_| format!("trace range exceeds u64: --frames {}", args.frames))?;
        let _ = u64::from(args.buffers)
            .checked_mul(frames_u64)
            .ok_or_else(|| {
                format!(
                    "trace range exceeds u64: --frames {} × --buffers {}",
                    args.frames, args.buffers
                )
            })?;

        let sink = TestSink::new();
        let last = args.buffers.saturating_sub(1);
        for b in 0..args.buffers {
            let start_sample = u64::from(b)
                .checked_mul(frames_u64)
                .expect("checked above");
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
            let evs = tick_stream(&channel, &stc, start_sample, args.frames);
            render_channel_block(&channel, &evs, transport, start_sample, &sink);
        }
        Ok(sink
            .records()
            .into_iter()
            .map(|r| TraceRow {
                at_sample: r.at_sample,
                // Plan 12 only emits single-byte System Real-Time
                // messages (0xF8/0xFA/0xFB/0xFC); longer messages
                // arrive with Plan 14's MidiCc work and the CSV
                // schema widens then.
                byte: r.bytes[0],
            })
            .collect())
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use agogo_core::out::midi::{MIDI_CLOCK, MIDI_START, MIDI_STOP};

        fn base_args() -> TraceArgs {
            TraceArgs {
                bpm: 120.0,
                sr: 48_000,
                divider: "t4".to_string(),
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
            let stop_row = rows.iter().find(|r| r.byte == MIDI_STOP).expect("Stop byte");
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
        fn bad_divider_errors() {
            let args = TraceArgs {
                divider: "notatbase".to_string(),
                ..base_args()
            };
            assert!(trace(&args).is_err());
        }

        #[test]
        fn negative_bpm_errors() {
            let args = TraceArgs {
                bpm: -1.0,
                ..base_args()
            };
            assert!(trace(&args).is_err());
        }
    }
}

pub mod time_sched {
    use agogo_core::time::swing::{self, SwingConfig};
    use agogo_core::time::tbase::TBase;
    use agogo_core::time::tick::Tick;
    use bpaf::Bpaf;

    #[derive(Bpaf, Debug, Clone)]
    pub struct ScheduleArgs {
        /// Grid resolution (e.g. `t16`, `t8t`, `t128t`).
        #[bpaf(long, argument::<String>("TBASE"), parse(parse_tbase))]
        pub tbase: TBase,

        /// Swing ratio in `[0.5, 0.75]`: 0.5 = straight, 0.75 = full
        /// triplet swing. f64 per the CLI argv-boundary rule
        /// (CLAUDE.md §Repository conventions).
        #[bpaf(long, argument("SWING"), fallback(0.5))]
        pub swing: f64,

        /// Number of 4/4 bars to schedule. Bounded to `u16` (≤ 65535)
        /// so memory and stdout stay reasonable — 65535 × 192 ≈ 12.6M
        /// tick positions ≈ 50 MB Vec at the finest grid. The plan
        /// specified `u32`; narrowing the type is the simplest honest
        /// bound (see plan's Review section).
        #[bpaf(long, argument("BARS"))]
        pub bars: u16,
    }

    fn parse_tbase(s: String) -> Result<TBase, String> {
        s.parse()
    }

    /// Convert a `0.5..=0.75` swing ratio into a `SwingConfig`. The
    /// displacement is `(swing - 0.5) * 96` ticks (so 0.5 → 0,
    /// 0.75 → 48 = full triplet on a T16 grid), with `multiplier = 1`
    /// so `amount` directly expresses the tick displacement.
    ///
    /// Values outside `[0.5, 0.75]` are clamped.
    pub fn swing_to_config(swing: f64) -> SwingConfig {
        let clamped = swing.clamp(0.5, 0.75);
        let amount = ((clamped - 0.5) * 96.0).round() as i32;
        SwingConfig {
            amount,
            multiplier: 1,
        }
    }

    /// Produce the absolute tick positions for a schedule (one per
    /// grid step; swing, if any, is already folded in). Pure function
    /// — useful for testing without capturing stdout.
    pub fn schedule_ticks(args: &ScheduleArgs) -> Vec<Tick> {
        let cfg = swing_to_config(args.swing);
        // 4/4 assumption: one bar = 4 * PPQN = 768 ticks.
        let ticks_per_bar = 4 * agogo_core::time::tick::PPQN;
        let step_tc = args.tbase.tick_count();
        let steps_per_bar = ticks_per_bar / step_tc;
        // `bars` is `u16` so `u32::from(bars) * steps_per_bar` cannot
        // overflow: max = 65535 * 192 = 12_582_720, well inside `u32`.
        let total_steps = u32::from(args.bars) * steps_per_bar;

        (0..total_steps)
            .map(|step| {
                let nominal = Tick(step * step_tc);
                swing::effective_tick(&cfg, nominal)
            })
            .collect()
    }
}

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

    use agogo_core::channel::{Channel, ChannelMode};
    use agogo_core::fxp::{Micro, S48, SampleRate, Tempo};
    use agogo_core::host::{AudioHost, Config};
    use agogo_core::machine::{Machine, TransportPolicy};
    use agogo_core::sync::{DetectorConfig, PeakDetector, PhaseSource, Pll, PllSettings};
    use agogo_core::time::swing::SwingConfig;
    use agogo_core::time::tbase::TBase;
    use agogo_core::time::tick::PPQN;
    use agogo_host_cpal::CpalHost;
    use agogo_host_cpal::cpal::callback::CallbackState;
    use std::collections::VecDeque;
    use agogo_host_cpal::cpal::control::spsc;
    use agogo_host_midi::MidirSink;
    use std::sync::Arc;
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    /// `--ppq 24` — MIDI clock baseline. Hard-coded for the demo;
    /// the user picks the *output* PPQN via `--divider` (`t32t` =
    /// 24 PPQN at 192 PPQN master).
    const DEMO_PPQ: u32 = 24;

    pub struct DemoArgs {
        pub audio_in: String,
        pub midi_out: String,
        pub source: String,
        pub bpm: f64,
        pub sr: u32,
        pub divider: String,
        pub buffer_frames: u32,
        pub duration_ms: u32,
    }

    /// Run the demo for `args.duration_ms` ms, then drop the audio
    /// stream + drain handle. Returns the dropped-message count
    /// observed at shutdown via `eprintln!` and an exit-2 path if
    /// non-zero.
    pub fn run(args: &DemoArgs) -> Result<(), String> {
        let divider: TBase = args
            .divider
            .parse()
            .map_err(|e| format!("invalid --divider {}: {e}", args.divider))?;
        // argv-boundary BPM (matches channel_trace + midi_trace).
        let bpm = {
            let scaled = (args.bpm * 1.0e6).round();
            if !(0.0..u32::MAX as f64).contains(&scaled) {
                return Err(format!(
                    "--bpm {} out of range (expected (0, {}) BPM)",
                    args.bpm,
                    u32::MAX as f64 / 1.0e6
                ));
            }
            Tempo(scaled as u32)
        };
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
        // Plan 13 T5 instantiates `CallbackState<S48>` only —
        // multi-rate dispatch via a static `match args.sr { ... }`
        // arrives with `agogo run` in Plan 14.
        if args.sr != S48::HZ {
            return Err(format!(
                "--sr {} not yet supported by `agogo demo` (only 48000 in Plan 13 T5; \
                 wider rate dispatch lands with `agogo run` in Plan 14)",
                args.sr
            ));
        }

        // PhaseSource: Internal | External(Pll).
        let phase_source: PhaseSource<S48> = match args.source.as_str() {
            "internal" => PhaseSource::Internal { bpm },
            "external" => {
                // Detector + PLL defaults — calibrated for click-track
                // input at 48 kHz. The CLI doesn't expose every PLL
                // knob; `agogo sync trace` is the debugging surface
                // for tuning. `hold_samples = sr / 4` allows up to
                // ~240 BPM clicks without spurious double-detections.
                let detector = PeakDetector::<S48>::new(DetectorConfig {
                    threshold_q15: 16_384, // 0.5 in Q0.15
                    hold_samples: args.sr / 4,
                });
                let pll = Pll::<S48>::new(PllSettings::DEFAULT, bpm, DEMO_PPQ);
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
        let channel = Channel {
            mode: ChannelMode::MidiClock,
            divider,
            shuffle: SwingConfig {
                amount: 0,
                multiplier: 1,
            },
            shift: Micro::ZERO,
            offset: Micro::ZERO,
            snap_to_quantum: None,
        };
        let machine = Machine::<S48>::new(
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
        let mut state = CallbackState::<S48> { machine, producer };

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

        let stream_handle = host
            .run(cfg, cb)
            .map_err(|e| format!("cpal run: {e}"))?;

        eprintln!(
            "agogo demo: running for {} ms, --bpm {} --sr {} --divider {} \
             --source {} --audio-in {} --midi-out {}",
            args.duration_ms,
            args.bpm,
            args.sr,
            args.divider,
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

#[cfg(all(test, feature = "core"))]
mod tests {
    use super::channel_trace::{self, TraceArgs};
    use super::sync_trace::trace;
    use super::time_sched::{ScheduleArgs, schedule_ticks, swing_to_config};
    use agogo_core::time::swing::SwingConfig;
    use agogo_core::time::tbase::TBase;

    /// E2E gate from the plan's build gates: 256 pulses at 120 BPM /
    /// 48 kHz / 24 PPQ with 50 µs jitter must converge to within
    /// ±50 000 µBPM (0.05 BPM) of 120 × 10⁶ by the end of the trace.
    #[test]
    fn sync_trace_converges() {
        let rows = trace(120.0, 24, 50.0, 256, 1);
        assert_eq!(rows.len(), 256);
        let last = rows.last().unwrap();
        let err = (last.tempo_ubpm as i64 - 120_000_000).unsigned_abs();
        assert!(
            err < 50_000,
            "final µBPM err {} > 50_000 (got tempo_ubpm = {})",
            err,
            last.tempo_ubpm
        );
    }

    #[test]
    fn swing_to_config_straight() {
        assert_eq!(
            swing_to_config(0.5),
            SwingConfig {
                amount: 0,
                multiplier: 1,
            }
        );
    }

    #[test]
    fn swing_to_config_054() {
        // (0.54 - 0.5) * 96 = 3.84 → round to 4
        assert_eq!(
            swing_to_config(0.54),
            SwingConfig {
                amount: 4,
                multiplier: 1,
            }
        );
    }

    #[test]
    fn swing_to_config_075_is_full_triplet() {
        // (0.75 - 0.5) * 96 = 24 — half of a T16 step, which is the
        // triplet-feel displacement.
        assert_eq!(
            swing_to_config(0.75),
            SwingConfig {
                amount: 24,
                multiplier: 1,
            }
        );
    }

    #[test]
    fn swing_to_config_clamps() {
        assert_eq!(swing_to_config(0.0).amount, 0);
        assert_eq!(swing_to_config(1.0).amount, 24);
    }

    #[test]
    fn schedule_ticks_two_bars_t16_yields_32_positions() {
        let ticks = schedule_ticks(&ScheduleArgs {
            tbase: TBase::T16,
            swing: 0.5,
            bars: 2,
        });
        assert_eq!(ticks.len(), 32);
        // Straight T16 schedule: 0, 48, 96, ..., 1488.
        for (i, t) in ticks.iter().enumerate() {
            assert_eq!(t.0, (i as u32) * 48);
        }
    }

    #[test]
    fn schedule_ticks_swing_054_shifts_off_beats() {
        let ticks = schedule_ticks(&ScheduleArgs {
            tbase: TBase::T16,
            swing: 0.54,
            bars: 1,
        });
        // 16 steps. Off-beats (indices 1, 3, 5, …, 15) shifted by -4.
        let expected: Vec<u32> = (0..16u32)
            .map(|i| if i % 2 == 1 { i * 48 - 4 } else { i * 48 })
            .collect();
        let got: Vec<u32> = ticks.iter().map(|t| t.0).collect();
        assert_eq!(got, expected);
    }

    #[test]
    fn schedule_ticks_t128t_has_192_steps_per_bar() {
        let ticks = schedule_ticks(&ScheduleArgs {
            tbase: TBase::T128t,
            swing: 0.5,
            bars: 1,
        });
        assert_eq!(ticks.len(), 192);
    }

    /// Plan build gate: the trace command at 120 BPM / 48 kHz / T4
    /// divider / 4 096-frame buffers must produce events at
    /// samples 0, 24 000, 48 000, … (one quarter note = 24 000
    /// samples) across the first few buffers.
    #[test]
    fn channel_trace_t4_120bpm_matches_expected_samples() {
        let args = TraceArgs {
            bpm: 120.0,
            sr: 48_000,
            divider: "t4".to_string(),
            shuffle: 0,
            shift_ms: 0.0,
            offset_ms: 0.0,
            frames: 4_096,
            buffers: 16,
        };
        let rows = channel_trace::trace(&args).expect("valid args");
        // 16 buffers × 4096 frames = 65 536 samples. Quarter notes at
        // 24 000 samples: 0, 24 000, 48 000 fit.
        let samples: Vec<u64> = rows.iter().map(|r| r.sample_index).collect();
        assert_eq!(samples, vec![0, 24_000, 48_000]);
    }

    #[test]
    fn channel_trace_rejects_invalid_divider() {
        let args = TraceArgs {
            bpm: 120.0,
            sr: 48_000,
            divider: "nope".to_string(),
            shuffle: 0,
            shift_ms: 0.0,
            offset_ms: 0.0,
            frames: 4_096,
            buffers: 1,
        };
        assert!(channel_trace::trace(&args).is_err());
    }

    #[test]
    fn schedule_ticks_t1_has_one_step_per_bar() {
        let ticks = schedule_ticks(&ScheduleArgs {
            tbase: TBase::T1,
            swing: 0.5,
            bars: 4,
        });
        assert_eq!(ticks.len(), 4);
        assert_eq!(
            ticks.iter().map(|t| t.0).collect::<Vec<_>>(),
            vec![0, 768, 1536, 2304]
        );
    }
}
