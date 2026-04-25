//! `agogo run` — Plan 14's end-to-end runner.
//!
//! Generalises Plan 13's single-channel `agogo demo run` into:
//!   - N channels via the docker-style repeatable `--ch` flag;
//!   - All six SampleTime rates via a static `match args.sr`;
//!   - Three sources: `internal | external | link`. `link` plugs
//!     `LinkSession` in via `PhaseSource::Custom` (Plan 14 T2);
//!   - Ctrl-C handling via the `ctrlc` crate (Plan 14 T4);
//!   - `MidiRtByte::Start` at first buffer / `Stop` on Ctrl-C
//!     teardown for `internal` and `external` sources;
//!     transition-driven Start/Stop for `link`.
//!
//! Feature-gated on `run` (= `demo + link + ctrlc`). Compiled in by
//! `cargo build -p agogo-cli --features run`.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use agogo_core::channel::Channel;
use agogo_core::fxp::{
    S44, S48, S88, S96, S176, S192, SampleRate, SampleTime, Tempo, f64_bpm_to_tempo,
};
use agogo_core::host::{AudioHost, AudioIo, Config};
use agogo_core::machine::{ChannelSpec, Machine, MachineStopHandle, TransportPolicy};
use agogo_core::sync::{DetectorConfig, PeakDetector, PhaseSource, Pll, PllSettings};
use agogo_core::time::tick::PPQN;
use agogo_host_cpal::CpalHost;
use agogo_host_cpal::cpal::callback::CallbackState;
use agogo_host_cpal::cpal::control::spsc;
use agogo_host_link::{
    HostTimeAnchor, LinkPhaseSource, LinkSession, LinkSessionHandle, LinkWriteConfig,
};
use agogo_host_midi::MidirSink;
use bpaf::Bpaf;
use std::num::NonZeroU32;

/// PLL pulse rate. `agogo run` external source feeds the detector +
/// PLL at MIDI clock cadence (24 PPQ); the master tick stream
/// scheduler uses [`PPQN`] (960).
const PULSE_PPQ: u32 = 24;

use crate::{parse_positive_f64, parse_positive_u32};

/// Argv container for `agogo run`. Used by both the bpaf derive and
/// the dispatcher in `main.rs`. Fields cross the argv boundary at
/// the handler's first lines: `--bpm` via `f64_bpm_to_tempo`,
/// `--link-quantum` through the `from_beats` helper exposed in
/// `agogo_core::fxp`.
#[derive(Debug, Clone, Bpaf)]
pub struct RunArgs {
    /// Tempo in beats per minute. Applies to all channels.
    #[bpaf(long, argument("BPM"), parse(parse_positive_f64))]
    pub bpm: f64, // argv boundary
    /// Sample rate in Hz. Six rates supported: 44100 / 48000 /
    /// 88200 / 96000 / 176400 / 192000.
    #[bpaf(long, argument("SR"), parse(parse_positive_u32), fallback(48_000))]
    pub sr: u32,
    /// cpal buffer size in frames.
    #[bpaf(long, argument("FRAMES"), parse(parse_positive_u32), fallback(1024))]
    pub buffer_frames: u32,
    /// Phase source: `internal` (free-running from --bpm),
    /// `external` (PLL from audio input click train), `link`
    /// (Ableton Link session — requires `--features link`).
    #[bpaf(long, argument("SOURCE"), fallback("internal".to_string()))]
    pub source: String,
    /// cpal input device name. Pass `default` for the host default.
    #[bpaf(long, argument("DEVICE"), fallback("default".to_string()))]
    pub audio_in: String,
    /// Per-channel spec, repeatable. One `--ch` per channel.
    /// Format: `key=val[,key=val]*`. Required keys: `div`, `dev`.
    /// Optional: `id`, `out`, `swing` (i8 tick offset),
    /// `swing-res` (binary resolution, default `t16`), `shift-ms`,
    /// `offset-ms`, `snap-quantum-us`. Quote values containing
    /// spaces or commas: `--ch "out=IAC Bus 1,div=t32t,dev=midi"`.
    #[bpaf(long, argument("SPEC"), many)]
    pub ch: Vec<String>,
    /// Link quantum in beats. Required when `--source link`;
    /// ignored otherwise. Default 4 (one bar of 4/4).
    #[bpaf(long, argument("BEATS"), parse(parse_positive_f64), optional)]
    pub link_quantum: Option<f64>, // argv boundary
    /// Mirror Link's `start_stop_sync` flag (publishes is_playing
    /// transitions to the Link network).
    #[bpaf(long)]
    pub link_enable_start_stop: bool,
    /// Hidden test-only flag: bound the run duration so the smoke
    /// suite can exit deterministically. Production runs use
    /// Ctrl-C.
    #[bpaf(long, argument("MS"), parse(parse_positive_u32), optional, hide)]
    pub max_duration_ms: Option<u32>,
}

