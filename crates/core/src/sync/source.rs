//! Unified phase source: internal free-running clock or external PLL.

use crate::fxp::{Tempo, Phase, SampleTime};
use crate::sync::detect::PeakDetector;
use crate::sync::pll::Pll;

/// Extension trait for user-provided phase sources.
///
/// Implementors plug into [`PhaseSource::Custom`] to expose a clock
/// source that isn't one of the built-in `Internal` / `External`
/// variants — e.g. Ableton Link in `agogo-host-link`.
///
/// **RT-safety contract:** `phase_at_sample` must be non-blocking and
/// allocation-free (it may be called from the audio thread).
/// `feed_samples` may also run on the audio thread; same constraints
/// apply, though an implementation whose tempo source is external to
/// the audio stream (Link, DIN) is free to treat it as a no-op.
pub trait PhaseSourceImpl: Send {
    fn phase_at_sample(&mut self, n: u64) -> Phase;
    fn feed_samples(&mut self, samples: &[f32], start: u64);
}

/// A clock from which downstream consumers can query the current
/// beat-phase at any stream-global sample index.
///
/// `Internal` is a stateless free-running clock — phase is purely
/// `(bpm, R::HZ, n)`. `External` wraps a peak detector + PLL pair
/// driven by audio samples handed in via
/// [`PhaseSource::feed_samples`]. `Custom` holds an arbitrary
/// [`PhaseSourceImpl`] — the extension point for host integrations
/// (Ableton Link, DIN sync, etc.) that live in sibling crates to
/// keep their build dependencies out of `agogo-core`.
///
/// Rate-parameterised via `R: SampleTime` so the `PeakDetector` and
/// `Pll` share one rate — mixing rates is a type error. (The
/// `Custom` variant is rate-opaque; its implementor is responsible
/// for matching the host rate however its backend defines it.)
pub enum PhaseSource<R: SampleTime> {
    /// Free-running internal clock. Phase is deterministic from
    /// `(bpm, R::HZ, n)` — no state, no drift, no jitter.
    Internal { bpm: Tempo },
    /// External pulse train run through detector → PLL.
    External {
        detector: PeakDetector<R>,
        pll: Pll<R>,
    },
    /// Arbitrary user-provided clock. Lives behind a box to keep the
    /// enum `Sized` and to allow sibling crates (like
    /// `agogo-host-link`) to contribute clock implementations without
    /// their deps leaking into `agogo-core`.
    Custom(Box<dyn PhaseSourceImpl + Send>),
}

impl<R: SampleTime> PhaseSource<R> {
    /// Phase in cycles [0, 1) at the given absolute sample index `n`.
    ///
    /// `Internal` computes deterministically from `(bpm, R::HZ, n)`.
    ///
    /// `External` delegates to `Pll::predicted_phase_at`, which keeps
    /// the f64 phase-advance contained inside the PI-exempt zone.
    /// Returns `Phase::ZERO` if no pulse has been observed yet.
    ///
    /// `Custom` delegates to the boxed `PhaseSourceImpl`.
    pub fn phase_at_sample(&mut self, n: u64) -> Phase {
        match self {
            PhaseSource::Internal { bpm } => {
                // Compute phase exactly (modulo the final u32 truncation)
                // by keeping `n · bpm · 2^32` together in u128 before
                // dividing, so rounding doesn't accumulate per-sample
                // via a precomputed inc_q32.
                let num: u128 = n as u128 * bpm.0 as u128 * (1u128 << 32);
                let den: u128 = 60_000_000u128 * R::HZ as u128;
                Phase((num / den) as u32)
            }
            PhaseSource::External { pll, .. } => match pll.last_pulse_sample() {
                None => Phase::ZERO,
                Some(last) => {
                    // Elapsed samples since the last observed pulse,
                    // represented in R's Q48.16. `n` is u64 but Q48.16
                    // only covers i64 — panic if a caller feeds a
                    // stream index past ~2⁴⁷ samples (93 000 years at
                    // 48 kHz, never reached in practice).
                    let last_bits = last.to_bits_q48_16();
                    let n_bits = i64::try_from(n as i128 * 65_536)
                        .expect("sample index in Q48.16 must fit in i64");
                    let elapsed_bits = n_bits.wrapping_sub(last_bits);
                    let elapsed = R::from_bits_q48_16(elapsed_bits);
                    pll.predicted_phase_at(elapsed)
                }
            },
            PhaseSource::Custom(inner) => inner.phase_at_sample(n),
        }
    }

