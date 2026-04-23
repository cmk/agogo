//! Peak detector with parabolic sub-sample interpolation.
//!
//! Streaming, block-at-a-time. State (the last two samples and a hold
//! countdown) carries between `process` calls so peaks straddling block
//! boundaries are not lost.

/// A detected pulse with sub-sample arrival precision.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Peak {
    /// Stream-global sample index of the interpolated peak centre.
    pub sample_index: f64,
    /// Parabolic-fit amplitude at the interpolated centre.
    pub amplitude: f32,
}

/// Detector tuning.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DetectorConfig {
    /// Minimum amplitude for a candidate to be considered a peak.
    pub threshold: f32,
    /// Minimum number of samples between consecutive emitted peaks.
    /// A peak at sample `n` blocks emission at samples `n+1 .. n+hold`.
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
pub struct PeakDetector {
    cfg: DetectorConfig,
    state: DetectorState,
}

impl PeakDetector {
    pub fn new(cfg: DetectorConfig) -> Self {
        Self {
            cfg,
            state: DetectorState::new(),
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
    /// `start_index` is the stream-global index of `samples[0]`, so
    /// returned `Peak::sample_index` values are stream-global and
    /// monotonically increasing across calls.
    pub fn process(&mut self, samples: &[f32], start_index: u64) -> Vec<Peak> {
        let mut peaks = Vec::new();

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
                let above_threshold = y_0 >= self.cfg.threshold;

                if is_local_max && above_threshold && self.state.hold_remaining == 0 {
                    let denom = (y_m1 - 2.0 * y_0 + y_p1) as f64;
                    let frac = if denom.abs() < 1e-30 {
                        0.0
                    } else {
                        (0.5 * (y_m1 - y_p1) as f64 / denom).clamp(-0.5, 0.5)
                    };

                    let centre_global =
                        (start_index as i64) + (offset as i64) - 1;
                    let sample_index = centre_global as f64 + frac;

                    // Parabolic-fit apex amplitude.
                    let amplitude =
                        y_0 - 0.25 * (y_p1 - y_m1) * frac as f32;

                    peaks.push(Peak {
                        sample_index,
                        amplitude,
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
    use crate::arb::{arb_bpm, arb_sample_rate, pulse_train};
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

    #[test]
    fn empty_block_yields_no_peaks() {
        let mut det = PeakDetector::new(DetectorConfig {
            threshold: 0.1,
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
        let mut det = PeakDetector::new(DetectorConfig {
            threshold: 0.5,
            hold_samples: 500,
        });
        let peaks = det.process(&buf, 0);
        assert_eq!(peaks.len(), centres.len(), "peak count");
        for (p, &truth) in peaks.iter().zip(centres.iter()) {
            assert!(
                (p.sample_index - truth).abs() < 0.1,
                "detected {} vs truth {} (err {})",
                p.sample_index,
                truth,
                (p.sample_index - truth).abs()
            );
        }
    }

    #[test]
    fn block_boundary_does_not_lose_peak() {
        // Place a peak that straddles two process() calls.
        let mut buf = vec![0.0_f32; 1024];
        emit_hann(&mut buf, 510.0, 72.0); // apex 510, spans ~474..546
        let mut det = PeakDetector::new(DetectorConfig {
            threshold: 0.5,
            hold_samples: 100,
        });
        let mut all = det.process(&buf[..512], 0);
        all.extend(det.process(&buf[512..], 512));
        assert_eq!(all.len(), 1);
        assert!((all[0].sample_index - 510.0).abs() < 0.1);
    }

    proptest! {
        // P1: every truth peak is reported exactly once, no extras.
        #[test]
        fn detector_recovers_all_peaks(
            bpm in arb_bpm(),
            sr in arb_sample_rate(),
            seed in any::<u64>(),
            n_pulses in 4u32..32u32,
        ) {
            let ppq = 24u32;
            let (samples, truth) = pulse_train(bpm, sr, ppq, 0.0, n_pulses, seed);
            let pulse_rate_hz = bpm as f64 * ppq as f64 / 60.0;
            let spacing_samples = sr as f64 / pulse_rate_hz;
            let hold = (spacing_samples * 0.5) as u32;
            let mut det = PeakDetector::new(DetectorConfig {
                threshold: 0.5,
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
                let err = (d.sample_index - t).abs();
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
            sr in arb_sample_rate(),
            seed in any::<u64>(),
            n_pulses in 4u32..16u32,
        ) {
            let ppq = 24u32;
            let (samples, truth) = pulse_train(bpm, sr, ppq, 0.0, n_pulses, seed);
            let pulse_rate_hz = bpm as f64 * ppq as f64 / 60.0;
            let spacing_samples = sr as f64 / pulse_rate_hz;
            let hold = (spacing_samples * 0.5) as u32;
            let mut det = PeakDetector::new(DetectorConfig {
                threshold: 0.5,
                hold_samples: hold,
            });
            let detected = det.process(&samples, 0);
            prop_assert_eq!(detected.len(), truth.len());
            for (d, &t) in detected.iter().zip(truth.iter()) {
                let err = (d.sample_index - t).abs();
                prop_assert!(
                    err <= 0.1,
                    "subsample err {} > 0.1 (sr={} bpm={})",
                    err, sr, bpm
                );
            }
        }

        // P3: a second pulse arriving inside the hold window is suppressed.
        #[test]
        fn detector_hold_blocks_doubles(
            gap_samples in 80u32..400u32,
            seed in any::<u64>(),
        ) {
            let _ = seed; // RNG unused; keeps proptest happy with shrinking
            let pulse_width = 72.0_f64;
            let first_centre = 200.0_f64;
            let second_centre = first_centre + gap_samples as f64;
            // hold strictly larger than the gap → second peak suppressed.
            let hold = gap_samples + 50;
            let mut buf = Vec::new();
            emit_hann(&mut buf, first_centre, pulse_width);
            emit_hann(&mut buf, second_centre, pulse_width);
            let mut det = PeakDetector::new(DetectorConfig {
                threshold: 0.5,
                hold_samples: hold,
            });
            let peaks = det.process(&buf, 0);
            prop_assert_eq!(
                peaks.len(),
                1,
                "expected 1 peak with hold={} > gap={}, got {}",
                hold, gap_samples, peaks.len()
            );
            prop_assert!((peaks[0].sample_index - first_centre).abs() < 0.5);
        }

        // Negative-control for P3: with hold << gap, both peaks emit.
        #[test]
        fn detector_hold_allows_when_gap_exceeds_hold(
            gap_samples in 200u32..600u32,
        ) {
            let pulse_width = 72.0_f64;
            let first_centre = 200.0_f64;
            let second_centre = first_centre + gap_samples as f64;
            let hold = gap_samples / 2; // hold smaller than gap
            let mut buf = Vec::new();
            emit_hann(&mut buf, first_centre, pulse_width);
            emit_hann(&mut buf, second_centre, pulse_width);
            let mut det = PeakDetector::new(DetectorConfig {
                threshold: 0.5,
                hold_samples: hold,
            });
            let peaks = det.process(&buf, 0);
            prop_assert_eq!(peaks.len(), 2);
        }
    }
}
