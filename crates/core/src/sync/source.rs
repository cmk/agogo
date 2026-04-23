//! Unified phase source: internal free-running clock or external PLL.

use crate::sync::detect::PeakDetector;
use crate::sync::pll::Pll;

/// A clock from which downstream consumers can query the current
/// beat-phase at any stream-global sample index.
///
/// `Internal` is a stateless free-running clock — phase is purely
/// `(bpm, sr, n)`. `External` wraps a peak detector + PLL pair driven
/// by audio samples handed in via [`PhaseSource::feed_samples`].
///
/// **Unit caveat**: `Internal::phase_at_sample` returns *beat-phase*
/// (cycles per quarter-note ∈ [0, 1)), while `External::phase_at_sample`
/// returns the PLL's *pulse-phase* (cycles per PPQ pulse). Sprint 3
/// integration will reconcile these via a SampleTickConn shim that
/// counts pulses-within-a-beat; this sprint exposes the raw PLL phase.
pub enum PhaseSource {
    /// Free-running internal clock. Phase is deterministic from
    /// `(bpm, sr, n)` — no state, no drift, no jitter.
    Internal { bpm: f32, sr: u32 },
    /// External pulse train run through detector → PLL. Caller drives
    /// it via [`PhaseSource::feed_samples`].
    External {
        detector: PeakDetector,
        pll: Pll,
    },
}

impl PhaseSource {
    /// Phase in cycles \[0, 1) at the given absolute sample index `n`.
    ///
    /// `Internal` computes deterministically from `(bpm, sr, n)`.
    ///
    /// `External` projects analytically from the PLL's last observed
    /// pulse: `phase = ((n - last_pulse_sample) * freq_hz / sr) mod 1`.
    /// Works for `n` before, at, or after the last pulse, and does not
    /// require [`feed_samples`] to free-run the PLL through silent
    /// intervals. Returns `0.0` if no pulse has been observed yet.
    pub fn phase_at_sample(&mut self, n: u64) -> f32 {
        match self {
            PhaseSource::Internal { bpm, sr } => {
                let beats_per_sec = *bpm as f64 / 60.0;
                (n as f64 * beats_per_sec / *sr as f64).rem_euclid(1.0) as f32
            }
            PhaseSource::External { pll, .. } => match pll.last_pulse_sample() {
                None => 0.0,
                Some(last) => {
                    let elapsed = n as f64 - last;
                    let cycles_per_sample = pll.state().freq_hz / pll.sr() as f64;
                    let projected =
                        (pll.state().phase + elapsed * cycles_per_sample).rem_euclid(1.0);
                    projected as f32
                }
            },
        }
    }

