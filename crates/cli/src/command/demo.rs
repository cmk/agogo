//! `agogo demo run` single-channel end-to-end pipeline.
//!
//! Wires together cpal audio in via `agogo::host::cpal::CpalHost`, the
//! `CallbackState` hot loop, the rtrb SPSC + drain thread, and
//! midir output via `agogo::host::midi::MidirSink`. Single-channel
//! `MidiClock` for v0.1; `agogo run` generalises to N channels via
//! `Playhead`.
//!
//! Plan 2026-04-28-05 T7: extracted from `cli/main.rs`.

use agogo::core::channel::time::validate_schedule_params;
use agogo::core::channel::{Channel, ChannelCommon, MidiRole};
use agogo::core::conn::fixed::Micro;
use agogo::core::conn::sample::{S048, SampleRate};
use agogo::core::conn::tempo::Tempo;
use agogo::core::control::{DetectorConfig, PeakDetector, PhaseSource, Pll, PllSettings};
use agogo::core::sink::audio::{AudioHost, Config};
use agogo::core::time::grid::Grid;
use agogo::core::time::swing::SwingConfig;
use agogo::core::time::tbase::TBase;
use agogo::core::{Playhead, TransportPolicy};
use agogo::host::cpal::CpalHost;
use agogo::host::cpal::callback::CallbackState;
use agogo::host::cpal::control::spsc;
use agogo::host::midi::MidirSink;
use bpaf::Bpaf;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use crate::parse::{parse_bpm_to_tempo, parse_positive_u32};

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

#[derive(Debug, Clone, Bpaf)]
pub enum DemoSub {
    /// Run the demo pipeline. Connects cpal input + midir output,
    /// constructs a single MidiClock channel, and pumps the
    /// scheduler/renderer through the SPSC drain thread for
    /// `--duration-ms` ms.
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
        /// `external` drives a PLL from the audio input pulse train.
        #[bpaf(long, argument("SOURCE"))]
        source: String,
        /// Tempo in BPM.
        #[bpaf(long, argument::<String>("BPM"), parse(parse_bpm_to_tempo))]
        bpm: Tempo,
        /// Sample rate in Hz. `agogo demo` currently supports 48000.
        #[bpaf(long, argument("SR"), parse(parse_positive_u32), fallback(48_000))]
        sr: u32,
        /// Per-channel grid.
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
    #[bpaf(command("list-audio-inputs"))]
    ListAudioInputs,
    /// Print the names of midir output ports visible to the host.
    #[bpaf(command("list-midi-outputs"))]
    ListMidiOutputs,
}

pub fn dispatch(sub: DemoSub) -> Result<(), String> {
    match sub {
        DemoSub::Run {
            audio_in,
            midi_out,
            source,
            bpm,
            sr,
            grid,
            buffer_frames,
            duration_ms,
        } => run(&DemoArgs {
            audio_in,
            midi_out,
            source,
            bpm,
            sr,
            grid,
            buffer_frames,
            duration_ms,
        }),
        DemoSub::ListAudioInputs => {
            for name in list_audio_inputs() {
                println!("{name}");
            }
            Ok(())
        }
        DemoSub::ListMidiOutputs => {
            let names = list_midi_outputs()?;
            for name in names {
                println!("{name}");
            }
            Ok(())
        }
    }
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
    validate_schedule_params(args.sr, bpm)
        .map_err(|e| format!("invalid scheduling parameters: {e}"))?;
    // The demo instantiates `CallbackState<S048>` only. Multi-rate
    // dispatch via a static `match args.sr { ... }` lives in
    // `agogo run`.
    if args.sr != S048::HZ {
        return Err(format!(
            "--sr {} not yet supported by `agogo demo` (only 48000; \
             wider rate dispatch lives with `agogo run`)",
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
    let drain_sink: Arc<dyn agogo::core::sink::midi::MidiSink + Send + Sync> = sink;
    let drain = consumer.spawn_drain(drain_sink);

    // Playhead + CallbackState. `agogo run` generalises this
    // single-channel state to N channels; the demo keeps its
    // single-channel CLI surface by building a one-channel
    // Playhead with `TransportPolicy::Scripted { empty }` so the
    // emitted byte stream stays byte-identical to the original demo
    // path (no Start / Stop / Continue, just clock).
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
    let playhead = Playhead::<S048>::new(
        vec![channel],
        phase_source,
        args.sr,
        bpm,
        TransportPolicy::Scripted {
            schedule: VecDeque::new(),
        },
        args.buffer_frames as usize,
    );
    let mut state = CallbackState::<S048> { playhead, producer };

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
    let cb = Box::new(move |io: &mut agogo::core::sink::audio::AudioIo| {
        state.on_buffer(io);
    });

    let stream_handle = host.run(cfg, cb).map_err(|e| format!("cpal run: {e}"))?;

    eprintln!(
        "agogo demo: running for {} ms, --bpm {:.2} --sr {} --grid {} \
         --source {} --audio-in {} --midi-out {}",
        args.duration_ms,
        agogo::core::conn::boundary::tempo_to_f64_bpm(args.bpm),
        args.sr,
        args.grid,
        args.source,
        args.audio_in,
        midi_port_name,
    );

    // Block the main thread for the requested duration. Ctrl-C
    // handling lives with `agogo run`; for the demo a fixed duration
    // is sufficient.
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