/// Top-level entry. Dispatches the rate match and delegates to the
/// monomorphic [`run_with_rate`].
pub fn run(args: &RunArgs) -> Result<(), String> {
    if args.ch.is_empty() {
        return Err("at least one --ch <spec> is required (try `--ch \
                    dev=midi,div=t32t,out=default`)"
            .to_string());
    }

    // argv boundary: --bpm dies here; downstream sees only Tempo.
    let bpm: Tempo = f64_bpm_to_tempo(args.bpm);

    // Parse all --ch specs eagerly so a malformed entry fails
    // before any device opens.
    let channels: Vec<Channel> = args
        .ch
        .iter()
        .map(|s| {
            ChannelSpec::parse(s)
                .and_then(ChannelSpec::into_channel)
                .map_err(|e| format!("--ch `{s}`: {e}"))
        })
        .collect::<Result<_, _>>()?;

    // Static rate dispatch. Each arm monomorphises the entire
    // pipeline (Machine, PhaseSource, CallbackState) at its own
    // rate. `SampleTime::HZ` is the constant rate identifier.
    match args.sr {
        rate if rate == S44::HZ => run_with_rate::<S44>(args, bpm, channels),
        rate if rate == S48::HZ => run_with_rate::<S48>(args, bpm, channels),
        rate if rate == S88::HZ => run_with_rate::<S88>(args, bpm, channels),
        rate if rate == S96::HZ => run_with_rate::<S96>(args, bpm, channels),
        rate if rate == S176::HZ => run_with_rate::<S176>(args, bpm, channels),
        rate if rate == S192::HZ => run_with_rate::<S192>(args, bpm, channels),
        other => Err(format!(
            "--sr {other} not supported (allowed: 44100, 48000, 88200, 96000, \
             176400, 192000)"
        )),
    }
}