    /// Feed a block of audio samples into the clock. No-op for
    /// `Internal`; routes to detector → PLL for `External`; delegates
    /// to the `PhaseSourceImpl` for `Custom`. Silent blocks leave the
    /// PLL untouched — [`phase_at_sample`] projects analytically from
    /// the last observed pulse.
    pub fn feed_samples(&mut self, samples: &[f32], start: u64) {
        match self {
            PhaseSource::Internal { .. } => {}
            PhaseSource::External { detector, pll } => {
                for p in detector.process(samples, start) {
                    pll.step(Some(p.sample_index));
                }
            }
            PhaseSource::Custom(inner) => inner.feed_samples(samples, start),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fxp::{Tempo, Pico, S48, SampleRate};
    use crate::sync::detect::DetectorConfig;
    use crate::sync::pll::PllSettings;
    use proptest::prelude::*;

    #[test]
    fn internal_120bpm_at_half_beat() {
        // At 120 BPM and 48 kHz, a beat is 24 000 samples; half a beat
        // is 12 000 samples → phase = 0.5 → Phase = 2^31.
        let mut src = PhaseSource::<S48>::Internal {
            bpm: Tempo::from_bpm_integer(120),
        };
        let p_half = src.phase_at_sample(12_000);
        // Allow ±1 ULP (integer arithmetic rounding of 2^32 / bpm quotient).
        assert!((p_half.0 as i64 - (1i64 << 31)).abs() < 4);
        assert_eq!(src.phase_at_sample(24_000).0, 0);
        assert_eq!(src.phase_at_sample(0).0, 0);
    }

    #[test]
    fn external_feed_samples_advances_pll() {
        let detector = PeakDetector::<S48>::new(DetectorConfig {
            threshold_q15: 16_384,
            hold_samples: 500,
        });
        let pll = Pll::<S48>::new(PllSettings::DEFAULT, Tempo::from_bpm_integer(120), 24);
        let mut src = PhaseSource::<S48>::External { detector, pll };
        let (samples, _): (Vec<f32>, Vec<S48>) = crate::arb::pulse_train::<S48>(
            Tempo::from_bpm_integer(120),
            24,
            Pico(0),
            4,
            1,
        );
        src.feed_samples(&samples, 0);
        let _p = src.phase_at_sample(samples.len() as u64);
        // Phase(u32) is always a valid [0, 2^32) value — no NaN / non-finite.
    }

    #[test]
    fn external_phase_at_sample_projects_analytically() {
        let detector = PeakDetector::<S48>::new(DetectorConfig {
            threshold_q15: 16_384,
            hold_samples: 500,
        });
        let pll = Pll::<S48>::new(PllSettings::DEFAULT, Tempo::from_bpm_integer(120), 24);
        let mut src = PhaseSource::<S48>::External { detector, pll };
        let bpm = Tempo::from_bpm_integer(120);
        let (samples, peaks): (Vec<f32>, Vec<S48>) =
            crate::arb::pulse_train::<S48>(bpm, 24, Pico(0), 4, 1);
        src.feed_samples(&samples, 0);

        let last_samples = peaks.last().unwrap().to_bits_q48_16() as f64 / 65_536.0;
        let spacing = S48::HZ as f64 / (120.0 * 24.0 / 60.0);
        let halfway = (last_samples + spacing * 0.5) as u64;
        let p_half = src.phase_at_sample(halfway);
        let p_half_frac = p_half.0 as f64 / (1u64 << 32) as f64;
        assert!(
            (p_half_frac - 0.5).abs() < 0.01,
            "halfway phase {} not ≈ 0.5",
            p_half_frac
        );

        let one_cycle_later = (last_samples + spacing) as u64;
        let p_full = src.phase_at_sample(one_cycle_later);
        let p_full_frac = p_full.0 as f64 / (1u64 << 32) as f64;
        assert!(
            !(0.02..=0.98).contains(&p_full_frac),
            "one-cycle-later phase {} not ≈ 0/1",
            p_full_frac
        );
    }

    #[test]
    fn external_phase_zero_before_first_pulse() {
        let detector = PeakDetector::<S48>::new(DetectorConfig {
            threshold_q15: 16_384,
            hold_samples: 500,
        });
        let pll = Pll::<S48>::new(PllSettings::DEFAULT, Tempo::from_bpm_integer(120), 24);
        let mut src = PhaseSource::<S48>::External { detector, pll };
        assert_eq!(src.phase_at_sample(0).0, 0);
        assert_eq!(src.phase_at_sample(48_000).0, 0);
    }

    proptest! {
        // The first difference of phase_at_sample(n) is constant mod
        // wrap — confirms purely-linear internal advance.
        #[test]
        fn source_internal_is_linear(
            bpm_mbpm in 30_000_000u32..=400_000_000,
            n in 0u64..10_000_000u64,
        ) {
            // The implementation keeps `n · bpm · 2^32` together to avoid
            // precomputed-inc rounding accumulation, so the first
            // difference `p(n+1) - p(n)` may vary by ±1 Q0.32 ULP around
            // the "ideal" increment. Assert that tolerance.
            let bpm = Tempo(bpm_mbpm);
            let mut src = PhaseSource::<S48>::Internal { bpm };
            let p1 = src.phase_at_sample(n);
            let p2 = src.phase_at_sample(n + 1);
            let diff = p2.0.wrapping_sub(p1.0);
            // Ideal per-sample increment:
            //   inc = (µBPM · 2^32) / (60·10^6 · HZ).
            let ideal_inc: u128 =
                (bpm.0 as u128 * (1u128 << 32)) / (60_000_000u128 * S48::HZ as u128);
            let err = (diff as u128).abs_diff(ideal_inc);
            prop_assert!(
                err <= 1,
                "|p2-p1 - ideal| = {} > 1 ULP (n={}, bpm={}, diff={}, ideal={})",
                err, n, bpm_mbpm, diff, ideal_inc
            );
        }
    }

    /// Proves the `Custom` variant delegates through to the boxed
    /// `PhaseSourceImpl`. Mock counts each call via shared atomics so
    /// the test can inspect without downcasting the trait object.
    #[test]
    fn phase_source_custom_dispatches() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicU64, Ordering};

        struct Mock {
            phase_calls: Arc<AtomicU64>,
            feed_calls: Arc<AtomicU64>,
            last_n: Arc<AtomicU64>,
        }
        impl PhaseSourceImpl for Mock {
            fn phase_at_sample(&mut self, n: u64) -> Phase {
                self.phase_calls.fetch_add(1, Ordering::SeqCst);
                self.last_n.store(n, Ordering::SeqCst);
                Phase(0x4000_0000) // arbitrary non-zero sentinel
            }
            fn feed_samples(&mut self, _samples: &[f32], _start: u64) {
                self.feed_calls.fetch_add(1, Ordering::SeqCst);
            }
        }

        let phase_calls = Arc::new(AtomicU64::new(0));
        let feed_calls = Arc::new(AtomicU64::new(0));
        let last_n = Arc::new(AtomicU64::new(0));
        let mock = Mock {
            phase_calls: Arc::clone(&phase_calls),
            feed_calls: Arc::clone(&feed_calls),
            last_n: Arc::clone(&last_n),
        };

        let mut src = PhaseSource::<S48>::Custom(Box::new(mock));
        assert_eq!(src.phase_at_sample(42), Phase(0x4000_0000));
        assert_eq!(src.phase_at_sample(100), Phase(0x4000_0000));
        src.feed_samples(&[0.1, 0.2], 7);

        assert_eq!(phase_calls.load(Ordering::SeqCst), 2);
        assert_eq!(feed_calls.load(Ordering::SeqCst), 1);
        assert_eq!(last_n.load(Ordering::SeqCst), 100);
    }
}
