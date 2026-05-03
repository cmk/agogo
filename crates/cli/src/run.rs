//! `agogo run` — end-to-end runner.
//!
//! Generalises the single-channel `agogo demo run` into:
//!   - N channels via the docker-style repeatable `--ch` flag;
//!   - All six typed sample rates via a static `match args.sr`;
//!   - Three sources: `internal | external | link`. `link` plugs
//!     `LinkSession` in via `PhaseSource::Custom`;
//!   - Ctrl-C handling via the `ctrlc` crate;
//!   - `MidiRtByte::Start` at first buffer / `Stop` on Ctrl-C
//!     teardown for `internal` and `external` sources;
//!     transition-driven Start/Stop for `link`.
//!
//! Feature-gated on `run` (= `demo + link + ctrlc`). Compiled in by
//! `cargo build -p agogo-cli --features run`.
//!
//! ## `agogo::host::link::*` scoping rule (Plan 09 T3 audit)
//!
//! The `agogo-cli` crate is meant to stay buildable without the
//! `link` feature (`cargo build -p agogo-cli --no-default-features
//! --features core,cpal,midi` is the regression-pinned invariant —
//! see `.github/workflows/ci.yml` `cli-no-link` job). Module-scope
//! `agogo::host::link::*` imports in this file are OK because the
//! whole module sits behind `cfg(feature = "run")` and `run` requires
//! `link`.
//!
//! In-function uses **must remain inside the `Source::Link` branch**
//! of `run_with_rate`'s match (see `LinkSession::new(...)` site
//! below). Constructing a Link type in the `internal` or `external`
//! source arms would break the architectural separation: even though
//! the import compiles fine here, it advertises a Link dependency to
//! readers that internal/external mode emphatically does not have.
//! Plan 09's T1 helper (`agogo::host::link::apply_snap_offsets`) lives
//! in host-link itself for the same reason — fewer Link call sites
//! in cli, not more.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use agogo::core::channel::Channel;
use agogo::core::channel::spec::ChannelSpecRole;
use agogo::core::conn::boundary::tempo_to_f64_bpm;
use agogo::core::conn::sample::{S044, S048, S088, S096, S176, S192, SampleRate};
use agogo::core::conn::tempo::Tempo;
use agogo::core::control::sync::{DetectorConfig, PeakDetector, PhaseSource, Pll, PllSettings};
use agogo::core::control::{Playhead, PlayheadStopHandle, TransportPolicy};
use agogo::core::sink::audio::{AudioHost, AudioIo, Config};
use agogo::host::cpal::CpalHost;
use agogo::host::cpal::callback::CallbackState;
use agogo::host::cpal::control::spsc;
use agogo::host::link::{
    HostTimeAnchor, LinkPhaseSource, LinkSession, LinkSessionHandle, LinkWriteConfig, Quantum,
};
use agogo::host::midi::MidirSink;
use bpaf::Bpaf;
use std::num::NonZeroU32;

/// PLL pulse rate. `agogo run` external source feeds the detector +
/// PLL at MIDI clock cadence (24 PPQ); the master tick stream
/// scheduler uses `agogo::core::time::tick::PPQN` (960).
const PULSE_PPQ: u32 = 24;

use crate::parsers::{parse_bpm_to_tempo, parse_positive_u32, parse_quantum_from_beats};