/// Rate-monomorphic body. `R: SampleTime` plumbs all the way down
/// into `PhaseSource<R>` / `Machine<R>` / `CallbackState<R>` so the
/// audio callback never branches on rate at runtime. The
/// `Send + 'static` bound is what cpal's `data_callback` requires
/// of the moved closure.
fn run_with_rate<R: SampleTime + Send + 'static>(
    args: &RunArgs,
    bpm: Tempo,
    channels: Vec<Channel>,
) -> Result<(), String> {
    // Choose first dev=midi spec's port name (or "default") as the
    // single MIDI sink. Plan 14 v0.1 supports one port shared
    // across channels; multi-port routing is v0.2 (§Deferred).
    let midi_port_request = args
        .ch
        .iter()
        .find_map(|s| {
            // Re-parse to read `dev` / `out`. Cheap — already
            // validated in `run`. None on parse failure since the
            // earlier pass would have caught it.
            ChannelSpec::parse(s).ok().and_then(|spec| {
                if matches!(spec.dev, agogo_core::machine::ChannelDev::Midi) {
                    Some(spec.out.unwrap_or_else(|| "default".to_string()))
                } else {
                    None
                }
            })
        })
        .ok_or_else(|| {
            "no `dev=midi` channels among --ch specs (v0.1 only routes MIDI; \
             dev=audio is reserved for v0.4)"
                .to_string()
        })?;

    let midi_port_name = if midi_port_request == "default" {
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
        midi_port_request.clone()
    };
    let sink = Arc::new(
        MidirSink::open(&midi_port_name).map_err(|e| format!("midi open `{midi_port_name}`: {e}"))?,
    );

    // SPSC + drain thread.
    let (producer, consumer) = spsc(1024);
    let dropped_handle = producer.dropped_handle();
    let drain_sink: Arc<dyn agogo_core::out::midi::MidiSink + Send + Sync> = sink;
    let drain = consumer.spawn_drain(drain_sink);

    // Build PhaseSource per --source. Link case mints a
    // LinkSessionHandle for the control thread.
    let (phase_source, link_handle): (PhaseSource<R>, Option<LinkSessionHandle>) =
        match args.source.as_str() {
            "internal" => (PhaseSource::Internal { bpm }, None),
            "external" => {
                let detector = PeakDetector::<R>::new(DetectorConfig {
                    threshold_q15: 16_384, // 0.5 in Q0.15
                    hold_samples: args.sr / 4,
                });
                let pll = Pll::<R>::new(PllSettings::DEFAULT, bpm, PULSE_PPQ);
                (PhaseSource::External { detector, pll }, None)
            }
            "link" => {
                // Construct a LinkSession with the user's quantum +
                // start-stop config. The host-time anchor sits at
                // sample 0 / current Link host clock — Plan 09's
                // static-anchor pattern; Plan 09's deferred work
                // upgrades to per-buffer atomic.
                let sr_nz = NonZeroU32::new(args.sr)
                    .ok_or_else(|| "--sr 0 is invalid".to_string())?;
                let anchor = HostTimeAnchor {
                    host_origin_micros: 0,
                    sample_rate: sr_nz,
                };
                let quantum_beats = args.link_quantum.unwrap_or(4.0); // argv boundary
                if !quantum_beats.is_finite() || quantum_beats <= 0.0 {
                    return Err(format!(
                        "--link-quantum {} invalid (must be finite, > 0)",
                        quantum_beats
                    ));
                }
                let config = LinkWriteConfig {
                    enable_start_stop_sync: args.link_enable_start_stop,
                    enable_start_stop: args.link_enable_start_stop,
                    default_quantum: agogo_core::fxp::f64_beats_to_quantum(quantum_beats),
                    push_tempo_on_change: true,
                };
                let session = LinkSession::new(bpm, anchor, config);
                let (lps, handle) = LinkPhaseSource::new(session);
                (PhaseSource::Custom(Box::new(lps)), Some(handle))
            }
            other => {
                return Err(format!(
                    "--source {other} unknown (try `internal`, `external`, or `link`)"
                ));
            }
        };

    // TransportPolicy — link source consults LinkSession; otherwise
    // emit Start at first buffer, Stop on Ctrl-C.
    let transport = if let Some(h) = &link_handle {
        let h_for_policy = h.clone();
        TransportPolicy::LinkDriven {
            prev_playing: false,
            query: Box::new(move || h_for_policy.is_playing()),
        }
    } else {
        TransportPolicy::Internal {
            start_emitted: false,
        }
    };
    let machine = Machine::<R>::new(
        channels,
        phase_source,
        args.sr,
        bpm,
        PPQN,
        transport,
        args.buffer_frames as usize,
    );
    let stop_handle = machine.stop_handle();
    let mut state = CallbackState::<R> { machine, producer };

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
    let cb = Box::new(move |io: &mut AudioIo| {
        state.on_buffer(io);
    });
    let stream_handle = host.run(cfg, cb).map_err(|e| format!("cpal run: {e}"))?;

    eprintln!(
        "agogo run: --bpm {} --sr {} --source {} --audio-in {} (midi port: {}) ({} channel{}{})",
        args.bpm,
        args.sr,
        args.source,
        args.audio_in,
        midi_port_name,
        args.ch.len(),
        if args.ch.len() == 1 { "" } else { "s" },
        if args.max_duration_ms.is_some() {
            ", --max-duration-ms"
        } else {
            ", Ctrl-C to stop"
        },
    );

    // Install Ctrl-C handler. The handler flips the stop flag and
    // signals Machine to emit Stop on the next buffer.
    let stop_flag = Arc::new(AtomicBool::new(false));
    install_ctrlc_handler(stop_flag.clone(), stop_handle.clone(), link_handle.clone())?;

    // Park the main thread until Ctrl-C or --max-duration-ms expires.
    let start = Instant::now();
    let max = args.max_duration_ms.map(u64::from).map(Duration::from_millis);
    while !stop_flag.load(Ordering::Acquire) {
        std::thread::park_timeout(Duration::from_millis(100));
        if let Some(h) = &link_handle {
            h.poll_transport();
        }
        if let Some(d) = max {
            if start.elapsed() >= d {
                stop_handle.request_stop();
                if let Some(h) = &link_handle {
                    h.user_stop();
                }
                stop_flag.store(true, Ordering::Release);
                break;
            }
        }
    }

    // Give the Machine one more buffer-tick to emit the Stop byte
    // before tearing down the stream. ~50 ms covers the worst-case
    // cpal buffer + the drain thread's 1 ms loop.
    std::thread::sleep(Duration::from_millis(50));

    drop(stream_handle); // pause cpal stream
    drop(drain); // flush ring + join drain thread

    let dropped = dropped_handle.load(Ordering::Acquire);
    if dropped > 0 {
        eprintln!(
            "warning: {} MIDI message(s) dropped (SPSC overrun); investigate \
             ring capacity",
            dropped
        );
    }
    eprintln!("agogo run: clean exit");
    Ok(())
}

