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
//!
//! ## `agogo_host_link::*` scoping rule (Plan 09 T3 audit)
//!
//! The `agogo-cli` crate is meant to stay buildable without the
//! `link` feature (`cargo build -p agogo-cli --no-default-features
//! --features core,cpal,midi` is the regression-pinned invariant —
//! see `.github/workflows/ci.yml` `cli-no-link` job). Module-scope
//! `agogo_host_link::*` imports in this file are OK because the
//! whole module sits behind `cfg(feature = "run")` and `run` requires
//! `link`.
//!
//! In-function uses **must remain inside the `Source::Link` branch**
//! of `run_with_rate`'s match (see `LinkSession::new(...)` site
//! below). Constructing a Link type in the `internal` or `external`
//! source arms would break the architectural separation: even though
//! the import compiles fine here, it advertises a Link dependency to
//! readers that internal/external mode emphatically does not have.
//! Plan 09's T1 helper (`agogo_host_link::apply_snap_offsets`) lives
//! in host-link itself for the same reason — fewer Link call sites
//! in cli, not more.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use agogo_core::channel::Channel;
use agogo_core::conn::boundary::tempo_to_f64_bpm;
use agogo_core::conn::sample::{S044, S048, S088, S096, S176, S192, SampleRate, SampleTime};
use agogo_core::conn::tempo::Tempo;
use agogo_core::control::sync::{DetectorConfig, PeakDetector, PhaseSource, Pll, PllSettings};
use agogo_core::control::{Machine, MachineStopHandle, TransportPolicy};
use agogo_core::sink::audio::{AudioHost, AudioIo, Config};
use agogo_core::time::tick::PPQN;
use agogo_host_cpal::CpalHost;
use agogo_host_cpal::cpal::callback::CallbackState;
use agogo_host_cpal::cpal::control::spsc;
use agogo_host_link::{
    HostTimeAnchor, LinkPhaseSource, LinkSession, LinkSessionHandle, LinkWriteConfig, Quantum,
};
use agogo_host_midi::MidirSink;
use bpaf::Bpaf;
use std::num::NonZeroU32;

/// PLL pulse rate. `agogo run` external source feeds the detector +
/// PLL at MIDI clock cadence (24 PPQ); the master tick stream
/// scheduler uses [`PPQN`] (960).
const PULSE_PPQ: u32 = 24;

use crate::{parse_bpm_to_tempo, parse_positive_u32, parse_quantum_from_beats};

/// Argv container for `agogo run`. Used by both the bpaf derive and
/// the dispatcher in `main.rs`. The two formerly-`f64` fields
/// (`bpm`, `link_quantum`) now land as typed `Tempo` / `Quantum`
/// directly — the f64 surface area collapses to the bodies of
/// `parse_bpm_to_tempo` and `parse_quantum_from_beats` (in `main.rs`).
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
    let named = match agogo_core::channel::spec::parse_channels(&args.ch) {
        Ok(named) => named,
        Err(e) => {
            let failing_entry = (0..args.ch.len()).find_map(|idx| {
                agogo_core::channel::spec::parse_channels(&args.ch[..=idx])
                    .err()
                    .map(|_| (idx, args.ch[idx].as_str()))
            });

            match failing_entry {
                Some((idx, spec)) => return Err(format!("--ch[{idx}] `{spec}`: {e}")),
                None => return Err(format!("--ch: {e}")),
            }
        }
    };

    // Extract the first MIDI port name before consuming specs.
    // Audit P4 (Plan 22): every spec is implicitly MIDI-targeted —
    // `dev=audio` is rejected at parse time, so by here the only
    // routing target is MIDI. The pre-P4 dev-filter collapses to
    // "first spec's `out`." Returns `Err` if `named` is somehow empty
    // (the `args.ch.is_empty()` guard at line 95 makes this
    // structurally unreachable today, but `?` keeps the function
    // graceful if a future caller path bypasses that guard).
    let midi_port_request = named
        .first()
        .ok_or_else(|| "at least one --ch spec is required".to_string())
        .map(|(_, spec)| spec.out.clone().unwrap_or_else(|| "default".to_string()))?;

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
        rate if rate == S044::HZ => {
            run_with_rate::<S044>(args, bpm, specs, channels, midi_port_request)
        }
        rate if rate == S048::HZ => {
            run_with_rate::<S048>(args, bpm, specs, channels, midi_port_request)
        }
        rate if rate == S088::HZ => {
            run_with_rate::<S088>(args, bpm, specs, channels, midi_port_request)
        }
        rate if rate == S096::HZ => {
            run_with_rate::<S096>(args, bpm, specs, channels, midi_port_request)
        }
        rate if rate == S176::HZ => {
            run_with_rate::<S176>(args, bpm, specs, channels, midi_port_request)
        }
        rate if rate == S192::HZ => {
            run_with_rate::<S192>(args, bpm, specs, channels, midi_port_request)
        }
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
    specs: Vec<agogo_core::channel::spec::ChannelSpec>,
    mut channels: Vec<Channel>,
    midi_port_request: String,
) -> Result<(), String> {
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
        MidirSink::open(&midi_port_name)
            .map_err(|e| format!("midi open `{midi_port_name}`: {e}"))?,
    );

    // SPSC + drain thread.
    let (producer, consumer) = spsc(1024);
    let dropped_handle = producer.dropped_handle();
    let drain_sink: Arc<dyn agogo_core::sink::midi::MidiSink + Send + Sync> = sink;
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
                agogo_host_link::apply_snap_offsets(&specs, &mut session, &mut channels);
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
        tempo_to_f64_bpm(args.bpm),
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
            let in_range = parsed.is_finite() && parsed > 0.0 && parsed <= agogo_core::conn::boundary::MAX_BPM_F64;
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
        assert_eq!(got, agogo_host_link::f64_beats_to_quantum(4.5));
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
        let args = args_with(vec!["dev=audio,grid=t32t"], 48_000);
        let err = run(&args).unwrap_err();
        assert!(
            err.contains("dev=audio") && err.contains("v0.4"),
            "expected dev=audio v0.4 message, got: {err}"
        );
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
        // SampleTime allowlist, not from spec/dev rejection.
        assert!(
            err.contains("22050"),
            "expected rate-dispatch error indicating parse cleared, got: {err}"
        );
        assert!(
            !err.contains("dev=") && !err.contains("MissingKey"),
            "expected no dev/parse error, got: {err}"
        );
    }

    /// Plan 14 spot-check: rates outside the SampleTime allowlist
    /// error before any device opens, with the allowlist enumerated.
    #[test]
    fn run_rejects_unsupported_rate() {
        let args = args_with(vec!["dev=midi,grid=t32t,out=default"], 22_050);
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