/// Argv container for `agogo run`. Used by both the bpaf derive and
/// the dispatcher in `main.rs`. The two formerly-`f64` fields
/// (`bpm`, `link_quantum`) now land as typed `Tempo` / `Quantum`
/// directly — the f64 surface area collapses to the bodies of
/// `parse_bpm_to_tempo` and `parse_quantum_from_beats` (in `parsers.rs`).
#[derive(Debug, Clone, Bpaf)]
pub struct RunArgs {
    /// Tempo in beats per minute. Applies to all channels.
    #[bpaf(long, argument::<String>("BPM"), parse(parse_bpm_to_tempo))]
    pub bpm: Tempo,
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
    /// Format: `key=val[,key=val]*`. Required keys: `grid`, `dev`.
    /// Optional: `id`, `out`, `delay` (latency compensation in ms),
    /// `snap-quantum-us`. Quote values containing spaces or commas:
    /// `--ch "out=IAC Bus 1,grid=t32t,dev=midi"`.
    #[bpaf(long, argument("SPEC"), many)]
    pub ch: Vec<String>,
    /// Link quantum in beats. Required when `--source link`;
    /// ignored otherwise. Default 4 (one bar of 4/4).
    #[bpaf(long, argument::<String>("BEATS"), parse(parse_quantum_from_beats), optional)]
    pub link_quantum: Option<Quantum>,
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
                    dev=midi,grid=t32t,out=default`)"
            .to_string());
    }

    // `args.bpm` is already `Tempo` — bpaf's `parse_bpm_to_tempo`
    // consumed the f64 at parse time.
    let bpm: Tempo = args.bpm;

    // Parse all --ch specs eagerly (in order, so variable refs
    // resolve) before any device opens.
    let named = match agogo::core::channel::spec::parse_channels(&args.ch) {
        Ok(named) => named,
        Err(e) => {
            let failing_entry = (0..args.ch.len()).find_map(|idx| {
                agogo::core::channel::spec::parse_channels(&args.ch[..=idx])
                    .err()
                    .map(|_| (idx, args.ch[idx].as_str()))
            });

            match failing_entry {
                Some((idx, spec)) => return Err(format!("--ch[{idx}] `{spec}`: {e}")),
                None => return Err(format!("--ch: {e}")),
            }
        }
    };

    // Extract target-specific device requests before consuming specs.
    let midi_port_request = single_target_output_request(
        &named,
        |role| matches!(role, ChannelSpecRole::Midi(_)),
        "MIDI",
    )?;
    let audio_output_request = single_target_output_request(
        &named,
        |role| matches!(role, ChannelSpecRole::Audio(_)),
        "audio",
    )?;

    // Keep specs alongside channels so the link branch in
    // `run_with_rate` can call `apply_snap_offsets` after constructing
    // the `LinkSession`. Plan 2026-04-28-09 T1.
    let (specs, channel_results): (Vec<_>, Vec<_>) = named
        .into_iter()
        .map(|(id, spec)| {
            let channel = spec
                .clone()
                .into_channel()
                .map_err(|e| format!("--ch {id}: {e}"));
            (spec, channel)
        })
        .unzip();
    let channels: Vec<Channel> = channel_results.into_iter().collect::<Result<_, _>>()?;

    // Static rate dispatch.
    match args.sr {
        rate if rate == S044::HZ => run_s044(
            args,
            bpm,
            specs,
            channels,
            midi_port_request,
            audio_output_request,
        ),
        rate if rate == S048::HZ => run_s048(
            args,
            bpm,
            specs,
            channels,
            midi_port_request,
            audio_output_request,
        ),
        rate if rate == S088::HZ => run_s088(
            args,
            bpm,
            specs,
            channels,
            midi_port_request,
            audio_output_request,
        ),
        rate if rate == S096::HZ => run_s096(
            args,
            bpm,
            specs,
            channels,
            midi_port_request,
            audio_output_request,
        ),
        rate if rate == S176::HZ => run_s176(
            args,
            bpm,
            specs,
            channels,
            midi_port_request,
            audio_output_request,
        ),
        rate if rate == S192::HZ => run_s192(
            args,
            bpm,
            specs,
            channels,
            midi_port_request,
            audio_output_request,
        ),
        other => Err(format!(
            "--sr {other} not supported (allowed: 44100, 48000, 88200, 96000, \
             176400, 192000)"
        )),
    }
}

/// Channel-role summary used to select the required runtime sinks.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct ChannelMix {
    has_midi: bool,
    has_audio: bool,
}

fn channel_mix(channels: &[Channel]) -> ChannelMix {
    ChannelMix {
        has_midi: channels.iter().any(|ch| matches!(ch, Channel::Midi { .. })),
        has_audio: channels
            .iter()
            .any(|ch| matches!(ch, Channel::Audio { .. })),
    }
}

fn single_target_output_request(
    named: &[(String, agogo::core::channel::spec::ChannelSpec)],
    mut matches_target: impl FnMut(&ChannelSpecRole) -> bool,
    target_name: &str,
) -> Result<Option<String>, String> {
    let mut request: Option<String> = None;
    for (id, spec) in named {
        if !matches_target(&spec.role) {
            continue;
        }
        let out = spec.out.clone().unwrap_or_else(|| "default".to_string());
        match &request {
            Some(existing) if existing != &out => {
                return Err(format!(
                    "multiple {target_name} output devices requested: `{existing}` and `{out}` \
                     (channel `{id}`); use one shared out= until multi-device routing exists"
                ));
            }
            Some(_) => {}
            None => request = Some(out),
        }
    }
    Ok(request)
}

fn config_device_name(request: &str) -> Option<String> {
    (request != "default").then(|| request.to_string())
}

macro_rules! def_run_with_rate {
    ($func:ident, $Rate:ty) => {
        fn $func(
            args: &RunArgs,
            bpm: Tempo,
            specs: Vec<agogo::core::channel::spec::ChannelSpec>,
            mut channels: Vec<Channel>,
            midi_port_request: Option<String>,
            audio_output_request: Option<String>,
        ) -> Result<(), String> {
    let mix = channel_mix(&channels);
    debug_assert_eq!(mix.has_midi, midi_port_request.is_some());
    debug_assert_eq!(mix.has_audio, audio_output_request.is_some());
    if args.source == "external" && mix.has_audio {
        return Err(
            "--source external with dev=audio output is not wired yet; use --source internal for \
             the audio metronome test feature"
                .to_string(),
        );
    }

    // SPSC + optional MIDI drain thread.
    let (producer, consumer) = spsc(1024);
    let dropped_handle = producer.dropped_handle();
    let (midi_port_name, drain, undrained_consumer) = if let Some(request) = midi_port_request {
        let midi_port_name = if request == "default" {
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
            request
        };
        let sink = Arc::new(
            MidirSink::open(&midi_port_name)
                .map_err(|e| format!("midi open `{midi_port_name}`: {e}"))?,
        );
        let drain_sink: Arc<dyn agogo::core::sink::midi::MidiSink + Send + Sync> = sink;
        (
            Some(midi_port_name),
            Some(consumer.spawn_drain(drain_sink)),
            None,
        )
    } else {
        (None, None, Some(consumer))
    };

    // Build PhaseSource per --source. Link case mints a
    // LinkSessionHandle for the control thread.
    let (phase_source, link_handle): (PhaseSource<$Rate>, Option<LinkSessionHandle>) =
        match args.source.as_str() {
            "internal" => (PhaseSource::Internal { bpm }, None),
            "external" => {
                let detector = PeakDetector::<$Rate>::new(DetectorConfig {
                    threshold_q15: 16_384, // 0.5 in Q0.15
                    hold_samples: args.sr / 4,
                });
                let pll = Pll::<$Rate>::new(PllSettings::DEFAULT, bpm, PULSE_PPQ);
                (PhaseSource::External { detector, pll }, None)
            }
            "link" => {
                // Construct a LinkSession with the user's quantum +
                // start-stop config. The host-time anchor sits at
                // sample 0 / current Link host clock — Plan 09's
                // static-anchor pattern; Plan 09's deferred work
                // upgrades to per-buffer atomic.
                let sr_nz =
                    NonZeroU32::new(args.sr).ok_or_else(|| "--sr 0 is invalid".to_string())?;
                let anchor = HostTimeAnchor {
                    host_origin_micros: 0,
                    sample_rate: sr_nz,
                };
                // `args.link_quantum` is already `Option<Quantum>` —
                // bpaf's `parse_quantum_from_beats` consumed the f64
                // at parse time. Fallback `Quantum::from_bars(4)` is
                // one bar in 4/4, i.e. 4 beats = 4_000_000 microbeats.
                let default_quantum = args.link_quantum.unwrap_or(Quantum::from_bars(4));
                let config = LinkWriteConfig {
                    enable_start_stop_sync: args.link_enable_start_stop,
                    enable_start_stop: args.link_enable_start_stop,
                    default_quantum,
                    push_tempo_on_change: true,
                };
                let mut session = LinkSession::new(bpm, anchor, config);
                // Plan 2026-04-28-09 T1: fold per-channel
                // `snap-quantum-us=N` intents into each channel's
                // offset before the session is consumed by
                // `LinkPhaseSource::new`. The walk lives in
                // host-link itself (helper imported via the existing
                // `LinkSession` use line) — cli code doesn't grow a
                // new `LinkSession::*` call site for this.
                agogo::host::link::apply_snap_offsets(&specs, &mut session, &mut channels);
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
    let playhead = Playhead::<$Rate>::new(
        channels,
        phase_source,
        args.sr,
        bpm,
        transport,
        args.buffer_frames as usize,
    );
    let stop_handle = playhead.stop_handle();
    let mut state = CallbackState::<$Rate> { playhead, producer };

    // Open audio host. MIDI-only runs preserve the existing input
    // stream timing source; audio-click runs use output-only cpal so
    // `--source internal` works without an audio input device.
    let (host, cfg, audio_device_label) = if mix.has_audio {
        let request = audio_output_request.unwrap_or_else(|| "default".to_string());
        let host = if request == "default" {
            CpalHost::default_output()
        } else {
            CpalHost::with_output_name(&request)
        }
        .map_err(|e| format!("cpal output open: {e}"))?;
        (
            host,
            Config {
                input_device: None,
                output_device: config_device_name(&request),
                sample_rate: args.sr,
                buffer_frames: args.buffer_frames,
                input_channels: 0,
                output_channels: 1,
            },
            format!("audio out: {request}"),
        )
    } else {
        let host = if args.audio_in == "default" {
            CpalHost::default_input()
        } else {
            CpalHost::with_input_name(&args.audio_in)
        }
        .map_err(|e| format!("cpal open: {e}"))?;
        (
            host,
            Config {
                input_device: config_device_name(&args.audio_in),
                output_device: None,
                sample_rate: args.sr,
                buffer_frames: args.buffer_frames,
                input_channels: 1,
                output_channels: 0,
            },
            format!("audio in: {}", args.audio_in),
        )
    };

    // Move state into the data callback.
    let cb = Box::new(move |io: &mut AudioIo| {
        state.on_buffer(io);
    });
    let stream_handle = host.run(cfg, cb).map_err(|e| format!("cpal run: {e}"))?;

    eprintln!(
        "agogo run: --bpm {} --sr {} --source {} ({}) (midi port: {}) ({} channel{}{})",
        tempo_to_f64_bpm(args.bpm),
        args.sr,
        args.source,
        audio_device_label,
        midi_port_name.as_deref().unwrap_or("none"),
        args.ch.len(),
        if args.ch.len() == 1 { "" } else { "s" },
        if args.max_duration_ms.is_some() {
            ", --max-duration-ms"
        } else {
            ", Ctrl-C to stop"
        },
    );

    // Install Ctrl-C handler. The handler flips the stop flag and
    // signals Playhead to emit Stop on the next buffer.
    let stop_flag = Arc::new(AtomicBool::new(false));
    install_ctrlc_handler(stop_flag.clone(), stop_handle.clone(), link_handle.clone())?;

    // Park the main thread until Ctrl-C or --max-duration-ms expires.
    let start = Instant::now();
    let max = args
        .max_duration_ms
        .map(u64::from)
        .map(Duration::from_millis);
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

    // Give the Playhead one more buffer-tick to emit the Stop byte
    // before tearing down the stream. ~50 ms covers the worst-case
    // cpal buffer + the drain thread's 1 ms loop.
    std::thread::sleep(Duration::from_millis(50));

    drop(stream_handle); // pause cpal stream
    drop(drain); // flush ring + join optional MIDI drain thread
    drop(undrained_consumer);

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
    };
}

def_run_with_rate!(run_s044, S044);
def_run_with_rate!(run_s048, S048);
def_run_with_rate!(run_s088, S088);
def_run_with_rate!(run_s096, S096);
def_run_with_rate!(run_s176, S176);
def_run_with_rate!(run_s192, S192);

fn install_ctrlc_handler(
    stop_flag: Arc<AtomicBool>,
    stop_handle: PlayheadStopHandle,
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
    use proptest::prelude::*;

    // ── parse_bpm_to_tempo / parse_quantum_from_beats ────────────
    //
    // CLAUDE.md mandates property tests for parsing/transforming
    // functions. These two bpaf parsers are the only argv-handler
    // sites where f64 enters the pipeline; pin their validation
    // contract directly rather than relying on transitive coverage
    // through downstream Tempo / Quantum proptests.

    #[test]
    fn parse_bpm_to_tempo_accepts_120_exactly() {
        let got = parse_bpm_to_tempo("120".to_string()).unwrap();
        assert_eq!(got, Tempo::from_bpm_integer(120));
    }

    #[test]
    fn parse_bpm_to_tempo_accepts_120_5_decimal() {
        let got = parse_bpm_to_tempo("120.5".to_string()).unwrap();
        assert_eq!(got, Tempo(120_500_000));
    }

    #[test]
    fn parse_bpm_to_tempo_rejects_zero() {
        let err = parse_bpm_to_tempo("0".to_string()).unwrap_err();
        assert!(err.contains("out of range"), "got: {err}");
    }

    #[test]
    fn parse_bpm_to_tempo_rejects_negative() {
        let err = parse_bpm_to_tempo("-5".to_string()).unwrap_err();
        assert!(err.contains("out of range"), "got: {err}");
    }

    #[test]
    fn parse_bpm_to_tempo_rejects_nan() {
        let err = parse_bpm_to_tempo("nan".to_string()).unwrap_err();
        assert!(err.contains("out of range"), "got: {err}");
    }

    #[test]
    fn parse_bpm_to_tempo_rejects_above_max() {
        // u32::MAX µBPM ≈ 4294.967295 BPM. 5000 is comfortably above.
        let err = parse_bpm_to_tempo("5000".to_string()).unwrap_err();
        assert!(err.contains("out of range"), "got: {err}");
    }

    #[test]
    fn parse_bpm_to_tempo_rejects_garbage() {
        let err = parse_bpm_to_tempo("hello".to_string()).unwrap_err();
        assert!(err.contains("not a number"), "got: {err}");
    }

    proptest! {
        /// Validation contract: `parse_bpm_to_tempo` returns Ok iff
        /// the parsed f64 is finite, positive, and ≤ max_bpm. Test
        /// the round-tripped f64 (not `f` directly) because String
        /// formatting can drop NaN payload bits.
        #[test]
        fn parse_bpm_to_tempo_ok_iff_in_range(f in prop::num::f64::ANY) {
            let s = format!("{f}");
            let parsed: f64 = s.parse().unwrap_or(f64::NAN);
            let in_range = parsed.is_finite() && parsed > 0.0 && parsed <= agogo::core::conn::boundary::MAX_BPM_F64;
            prop_assert_eq!(parse_bpm_to_tempo(s).is_ok(), in_range);
        }

        /// Validation contract: `parse_quantum_from_beats` returns
        /// Ok iff the parsed f64 is finite and positive.
        #[test]
        fn parse_quantum_from_beats_ok_iff_in_range(f in prop::num::f64::ANY) {
            let s = format!("{f}");
            let parsed: f64 = s.parse().unwrap_or(f64::NAN);
            let in_range = parsed.is_finite() && parsed > 0.0;
            prop_assert_eq!(parse_quantum_from_beats(s).is_ok(), in_range);
        }
    }

    #[test]
    fn parse_quantum_from_beats_accepts_4() {
        let got = parse_quantum_from_beats("4".to_string()).unwrap();
        assert_eq!(got, Quantum::from_bars(4));
    }

    #[test]
    fn parse_quantum_from_beats_accepts_4_5() {
        let got = parse_quantum_from_beats("4.5".to_string()).unwrap();
        assert_eq!(got, agogo::host::link::f64_beats_to_quantum(4.5));
    }

    #[test]
    fn parse_quantum_from_beats_rejects_zero() {
        let err = parse_quantum_from_beats("0".to_string()).unwrap_err();
        assert!(err.contains("invalid"), "got: {err}");
    }

    #[test]
    fn parse_quantum_from_beats_rejects_negative() {
        let err = parse_quantum_from_beats("-1".to_string()).unwrap_err();
        assert!(err.contains("invalid"), "got: {err}");
    }

    #[test]
    fn parse_quantum_from_beats_rejects_nan() {
        let err = parse_quantum_from_beats("nan".to_string()).unwrap_err();
        assert!(err.contains("invalid"), "got: {err}");
    }

    fn args_with(ch: Vec<&str>, sr: u32) -> RunArgs {
        RunArgs {
            bpm: Tempo::from_bpm_integer(120),
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

    /// Empty `--ch` list errors before any device opens.
    #[test]
    fn run_rejects_empty_ch_list() {
        let args = args_with(vec![], 48_000);
        let err = run(&args).unwrap_err();
        assert!(
            err.contains("at least one --ch"),
            "expected --ch hint, got: {err}"
        );
    }

    /// `dev=audio` now accepts only the generated click test
    /// feature, so a bare audio target still errors before any
    /// device opens.
    #[test]
    fn run_rejects_dev_audio_without_click_mode() {
        let args = args_with(vec!["dev=audio,grid=t32t"], 48_000);
        let err = run(&args).unwrap_err();
        assert!(
            err.contains("mode") && err.contains("click"),
            "expected dev=audio mode=click message, got: {err}"
        );
    }

    #[test]
    fn run_accepts_audio_click_internal_spec() {
        let args = args_with(vec!["dev=audio,mode=click,grid=t4,out=default"], 22_050);
        let err = run(&args).unwrap_err();
        assert!(
            err.contains("22050") && !err.contains("--ch"),
            "expected --sr rate error (audio spec parsed OK), got: {err}"
        );
    }

    #[test]
    fn run_audio_only_does_not_require_midi_port() {
        let named =
            agogo::core::channel::spec::parse_channels(&["dev=audio,mode=click,grid=t4".into()])
                .unwrap();
        let channels: Vec<Channel> = named
            .into_iter()
            .map(|(_, spec)| spec.into_channel().unwrap())
            .collect();

        assert_eq!(
            channel_mix(&channels),
            ChannelMix {
                has_midi: false,
                has_audio: true,
            }
        );
    }

    #[test]
    fn run_rejects_multiple_midi_outputs_until_routing_exists() {
        let args = args_with(
            vec!["dev=midi,grid=t4,out=midi-a", "dev=midi,grid=t8,out=midi-b"],
            48_000,
        );
        let err = run(&args).unwrap_err();
        assert!(
            err.contains("multiple MIDI output devices") && err.contains("midi-a"),
            "expected multi-MIDI-output error, got: {err}"
        );
    }

    #[test]
    fn run_rejects_multiple_audio_outputs_until_routing_exists() {
        let args = args_with(
            vec![
                "dev=audio,mode=click,grid=t4,out=speakers-a",
                "dev=audio,mode=click,grid=t8,out=speakers-b",
            ],
            48_000,
        );
        let err = run(&args).unwrap_err();
        assert!(
            err.contains("multiple audio output devices") && err.contains("speakers-a"),
            "expected multi-audio-output error, got: {err}"
        );
    }

    #[test]
    fn config_device_name_preserves_default_as_none() {
        assert_eq!(config_device_name("default"), None);
        assert_eq!(config_device_name("named"), Some("named".to_string()));
    }

    /// Plan 22 (audit P4): post-field-removal, every spec is
    /// implicitly MIDI-targeted. Verify the minimal MIDI spec
    /// reaches the rate-dispatch gate (the next thing that can
    /// fail in `run`'s pre-flight, intentionally tripped here by
    /// using an unsupported rate so the test doesn't try to open
    /// real hardware). The error originating from rate dispatch
    /// — not from a dev/spec rejection — confirms parse cleared.
    #[test]
    fn run_accepts_minimal_midi_spec_through_to_rate_dispatch() {
        let args = args_with(vec!["dev=midi,grid=t32t,out=default"], 22_050);
        let err = run(&args).unwrap_err();
        // Parse succeeded — the failure must come from the
        // typed sample-rate allowlist, not from spec/dev rejection.
        assert!(
            err.contains("22050"),
            "expected rate-dispatch error indicating parse cleared, got: {err}"
        );
        assert!(
            !err.contains("dev=") && !err.contains("MissingKey"),
            "expected no dev/parse error, got: {err}"
        );
    }

    /// Rates outside the typed sample-rate allowlist error before any
    /// device opens, with the allowlist enumerated.
    #[test]
    fn run_rejects_unsupported_rate() {
        let args = args_with(vec!["dev=midi,grid=t32t,out=default"], 22_050);
        let err = run(&args).unwrap_err();
        assert!(
            err.contains("22050") && err.contains("44100"),
            "expected rate-allowlist message, got: {err}"
        );
    }

    /// A malformed `--ch` spec errors at parse time with the
    /// offending key.
    #[test]
    fn run_surfaces_channel_spec_parse_errors() {
        let args = args_with(vec!["dev=midi,grid=t32t,unknown=x"], 48_000);
        let err = run(&args).unwrap_err();
        assert!(
            err.contains("unknown") && err.contains("--ch"),
            "expected --ch parse error with key name, got: {err}"
        );
    }

    /// Plan 2026-04-25-03 spot-check: a `mode=click` spec passes the
    /// CLI's eager `ChannelSpec` validation. We pin the success at
    /// the parse layer by combining the click spec with an
    /// unsupported `sr=22_050` — `run` rejects the rate *after* the
    /// spec validation step, so seeing "22050" (and the absence of
    /// the "--ch" parse-error prefix) means click parsed cleanly.
    #[test]
    fn run_accepts_mode_click_spec() {
        let args = args_with(
            vec!["dev=midi,mode=click,grid=t4,note=37,vel=80,mch=10,out=default"],
            22_050,
        );
        let err = run(&args).unwrap_err();
        assert!(
            err.contains("22050") && !err.contains("--ch"),
            "expected --sr rate error (spec parsed OK), got: {err}"
        );
    }

    /// Plan 2026-04-25-03 spot-check: the same surface accepts
    /// `bars=N` on a non-T1 divider (the gating restriction was
    /// dropped per design discussion).
    #[test]
    fn run_accepts_bars_on_non_t1_div() {
        let args = args_with(vec!["dev=midi,grid=t8,bars=3,out=default"], 22_050);
        let err = run(&args).unwrap_err();
        assert!(
            err.contains("22050") && !err.contains("--ch"),
            "expected --sr rate error (spec parsed OK), got: {err}"
        );
    }
}
