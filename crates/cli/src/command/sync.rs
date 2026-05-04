//! `agogo sync trace` handler — synthetic-PCM-driven PLL trace.
//!
//! Generates a Hann-bell pulse train at the requested BPM/PPQ with
//! optional Gaussian timing jitter, runs it through the
//! `PeakDetector` + `Pll` pipeline, and emits one CSV row per
//! detected peak. The plan-level convergence gate
//! (`sync_trace_converges`) lives next to the implementation
//! rather than in `main.rs`'s EOF test block (Plan
//! 2026-04-28-05 T1).

use agogo::chan::conn::fixed::Pico;
use agogo::chan::conn::rate::{R048, SampleRate};
use agogo::chan::conn::tempo::Tempo;
use agogo::chan::control::pulse::pulse_train_s048;
use agogo::chan::control::{DetectorConfig, PeakDetector, Pll, PllSettings};
use bpaf::Bpaf;

use crate::parse::{parse_bpm_to_tempo, parse_jitter_us_to_pico, parse_positive_u32};

#[derive(Debug, Clone, Bpaf)]
pub enum SyncSub {
    /// Synthesise a pulse train and trace the detector + PLL output as CSV.
    ///
    /// One row per detected peak: `sample_index,bpm_estimate,phase_estimate`.
    #[bpaf(command("trace"))]
    Trace {
        #[bpaf(long, argument::<String>("BPM"), parse(parse_bpm_to_tempo))]
        bpm: agogo::chan::conn::tempo::Tempo,
        #[bpaf(long, argument("SR"), parse(parse_positive_u32))]
        sr: u32,
        #[bpaf(long, argument("PPQ"), parse(parse_positive_u32))]
        ppq: u32,
        #[bpaf(long, argument::<String>("JITTER_US"), parse(parse_jitter_us_to_pico), fallback(agogo::chan::conn::fixed::Pico::ZERO))]
        jitter_us: agogo::chan::conn::fixed::Pico,
        #[bpaf(long, argument("PULSES"), parse(parse_positive_u32))]
        pulses: u32,
        #[bpaf(long, argument("SEED"), fallback(1))]
        seed: u64,
    },
}

pub fn dispatch(sub: SyncSub) -> Result<(), String> {
    match sub {
        SyncSub::Trace {
            bpm,
            sr,
            ppq,
            jitter_us,
            pulses,
            seed,
        } => {
            if sr != <agogo::chan::conn::rate::R048 as agogo::chan::conn::rate::SampleRate>::HZ {
                return Err(format!(
                    "sync trace is pinned to 48 kHz this sprint (got --sr {sr}); \
                     multi-rate support deferred"
                ));
            }
            let rows = trace(bpm, ppq, jitter_us, pulses, seed);
            println!("bits_q48_16,tempo_ubpm,phase_q32");
            for r in rows {
                println!("{},{},{}", r.bits_q48_16, r.tempo_ubpm, r.phase_q32);
            }
            Ok(())
        }
    }
}

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
    /// Peak position as raw Q48.16 bits at R048's 48 kHz.
    pub bits_q48_16: i64,
    /// PLL smoothed BPM × 10⁶.
    pub tempo_ubpm: u32,
    /// PLL phase, Q0.32 cycles.
    pub phase_q32: u32,
}

pub fn trace(bpm: Tempo, ppq: u32, jitter: Pico, pulses: u32, seed: u64) -> Vec<TraceRow> {
    let (samples, _truth): (Vec<f32>, Vec<R048>) = pulse_train_s048(bpm, ppq, jitter, pulses, seed);
    let pulse_rate_hz = agogo::chan::conn::float::tempo_to_hz(bpm, ppq);
    let spacing_samples = (R048::HZ as f64 / pulse_rate_hz) as u32;
    let mut detector = PeakDetector::<R048>::new(DetectorConfig {
        threshold_q15: 16_384, // 0.5 Q0.15
        hold_samples: spacing_samples / 2,
    });
    let mut pll = Pll::<R048>::new(PllSettings::DEFAULT, bpm, ppq);
    let peaks = detector.process(&samples, 0);
    peaks
        .into_iter()
        .map(|p| {
            let out = pll.step(Some(p.sample_index));
            TraceRow {
                bits_q48_16: p.sample_index.to_bits(),
                tempo_ubpm: out.bpm.0,
                phase_q32: out.phase.0,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// E2E gate from the plan's build gates: 256 pulses at 120 BPM /
    /// 48 kHz / 24 PPQ with 50 µs jitter must converge to within
    /// ±50 000 µBPM (0.05 BPM) of 120 × 10⁶ by the end of the trace.
    #[test]
    fn sync_trace_converges() {
        // 50 µs = 50 × 10⁶ ps. Pico is the FD12 (1 ps) rung.
        let rows = trace(Tempo::from_bpm_integer(120), 24, Pico(50_000_000), 256, 1);
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
}
