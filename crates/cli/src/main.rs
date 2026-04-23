#![forbid(unsafe_code)]

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "agogo", about = "agogo workspace CLI")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Audio-clock sync utilities.
    Sync {
        #[command(subcommand)]
        sub: SyncSub,
    },
    /// Musical-time operations (Cirklon grid algebra).
    Time {
        #[command(subcommand)]
        op: TimeOp,
    },
}

#[derive(Subcommand)]
enum SyncSub {
    /// Synthesise a pulse train and trace the detector + PLL output as CSV.
    ///
    /// One row per detected peak: `sample_index,bpm_estimate,phase_estimate`.
    Trace {
        #[arg(long, value_parser = parse_positive_f32)]
        bpm: f32,
        #[arg(long, value_parser = clap::value_parser!(u32).range(1..))]
        sr: u32,
        #[arg(long, value_parser = clap::value_parser!(u32).range(1..))]
        ppq: u32,
        #[arg(long, default_value_t = 0.0, value_parser = parse_non_negative_f32)]
        jitter_us: f32,
        #[arg(long, value_parser = clap::value_parser!(u32).range(1..))]
        pulses: u32,
        #[arg(long, default_value_t = 1)]
        seed: u64,
    },
}

#[derive(Subcommand)]
enum TimeOp {
    /// Print absolute tick positions for a schedule at a given TBase.
    /// On off-beat 16th-note steps the swing shift (if any) is
    /// applied before printing, so odd steps come out earlier than
    /// their nominal grid position.
    Schedule(time_sched::ScheduleArgs),
}

fn parse_positive_f32(s: &str) -> Result<f32, String> {
    let v: f32 = s.parse().map_err(|e| format!("not a number: {e}"))?;
    if v.is_finite() && v > 0.0 {
        Ok(v)
    } else {
        Err(format!("must be a positive finite number, got {v}"))
    }
}

fn parse_non_negative_f32(s: &str) -> Result<f32, String> {
    let v: f32 = s.parse().map_err(|e| format!("not a number: {e}"))?;
    if v.is_finite() && v >= 0.0 {
        Ok(v)
    } else {
        Err(format!("must be a non-negative finite number, got {v}"))
    }
}

fn main() {
    let cli = Cli::parse();
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
                let rows = sync_trace::trace(bpm, sr, ppq, jitter_us, pulses, seed);
                println!("sample_index,bpm_estimate,phase_estimate");
                for r in rows {
                    println!("{:.4},{:.6},{:.6}", r.sample_index, r.bpm, r.phase);
                }
            }
            #[cfg(not(feature = "core"))]
            {
                let _ = (bpm, sr, ppq, jitter_us, pulses, seed);
                eprintln!("error: build with --features core to enable `sync trace`");
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
        None => {
            #[cfg(feature = "core")]
            let tag = "with core";
            #[cfg(not(feature = "core"))]
            let tag = "core disabled";
            println!("agogo-cli ({tag})");
        }
    }
}

#[cfg(feature = "core")]
mod sync_trace {
    use agogo_core::arb::pulse_train;
    use agogo_core::sync::{DetectorConfig, PeakDetector, Pll, PllSettings};

    #[derive(Debug, Clone, Copy)]
    pub struct TraceRow {
        pub sample_index: f64,
        pub bpm: f32,
        pub phase: f32,
    }

    pub fn trace(
        bpm: f32,
        sr: u32,
        ppq: u32,
        jitter_us: f32,
        pulses: u32,
        seed: u64,
    ) -> Vec<TraceRow> {
        let (samples, _truth) = pulse_train(bpm, sr, ppq, jitter_us, pulses, seed);
        let pulse_rate_hz = bpm as f64 * ppq as f64 / 60.0;
        let spacing_samples = (sr as f64 / pulse_rate_hz) as u32;
        let mut detector = PeakDetector::new(DetectorConfig {
            threshold: 0.5,
            hold_samples: spacing_samples / 2,
        });
        let mut pll = Pll::new(PllSettings::DEFAULT, bpm, sr, ppq);
        let peaks = detector.process(&samples, 0);
        peaks
            .into_iter()
            .map(|p| {
                let out = pll.step(Some(p.sample_index));
                TraceRow {
                    sample_index: p.sample_index,
                    bpm: out.bpm,
                    phase: out.phase,
                }
            })
            .collect()
    }
}

pub mod time_sched {
    use agogo_core::time::swing::{self, SwingConfig};
    use agogo_core::time::tbase::TBase;
    use agogo_core::time::tick::Tick;
    use clap::Args;

    #[derive(Args, Debug, Clone)]
    pub struct ScheduleArgs {
        /// Tempo in beats per minute. Informational only — scheduling
        /// happens in tick space, tempo-independently.
        #[arg(long)]
        pub bpm: f32,

        /// Grid resolution (e.g. `t16`, `t8t`, `t128t`).
        #[arg(long, value_parser = str::parse::<TBase>)]
        pub tbase: TBase,

        /// Swing ratio in `[0.5, 0.75]`: 0.5 = straight, 0.75 = full
        /// triplet swing.
        #[arg(long, default_value_t = 0.5)]
        pub swing: f32,

        /// Number of 4/4 bars to schedule. Bounded to `u16` (≤ 65535)
        /// so memory and stdout stay reasonable — 65535 × 192 ≈ 12.6M
        /// tick offsets ≈ 50 MB Vec at the finest grid.
        #[arg(long)]
        pub bars: u16,
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
    use super::sync_trace::trace;
    use super::time_sched::{schedule_ticks, swing_to_config, ScheduleArgs};
    use agogo_core::time::swing::SwingConfig;
    use agogo_core::time::tbase::TBase;

    /// E2E gate from the plan's build gates: 256 pulses at 120 BPM /
    /// 48 kHz / 24 PPQ with 50 µs jitter must converge to within
    /// ±0.05 BPM of 120.0 by the end of the trace.
    #[test]
    fn sync_trace_converges() {
        let rows = trace(120.0, 48_000, 24, 50.0, 256, 1);
        assert_eq!(rows.len(), 256);
        let last = rows.last().unwrap();
        assert!(
            (last.bpm - 120.0).abs() < 0.05,
            "final bpm {} not within ±0.05 of 120",
            last.bpm
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
    fn schedule_ticks_two_bars_t16_yields_32_offsets() {
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