fn install_ctrlc_handler(
    stop_flag: Arc<AtomicBool>,
    stop_handle: MachineStopHandle,
    link_handle: Option<LinkSessionHandle>,
) -> Result<(), String> {
    ctrlc::set_handler(move || {
        stop_handle.request_stop();
        if let Some(h) = &link_handle {
            h.user_stop();
        }
        stop_flag.store(true, Ordering::Release);
    })
    .map_err(|e| format!("ctrlc handler: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args_with(ch: Vec<&str>, sr: u32) -> RunArgs {
        RunArgs {
            bpm: 120.0,
            sr,
            buffer_frames: 1024,
            source: "internal".into(),
            audio_in: "default".into(),
            ch: ch.into_iter().map(|s| s.to_string()).collect(),
            link_quantum: None,
            link_enable_start_stop: false,
            max_duration_ms: Some(50),
        }
    }

    /// Plan 14 spot-check: empty `--ch` list errors before any
    /// device opens.
    #[test]
    fn run_rejects_empty_ch_list() {
        let args = args_with(vec![], 48_000);
        let err = run(&args).unwrap_err();
        assert!(
            err.contains("at least one --ch"),
            "expected --ch hint, got: {err}"
        );
    }

    /// Plan 14 spot-check: `dev=audio` is reserved for v0.4 and
    /// rejected at parse time.
    #[test]
    fn run_rejects_dev_audio() {
        let args = args_with(vec!["dev=audio,div=t32t"], 48_000);
        let err = run(&args).unwrap_err();
        assert!(
            err.contains("dev=audio") && err.contains("v0.4"),
            "expected dev=audio v0.4 message, got: {err}"
        );
    }

    /// Plan 14 spot-check: rates outside the SampleTime allowlist
    /// error before any device opens, with the allowlist enumerated.
    #[test]
    fn run_rejects_unsupported_rate() {
        let args = args_with(vec!["dev=midi,div=t32t,out=default"], 22_050);
        let err = run(&args).unwrap_err();
        assert!(
            err.contains("22050") && err.contains("44100"),
            "expected rate-allowlist message, got: {err}"
        );
    }

    /// Plan 14 spot-check: a malformed `--ch` spec errors at parse
    /// time with the offending key.
    #[test]
    fn run_surfaces_channel_spec_parse_errors() {
        let args = args_with(vec!["dev=midi,div=t32t,unknown=x"], 48_000);
        let err = run(&args).unwrap_err();
        assert!(
            err.contains("unknown") && err.contains("--ch"),
            "expected --ch parse error with key name, got: {err}"
        );
    }
}

