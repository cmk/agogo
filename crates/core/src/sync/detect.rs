//! Peak detector with parabolic sub-sample interpolation.
//!
//! Streaming, block-at-a-time. State (the last two samples and a hold
//! countdown) carries between `process` calls so peaks straddling block
//! boundaries are not lost.
//!
//! Rate-parameterised via `R: SampleTime` — the detector itself operates
//! on `&[f32]` PCM blocks (cpal ABI), and emits `Peak<R>` so downstream
//! consumers don't confuse two rates at compile time.

use crate::time::sample::SampleTime;
use core::marker::PhantomData;

/// A detected pulse with sub-sample arrival precision.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Peak<R: SampleTime> {
    /// Stream-global position of the interpolated peak centre.
    pub sample_index: R,
}

/// Detector tuning.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DetectorConfig {
    /// Minimum amplitude for a candidate to be considered a peak, as
    /// Q0.15 of full-scale (`16_384 ≈ 0.5`, `32_768 ≈ 1.0`). Compared
    /// against the normalised f32 PCM input inside `process` — this is
    /// the one narrow f32 comparison in the detector, and lives at the
    /// cpal ABI boundary.
    pub threshold_q15: u16,
    /// Minimum number of samples between consecutive emitted peaks: a
    /// peak at sample `n` suppresses any candidate at samples `n+1`
    /// through `n + hold_samples - 1` inclusive; the next emission is
    /// allowed at `n + hold_samples` or later.
    pub hold_samples: u32,
}

#[derive(Debug, Clone)]
struct DetectorState {
    prev2: f32,
    prev1: f32,
    have_prev2: bool,
    have_prev1: bool,
    hold_remaining: u32,
}