    /// Feed a block of audio samples into the clock. No-op for
    /// `Internal`; routes to detector → PLL for `External`. Silent
    /// blocks (no peaks detected) leave the PLL untouched —
    /// [`phase_at_sample`] projects analytically from the last
    /// observed pulse and does not need the PLL to be free-run
    /// through silence.
    pub fn feed_samples(&mut self, samples: &[f32], start: u64) {
        match self {
            PhaseSource::Internal { .. } => {}
            PhaseSource::External { detector, pll } => {
                for p in detector.process(samples, start) {
                    pll.step(Some(p.sample_index));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::detect::DetectorConfig;
    use crate::sync::pll::PllSettings;
    use proptest::prelude::*;

    #[test]
    fn internal_120bpm_at_half_beat() {
        // Spot check (adjusted from the plan): at 120 BPM and 48 kHz,
        // a beat is 24 000 samples, so half a beat is 12 000 samples
        // and phase = 0.5. The plan's text stated `phase_at_sample
        // (24000) == 0.5`, but n=24 000 is one full beat — phase
        // wraps to 0.0 there. Documented in the sprint Review.
        let mut src = PhaseSource::Internal {
            bpm: 120.0,
            sr: 48_000,
        };
        assert!((src.phase_at_sample(12_000) - 0.5).abs() < 1e-6);
        assert!(src.phase_at_sample(24_000).abs() < 1e-6);
        assert!(src.phase_at_sample(0).abs() < 1e-6);
    }

    #[test]
    fn external_feed_samples_advances_pll() {
        // Smoke check: feeding a synthetic pulse-train block makes
        // External's PLL state move (last_pulse_sample becomes Some).
        let detector = PeakDetector::new(DetectorConfig {
            threshold: 0.5,
            hold_samples: 500,
        });
        let pll = Pll::new(PllSettings::DEFAULT, 120.0, 48_000, 24);
        let mut src = PhaseSource::External { detector, pll };
        let (samples, _) = crate::arb::pulse_train(120.0, 48_000, 24, 0.0, 4, 1);
        src.feed_samples(&samples, 0);
        let p = src.phase_at_sample(samples.len() as u64);
        assert!(p.is_finite() && (0.0..1.0).contains(&p));
    }

    #[test]
    fn external_phase_at_sample_projects_analytically() {
        // After feeding a pulse train, queries for `n` between or
        // beyond observed pulses should return analytically projected
        // phase, not just the last-step snapshot.
        let detector = PeakDetector::new(DetectorConfig {
            threshold: 0.5,
            hold_samples: 500,
        });
        let pll = Pll::new(PllSettings::DEFAULT, 120.0, 48_000, 24);
        let mut src = PhaseSource::External { detector, pll };
        let (samples, peaks) = crate::arb::pulse_train(120.0, 48_000, 24, 0.0, 4, 1);
        src.feed_samples(&samples, 0);

        // Halfway between the last observed pulse and the next
        // expected pulse, phase should be ≈ 0.5.
        let last = *peaks.last().unwrap();
        let spacing = 48_000.0 / (120.0 * 24.0 / 60.0);
        let halfway = (last + spacing * 0.5) as u64;
        let p_half = src.phase_at_sample(halfway);
        assert!(
            (p_half - 0.5).abs() < 0.01,
            "halfway phase {} not ≈ 0.5",
            p_half
        );

        // At an integer number of cycles past the last pulse, phase
        // should wrap back to ≈ 0.
        let one_cycle_later = (last + spacing) as u64;
        let p_full = src.phase_at_sample(one_cycle_later);
        assert!(
            !(0.02..=0.98).contains(&p_full),
            "one-cycle-later phase {} not ≈ 0/1",
            p_full
        );
    }

    #[test]
    fn external_phase_zero_before_first_pulse() {
        // No pulse seen yet → phase_at_sample returns 0.0.
        let detector = PeakDetector::new(DetectorConfig {
            threshold: 0.5,
            hold_samples: 500,
        });
        let pll = Pll::new(PllSettings::DEFAULT, 120.0, 48_000, 24);
        let mut src = PhaseSource::External { detector, pll };
        assert_eq!(src.phase_at_sample(0), 0.0);
        assert_eq!(src.phase_at_sample(48_000), 0.0);
    }

    proptest! {
        // P: source_internal_is_linear
        // The first difference of phase_at_sample(n) is constant
        // (modulo the wrap at 1.0).
        #[test]
        fn source_internal_is_linear(
            bpm in 30.0_f32..400.0_f32,
            sr in prop_oneof![Just(44_100u32), Just(48_000), Just(96_000), Just(192_000)],
            n in 0u64..10_000_000u64,
        ) {
            let mut src = PhaseSource::Internal { bpm, sr };
            let p1 = src.phase_at_sample(n);
            let p2 = src.phase_at_sample(n + 1);
            let expected_inc = bpm as f64 / 60.0 / sr as f64;
            let observed_inc = if p2 >= p1 {
                (p2 - p1) as f64
            } else {
                (p2 + 1.0 - p1) as f64
            };
            prop_assert!(
                (observed_inc - expected_inc).abs() < 1e-5,
                "expected inc {} got {} (n={}, bpm={}, sr={})",
                expected_inc, observed_inc, n, bpm, sr
            );
        }
    }
}
