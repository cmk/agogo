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
    /// Ableton Link integration utilities.
    #[cfg(feature = "link")]
    #[bpaf(command("link"))]
    Link {
        #[bpaf(external(link_sub))]
        sub: LinkSub,
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
}

#[derive(Debug, Clone, Bpaf)]
enum SyncSub {
    /// Synthesise a pulse train and trace the detector + PLL output as CSV.
    ///
    /// One row per detected peak: `sample_index,bpm_estimate,phase_estimate`.
    #[bpaf(command("trace"))]
    Trace {
        #[bpaf(long, argument("BPM"), parse(parse_positive_f32))]
        bpm: f32,
        #[bpaf(long, argument("SR"), parse(parse_positive_u32))]
        sr: u32,
        #[bpaf(long, argument("PPQ"), parse(parse_positive_u32))]
        ppq: u32,
        #[bpaf(long, argument("JITTER_US"), parse(parse_non_negative_f32), fallback(0.0))]
        jitter_us: f32,
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
        #[bpaf(long, argument("SHIFT_MS"), parse(parse_non_negative_f32), fallback(0.0))]
        shift_ms: f32,
        /// Signed calibration offset in ms. Must be finite.
        #[bpaf(long, argument("OFFSET_MS"), parse(parse_finite_f32), fallback(0.0))]
        offset_ms: f32,
        /// Audio buffer length in samples.
        #[bpaf(long, argument("FRAMES"))]
        frames: usize,
        /// Number of consecutive buffers to schedule.
        #[bpaf(long, argument("BUFFERS"), parse(parse_positive_u32))]
        buffers: u32,
    },
}

fn parse_positive_f32(v: f32) -> Result<f32, String> {
    if v.is_finite() && v > 0.0 {
        Ok(v)
    } else {
        Err(format!("must be a positive finite number, got {v}"))
    }
}

fn parse_non_negative_f32(v: f32) -> Result<f32, String> {
    if v.is_finite() && v >= 0.0 {
        Ok(v)
    } else {
        Err(format!("must be a non-negative finite number, got {v}"))
    }
}

fn parse_finite_f32(v: f32) -> Result<f32, String> {
    if v.is_finite() {
        Ok(v)
    } else {
        Err(format!("must be a finite number, got {v}"))
    }
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
        Some(Command::Time {
            op: TimeOp::Schedule(args),
        }) => {
            #[cfg(feature = "core")]
            {
                // Header to stderr (stdout reserved for the schedule
                // itself). `bpm` is informational per the plan;
                // emitting it here makes the input visible.
                eprintln!(
                    "# schedule: {} bars @ {:.1} BPM, tbase={}, swing={:.3}",
                    args.bars, args.bpm, args.tbase, args.swing
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
                println!(
                    "{},{},{:.4},{:.6}",
                    row.t_ms, row.peers, row.tempo_bpm, row.phase
                );
            });
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
        pub tempo_bpm: f64,
        /// Beat-phase in `[0, 1)` at sample `t_ms × sr / 1000`,
        /// mapped through the anchor captured at probe start.
        pub phase: f64,
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
        let mut clock = LinkClock::new(
            initial_bpm,
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
                tempo_bpm: clock.tempo(),
                phase: f64::from(phase_u32) / (1u64 << 32) as f64,
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
            assert!(
                (first.tempo_bpm - 125.0).abs() < 1e-9,
                "tempo {} differs from initial 125.0",
                first.tempo_bpm
            );
            assert!(
                (0.0..1.0).contains(&first.phase),
                "phase {} not in [0, 1)",
                first.phase
            );
        }
    }
}

#[cfg(feature = "core")]
mod sync_trace {
    use agogo_core::arb::pulse_train;
    use agogo_core::fxp::{
        Tempo, Pico, S48, SampleRate, SampleTime, f32_bpm_to_tempo,
        f32_jitter_us_to_sigma,
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
        bpm_f32: f32,
        ppq: u32,
        jitter_us: f32,
        pulses: u32,
        seed: u64,
    ) -> Vec<TraceRow> {
        // argv-boundary conversions.
        let bpm: Tempo = f32_bpm_to_tempo(bpm_f32);
        let jitter: Pico = f32_jitter_us_to_sigma(jitter_us);

        let (samples, _truth): (Vec<f32>, Vec<S48>) =
            pulse_train::<S48>(bpm, ppq, jitter, pulses, seed);
        let pulse_rate_hz = (bpm.0 as f64 / 1.0e6) * ppq as f64 / 60.0;
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
    use agogo_core::fxp::Tempo;
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
        pub shift_ms: f32,
        pub offset_ms: f32,
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
        let stc = SampleTickConn::new(args.sr, bpm, PPQN);
        let channel = Channel {
            mode: ChannelMode::MidiClock,
            divider,
            shuffle: SwingConfig {
                amount: args.shuffle,
                multiplier: 1,
            },
            shift_ms: args.shift_ms,
            offset_ms: args.offset_ms,
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

pub mod time_sched {
    use agogo_core::time::swing::{self, SwingConfig};
    use agogo_core::time::tbase::TBase;
    use agogo_core::time::tick::Tick;
    use bpaf::Bpaf;

    #[derive(Bpaf, Debug, Clone)]
    pub struct ScheduleArgs {
        /// Tempo in beats per minute. Informational only — scheduling
        /// happens in tick space, tempo-independently.
        #[bpaf(long, argument("BPM"))]
        pub bpm: f32,

        /// Grid resolution (e.g. `t16`, `t8t`, `t128t`).
        #[bpaf(long, argument::<String>("TBASE"), parse(parse_tbase))]
        pub tbase: TBase,

        /// Swing ratio in `[0.5, 0.75]`: 0.5 = straight, 0.75 = full
        /// triplet swing.
        #[bpaf(long, argument("SWING"), fallback(0.5))]
        pub swing: f32,

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
    pub fn swing_to_config(swing: f32) -> SwingConfig {
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
            bpm: 120.0,
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
            bpm: 120.0,
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
            bpm: 120.0,
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
            bpm: 120.0,
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