impl DetectorState {
    fn new() -> Self {
        Self {
            prev2: 0.0,
            prev1: 0.0,
            have_prev2: false,
            have_prev1: false,
            hold_remaining: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PeakDetector<R: SampleTime> {
    cfg: DetectorConfig,
    state: DetectorState,
    _rate: PhantomData<R>,
}

impl<R: SampleTime> PeakDetector<R> {
    pub fn new(cfg: DetectorConfig) -> Self {
        Self {
            cfg,
            state: DetectorState::new(),
            _rate: PhantomData,
        }
    }

    pub fn config(&self) -> DetectorConfig {
        self.cfg
    }

    /// Reset all state. The next call to `process` behaves as if it
    /// were the first since construction.
    pub fn reset(&mut self) {
        self.state = DetectorState::new();
    }

    /// Process a block of samples and return all peaks discovered in it.
    ///
    /// `start_index` is the stream-global sample count of `samples[0]`,
    /// so returned `Peak::sample_index` values are stream-global and
    /// monotonically increasing across calls.
    pub fn process(&mut self, samples: &[f32], start_index: u64) -> Vec<Peak<R>> {
        let mut peaks = Vec::new();
        // ABI-local: Q0.15 threshold → f32 for one compare against
        // normalised PCM input below.
        let threshold_f32 = (self.cfg.threshold_q15 as f32) / 32_768.0;

        for (offset, &cur) in samples.iter().enumerate() {
            // Decrement hold first; the check below requires it to
            // reach 0 before another peak can be emitted.
            if self.state.hold_remaining > 0 {
                self.state.hold_remaining -= 1;
            }

            if self.state.have_prev2 && self.state.have_prev1 {
                let y_m1 = self.state.prev2;
                let y_0 = self.state.prev1;
                let y_p1 = cur;

                // `>=` on the left so the rightmost sample of a
                // plateau is treated as the local max — necessary for
                // smooth pulses whose apex lands exactly half-way
                // between two integer samples.
                let is_local_max = y_0 >= y_m1 && y_0 > y_p1;
                let above_threshold = y_0 >= threshold_f32;

                if is_local_max && above_threshold && self.state.hold_remaining == 0 {
                    // ABI-local f64: parabolic-fit locals. Contained
                    // to these three lines; converted to Q48.16 bits
                    // before any value escapes.
                    let denom = (y_m1 - 2.0 * y_0 + y_p1) as f64;
                    let frac = if denom.abs() < 1e-30 {
                        0.0
                    } else {
                        (0.5 * (y_m1 - y_p1) as f64 / denom).clamp(-0.5, 0.5)
                    };

                    // i128 keeps the intermediate product exact; the
                    // final i64 cast bounds the stream to the Q48.16
                    // range (≤ 2⁴⁷ samples ≈ 93 000 years at 48 kHz).
                    // Panic rather than wrap silently if a caller
                    // exceeds that.
                    let centre_int: i128 = (start_index as i128) + (offset as i128) - 1;
                    let frac_q16 = (frac * 65_536.0).round() as i64;
                    let bits_q48_16 = i64::try_from(centre_int * 65_536)
                        .expect("stream index in Q48.16 must fit in i64")
                        + frac_q16;

                    peaks.push(Peak {
                        sample_index: R::from_bits_q48_16(bits_q48_16),
                    });
                    self.state.hold_remaining = self.cfg.hold_samples;
                }
            }

            self.state.prev2 = self.state.prev1;
            self.state.prev1 = cur;
            self.state.have_prev2 = self.state.have_prev1;
            self.state.have_prev1 = true;
        }

        peaks
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arb::{arb_bpm, pulse_train};
    use crate::boundary::tempo_to_hz;
    use crate::time::decimal::Pico;
    use crate::time::sample::{S048, SampleRate, SampleTime};
    use crate::time::tempo::Tempo;
    use proptest::prelude::*;

    /// Stamp a Hann-bell pulse into `buf`. Mirrors `pulse_train`'s
    /// shape so detector tests that don't need full pulse-train
    /// scaffolding can still inject single pulses cleanly.
    fn emit_hann(buf: &mut Vec<f32>, centre: f64, width: f64) {
        let half_width = width * 0.5;
        let start = ((centre - half_width).floor() as i64).max(0) as usize;
        let end = ((centre + half_width).ceil() as i64).max(0) as usize;
        if buf.len() <= end {
            buf.resize(end + 1, 0.0);
        }
        for (n, slot) in buf.iter_mut().enumerate().take(end + 1).skip(start) {
            let dx = n as f64 - centre;
            if dx.abs() > half_width {
                continue;
            }
            let v = (std::f64::consts::PI * dx / width).cos();
            *slot += (v * v) as f32;
        }
    }

    /// Q0.15 helper for tests: 0.5 full-scale.
    const THRESHOLD_HALF: u16 = 16_384;

    #[test]
    fn empty_block_yields_no_peaks() {
        let mut det = PeakDetector::<S048>::new(DetectorConfig {
            threshold_q15: 3_277, // ≈ 0.1
            hold_samples: 10,
        });
        assert!(det.process(&[], 0).is_empty());
    }

    #[test]
    fn five_handcrafted_peaks_at_48k() {
        // Spot-check from the plan: 5 peaks at 1000-sample spacing,
        // detector recovers all 5 within ±0.1 sample.
        let mut buf = Vec::new();
        let centres = [1000.0_f64, 2000.0, 3000.5, 4000.25, 5000.0];
        for &c in &centres {
            emit_hann(&mut buf, c, 72.0);
        }
        let mut det = PeakDetector::<S048>::new(DetectorConfig {
            threshold_q15: THRESHOLD_HALF,
            hold_samples: 500,
        });
        let peaks = det.process(&buf, 0);
        assert_eq!(peaks.len(), centres.len(), "peak count");
        for (p, &truth) in peaks.iter().zip(centres.iter()) {
            let got = p.sample_index.samples_f64();
            assert!(
                (got - truth).abs() < 0.1,
                "detected {} vs truth {} (err {})",
                got,
                truth,
                (got - truth).abs()
            );
        }
    }

    #[test]
    fn block_boundary_does_not_lose_peak() {
        // Place a peak that straddles two process() calls.
        let mut buf = vec![0.0_f32; 1024];
        emit_hann(&mut buf, 510.0, 72.0);
        let mut det = PeakDetector::<S048>::new(DetectorConfig {
            threshold_q15: THRESHOLD_HALF,
            hold_samples: 100,
        });
        let mut all = det.process(&buf[..512], 0);
        all.extend(det.process(&buf[512..], 512));
        assert_eq!(all.len(), 1);
        let got = all[0].sample_index.samples_f64();
        assert!((got - 510.0).abs() < 0.1);
    }

    proptest! {
        // P1: every truth peak is reported exactly once, no extras.
        // Pinned to S048 for this sprint; multi-rate coverage deferred
        // (the detector algorithm is rate-agnostic — it operates on
        // &[f32] — so the rate only affected the test's own expected-
        // values math).
        #[test]
        fn detector_recovers_all_peaks(
            bpm in arb_bpm(),
            seed in any::<u64>(),
            n_pulses in 4u32..32u32,
        ) {
            let sr = S048::HZ;
            let ppq = 24u32;
            let (samples, truth): (Vec<f32>, Vec<S048>) =
                pulse_train::<S048>(bpm, ppq, Pico(0), n_pulses, seed);
            let pulse_rate_hz = tempo_to_hz(bpm, ppq);
            let spacing_samples = sr as f64 / pulse_rate_hz;
            let hold = (spacing_samples * 0.5) as u32;
            let mut det = PeakDetector::<S048>::new(DetectorConfig {
                threshold_q15: THRESHOLD_HALF,
                hold_samples: hold,
            });
            let detected = det.process(&samples, 0);
            prop_assert_eq!(
                detected.len(),
                truth.len(),
                "got {} peaks, expected {}",
                detected.len(),
                truth.len()
            );
            for (d, &t) in detected.iter().zip(truth.iter()) {
                let err_bits = (d.sample_index.to_bits_q48_16() - t.to_bits_q48_16()).abs();
                // PI-exempt: Q48.16 bit-difference → fractional samples
                // (binary scale, intrinsic to the representation).
                let err = err_bits as f64 / (1u64 << 16) as f64;
                prop_assert!(
                    err < spacing_samples * 0.5,
                    "peak err {} exceeded spacing/2 = {}",
                    err,
                    spacing_samples * 0.5
                );
            }
        }

        // P2: clean Hann pulses → ≤ 0.1-sample sub-sample precision.
        #[test]
        fn detector_subsample_precision(
            bpm in arb_bpm(),
            seed in any::<u64>(),
            n_pulses in 4u32..16u32,
        ) {
            let sr = S048::HZ;
            let ppq = 24u32;
            let (samples, truth): (Vec<f32>, Vec<S048>) =
                pulse_train::<S048>(bpm, ppq, Pico(0), n_pulses, seed);
            let pulse_rate_hz = tempo_to_hz(bpm, ppq);
            let spacing_samples = sr as f64 / pulse_rate_hz;
            let hold = (spacing_samples * 0.5) as u32;
            let mut det = PeakDetector::<S048>::new(DetectorConfig {
                threshold_q15: THRESHOLD_HALF,
                hold_samples: hold,
            });
            let detected = det.process(&samples, 0);
            prop_assert_eq!(detected.len(), truth.len());
            for (d, &t) in detected.iter().zip(truth.iter()) {
                let err_bits = (d.sample_index.to_bits_q48_16() - t.to_bits_q48_16()).abs();
                // PI-exempt: Q48.16 bit-difference → fractional samples
                // (binary scale, intrinsic to the representation).
                let err = err_bits as f64 / (1u64 << 16) as f64;
                prop_assert!(
                    err <= 0.1,
                    "subsample err {} > 0.1 (bpm={})",
                    err, bpm.0
                );
            }
        }

        // P3: a second pulse arriving inside the hold window is suppressed.
        #[test]
        fn detector_hold_blocks_doubles(
            gap_samples in 80u32..400u32,
            seed in any::<u64>(),
        ) {
            let _ = seed;
            let pulse_width = 72.0_f64;
            let first_centre = 200.0_f64;
            let second_centre = first_centre + gap_samples as f64;
            let hold = gap_samples + 50;
            let mut buf = Vec::new();
            emit_hann(&mut buf, first_centre, pulse_width);
            emit_hann(&mut buf, second_centre, pulse_width);
            let mut det = PeakDetector::<S048>::new(DetectorConfig {
                threshold_q15: THRESHOLD_HALF,
                hold_samples: hold,
            });
            let peaks = det.process(&buf, 0);
            prop_assert_eq!(
                peaks.len(),
                1,
                "expected 1 peak with hold={} > gap={}, got {}",
                hold, gap_samples, peaks.len()
            );
            let got = peaks[0].sample_index.samples_f64();
            prop_assert!((got - first_centre).abs() < 0.5);
        }

        #[test]
        fn detector_hold_allows_when_gap_exceeds_hold(
            gap_samples in 200u32..600u32,
        ) {
            let pulse_width = 72.0_f64;
            let first_centre = 200.0_f64;
            let second_centre = first_centre + gap_samples as f64;
            let hold = gap_samples / 2;
            let mut buf = Vec::new();
            emit_hann(&mut buf, first_centre, pulse_width);
            emit_hann(&mut buf, second_centre, pulse_width);
            let mut det = PeakDetector::<S048>::new(DetectorConfig {
                threshold_q15: THRESHOLD_HALF,
                hold_samples: hold,
            });
            let peaks = det.process(&buf, 0);
            prop_assert_eq!(peaks.len(), 2);
        }
    }

    // Suppress Tempo import drain-warning on cfg(not(test)).
    #[allow(dead_code)]
    fn _tempo_unused_reminder(_: Tempo) {}
}
