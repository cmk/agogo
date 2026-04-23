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

#[cfg(all(test, feature = "core"))]
mod tests {
    use super::sync_trace::trace;

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
}
