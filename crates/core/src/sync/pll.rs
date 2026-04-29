//! Type-II (PI) second-order phase-locked loop.
//!
//! Drives a numerically-controlled oscillator (NCO) toward a measured
//! pulse train. The phase detector compares observed inter-pulse spacing
//! against the prediction; a PI loop filter steers the NCO frequency.
//!
//! Two frequencies are tracked:
//!
//! - `state.freq_hz` is the PI-driven prediction frequency. Used to
//!   advance phase between pulses and compute the next expected
//!   spacing. Bounces per-pulse with the `kp * phase_error` term.
//! - The reported `PllOutput::bpm` is integrator-only:
//!   `nominal_freq_hz * (1 + integrator) * 60 / ppq`. This is the
//!   smoothed estimate downstream consumers want — it tracks the
//!   underlying tempo without per-pulse jitter modulation.
//!
//! ## PI-exempt state
//!
//! `PllSettings { kp, ki, clamp_hz, interp }` and `PllState
//! { phase, freq_hz, integrator }` stay `f64` — they are the analog
//! control-law quantities the user explicitly exempted from the
//! no-float rule. The only f64→fxp casts live in
//! [`crate::boundary::f64_bpm_to_tempo`] / [`crate::boundary::f64_phase_to_phase`]
//! at the `PllOutput` boundary.

use crate::boundary::{bits_q48_16_to_seconds, f64_bpm_to_tempo, f64_phase_to_phase, tempo_to_hz};
use crate::sync::phase::Phase;
use crate::time::sample::SampleTime;
use crate::time::tempo::Tempo;

/// Loop-filter tuning.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PllSettings {
    pub kp: f64,
    pub ki: f64,
    pub clamp_hz: f64,
    pub interp: f64,
}

impl PllSettings {
    /// Default tuning sized for tracking 24-PPQ audio-sync at 48 kHz.
    pub const DEFAULT: PllSettings = PllSettings {
        kp: 0.1,
        ki: 0.002,
        clamp_hz: 50.0,
        interp: 0.0,
    };
}

impl Default for PllSettings {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Loop state. `phase` is in cycles [0, 1); `freq_hz` is the PI-driven
/// prediction rate (in pulses per second).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PllState {
    pub phase: f64,
    pub freq_hz: f64,
    pub integrator: f64,
}

/// One PLL update result.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PllOutput {
    pub bpm: Tempo,
    pub phase: Phase,
}

/// Phase-locked loop instance.
///
/// Parameterised by sample-rate type `R: SampleTime`. The f64 control-
/// law arithmetic uses `R::HZ` as the sample rate, so construction
/// doesn't need a separate `sr` argument.
#[derive(Debug, Clone)]
pub struct Pll<R: SampleTime> {
    cfg: PllSettings,
    state: PllState,
    ppq: u32,
    last_pulse_sample: Option<R>,
    nominal_freq_hz: f64,
}

impl<R: SampleTime> Pll<R> {
    /// Construct a PLL seeded at the nominal BPM. The loop pulls toward
    /// the measured rate from there; the integrator's clamp bounds how
    /// far it can roam.
    pub fn new(cfg: PllSettings, nominal_bpm: Tempo, ppq: u32) -> Self {
        assert!(nominal_bpm.0 > 0, "nominal bpm must be positive");
        assert!(ppq > 0, "ppq must be positive");
        // PI-exempt: convert the argv/µBPM nominal into f64 Hz for the
        // control law.
        let nominal_freq_hz = tempo_to_hz(nominal_bpm, ppq);
        Self {
            cfg,
            state: PllState {
                phase: 0.0,
                freq_hz: nominal_freq_hz,
                integrator: 0.0,
            },
            ppq,
            last_pulse_sample: None,
            nominal_freq_hz,
        }
    }

    pub fn settings(&self) -> PllSettings {
        self.cfg
    }
    pub fn state(&self) -> PllState {
        self.state
    }
    pub fn sr(&self) -> u32 {
        R::HZ
    }
    pub fn ppq(&self) -> u32 {
        self.ppq
    }
    pub fn nominal_freq_hz(&self) -> f64 {
        self.nominal_freq_hz
    }
    pub fn last_pulse_sample(&self) -> Option<R> {
        self.last_pulse_sample
    }

    /// Smoothed BPM derived from the integrator only.
    fn smoothed_bpm(&self) -> Tempo {
        // PI-exempt: integrator-driven f64 BPM → µBPM at the output boundary.
        let f = self.nominal_freq_hz * (1.0 + self.state.integrator);
        f64_bpm_to_tempo(f * 60.0 / self.ppq as f64)
    }

    /// Project the current NCO phase forward by `elapsed` samples —
    /// encapsulates the one f64 phase-advance for external consumers
    /// (e.g. `PhaseSource::External`) so the f64 stays inside the
    /// PI-exempt zone.
    pub fn predicted_phase_at(&self, elapsed: R) -> Phase {
        // PI-exempt. `elapsed_seconds × freq` is cycles; project the
        // current phase by that amount and wrap.
        let elapsed_seconds = bits_q48_16_to_seconds(elapsed.to_bits_q48_16(), R::HZ);
        let projected = self.state.phase + elapsed_seconds * self.state.freq_hz;
        f64_phase_to_phase(projected)
    }

    /// Advance one step.
    pub fn step(&mut self, measured_pulse_sample: Option<R>) -> PllOutput {
        if let Some(observed) = measured_pulse_sample {
            // PI-exempt: phase error computed in seconds (f64) from the
            // Q48.16-bits sample positions of prev and observed.
            let phase_error = match self.last_pulse_sample {
                Some(prev) => {
                    let prev_s = bits_q48_16_to_seconds(prev.to_bits_q48_16(), R::HZ);
                    let observed_s = bits_q48_16_to_seconds(observed.to_bits_q48_16(), R::HZ);
                    let observed_spacing_s = observed_s - prev_s;
                    let expected_spacing_s = 1.0 / self.state.freq_hz;
                    if expected_spacing_s < f64::EPSILON {
                        0.0
                    } else {
                        (expected_spacing_s - observed_spacing_s) / expected_spacing_s
                    }
                }
                None => 0.0,
            };

            self.state.integrator += self.cfg.ki * phase_error;
            let clamp_frac = self.cfg.clamp_hz / self.nominal_freq_hz;
            let min_integrator = (-clamp_frac).max(-1.0 + f64::EPSILON);
            self.state.integrator = self.state.integrator.clamp(min_integrator, clamp_frac);

            let correction = self.cfg.kp * phase_error + self.state.integrator;
            self.state.freq_hz = self.nominal_freq_hz * (1.0 + correction);
            if !self.state.freq_hz.is_finite() || self.state.freq_hz <= 0.0 {
                self.state.freq_hz = self.nominal_freq_hz;
            }

            // Snap NCO to the observed pulse boundary.
            self.state.phase = 0.0;
            self.last_pulse_sample = Some(observed);
        } else {
            let inc = self.state.freq_hz / R::HZ as f64;
            self.state.phase = (self.state.phase + inc).rem_euclid(1.0);
            if !self.state.phase.is_finite() {
                self.state.phase = 0.0;
            }
            if !self.state.freq_hz.is_finite() || self.state.freq_hz <= 0.0 {
                self.state.freq_hz = self.nominal_freq_hz;
            }
        }

        PllOutput {
            bpm: self.smoothed_bpm(),
            phase: f64_phase_to_phase(self.state.phase),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::pulse_train::pulse_train;
    use crate::time::decimal::{FD06, FD12FD06, Pico};
    use crate::time::sample::{S048, SampleRate};
    use proptest::prelude::*;

    /// PLL initialised at the true BPM under jitter — tracks the rate
    /// to within 50_000 µBPM after a 32-pulse warm-up.
    fn assert_bpm_converges(bpm: Tempo, jitter: Pico, seed: u64) {
        let ppq = 24u32;
        let n_pulses = 64u32;
        let (_, peaks): (Vec<f32>, Vec<S048>) =
            pulse_train::<S048>(bpm, ppq, jitter, n_pulses, seed);
        let mut pll = Pll::<S048>::new(PllSettings::DEFAULT, bpm, ppq);
        let mut last = bpm;
        for &p in &peaks {
            last = pll.step(Some(p)).bpm;
        }
        let err = last.abs_diff(bpm);
        assert!(
            err < 50_000,
            "bpm err {err} µBPM > 50_000 (last={} µBPM, target={} µBPM)",
            last.0,
            bpm.0
        );
    }

    #[test]
    fn default_settings_track_120_at_48k() {
        let ppq = 24u32;
        let bpm = Tempo::from_bpm_integer(120);
        let (_, peaks): (Vec<f32>, Vec<S048>) = pulse_train::<S048>(bpm, ppq, Pico(0), 48, 1);
        let mut pll = Pll::<S048>::new(PllSettings::DEFAULT, bpm, ppq);
        let mut last = Tempo::ZERO;
        for &p in &peaks {
            last = pll.step(Some(p)).bpm;
        }
        let err = last.abs_diff(Tempo(120_000_000));
        assert!(err < 1_000, "{last:?}");
    }

    #[test]
    fn jitter_free_tracks_perfectly() {
        assert_bpm_converges(Tempo::from_bpm_integer(120), Pico(0), 1);
    }

    #[test]
    fn integrator_clamp_keeps_bpm_positive() {
        // Regression: when `clamp_hz / nominal_freq_hz > 1.0` the
        // integrator could reach `-1.0` and drive `1 + integrator` to
        // zero or below. Check the f64 control-law state stays in the
        // valid range — the Tempo output may legitimately round to
        // 0 when smoothed_bpm is well below 1 µBPM without indicating
        // the regression.
        let ppq = 24u32;
        let nominal_bpm = Tempo::from_bpm_integer(120);
        let mut pll = Pll::<S048>::new(PllSettings::DEFAULT, nominal_bpm, ppq);
        let huge_spacing_samples: f64 = S048::HZ as f64 * 100.0;
        let mut t: f64 = 0.0;
        for _ in 0..1000 {
            let bits_q16 = (t * 65_536.0).round() as i64;
            let obs = S048::from_bits_q48_16(bits_q16);
            let _out = pll.step(Some(obs));
            assert!(
                pll.state().integrator > -1.0,
                "integrator fell to -1.0 or below: {}",
                pll.state().integrator
            );
            assert!(
                pll.state().freq_hz > 0.0 && pll.state().freq_hz.is_finite(),
                "freq_hz went non-positive: {}",
                pll.state().freq_hz
            );
            t += huge_spacing_samples;
        }
    }

    proptest! {
        #[test]
        fn pll_bpm_converges(
            bpm_mbpm in 60_000_000u32..200_000_000,
            jitter_us in 0u32..200,
            seed in any::<u64>(),
        ) {
            let bpm = Tempo(bpm_mbpm);
            let jitter = FD12FD06.inner(FD06(jitter_us as i64));
            let ppq = 24u32;
            let n_pulses = 64u32;
            let (_, peaks): (Vec<f32>, Vec<S048>) =
                pulse_train::<S048>(bpm, ppq, jitter, n_pulses, seed);
            let mut pll = Pll::<S048>::new(PllSettings::DEFAULT, bpm, ppq);
            let mut last = bpm;
            for &p in &peaks {
                last = pll.step(Some(p)).bpm;
            }
            let err = last.abs_diff(bpm);
            prop_assert!(
                err < 50_000,
                "bpm err {} µBPM > 50_000 (last={} µBPM, target={} µBPM, jitter_us={})",
                err, last.0, bpm.0, jitter_us
            );
        }

        #[test]
        fn pll_phase_converges(
            bpm_mbpm in 60_000_000u32..200_000_000,
            jitter_us in 0u32..200,
            seed in any::<u64>(),
        ) {
            let bpm = Tempo(bpm_mbpm);
            let jitter = FD12FD06.inner(FD06(jitter_us as i64));
            let ppq = 24u32;
            let n_pulses = 64u32;
            let (_, peaks): (Vec<f32>, Vec<S048>) =
                pulse_train::<S048>(bpm, ppq, jitter, n_pulses, seed);
            let mut pll = Pll::<S048>::new(PllSettings::DEFAULT, bpm, ppq);
            // True pulse spacing in seconds (for error computation only;
            // test-local f64).
            let true_spacing_secs = 1.0 / tempo_to_hz(bpm, ppq);
            let mut sum_sq_us = 0.0_f64;
            let mut count = 0;
            for (i, &p) in peaks.iter().enumerate() {
                let out = pll.step(Some(p));
                if i >= 32 {
                    let est_spacing_secs = 1.0 / tempo_to_hz(out.bpm, ppq);
                    let err_us = (est_spacing_secs - true_spacing_secs) * 1e6;
                    sum_sq_us += err_us * err_us;
                    count += 1;
                }
            }
            let rms_us = (sum_sq_us / count as f64).sqrt();
            prop_assert!(
                rms_us < 50.0,
                "phase rms {} µs > 50 µs (bpm={}, jitter_us={})",
                rms_us, bpm.0, jitter_us
            );
        }

        #[test]
        fn pll_rejects_outliers(
            bpm_mbpm in 90_000_000u32..160_000_000,
            seed in any::<u64>(),
        ) {
            let bpm = Tempo(bpm_mbpm);
            let ppq = 24u32;
            let n_pulses = 96u32;
            let jitter = Pico(50_000_000); // 50 µs
            let (_, mut peaks): (Vec<f32>, Vec<S048>) =
                pulse_train::<S048>(bpm, ppq, jitter, n_pulses, seed);
            let mut pll = Pll::<S048>::new(PllSettings::DEFAULT, bpm, ppq);
            let mut pre_bpm = bpm;
            for &p in peaks.iter().take(40) {
                pre_bpm = pll.step(Some(p)).bpm;
            }
            // 10×σ = 500 µs in samples at S048. Spike the 40th peak.
            let spike_samples = 500.0 * S048::HZ as f64 / 1e6;
            let spike_bits = (spike_samples * 65_536.0).round() as i64;
            peaks[40] = S048::from_bits_q48_16(peaks[40].to_bits_q48_16() + spike_bits);
            let mut last_post = pre_bpm;
            for &p in &peaks[40..] {
                last_post = pll.step(Some(p)).bpm;
            }
            let drift = last_post.abs_diff(pre_bpm);
            // 0.5 BPM = 500_000 µBPM.
            prop_assert!(
                drift < 500_000,
                "post-spike drift {} µBPM > 500_000 (pre={}, post={}, bpm={})",
                drift, pre_bpm.0, last_post.0, bpm.0
            );
        }

        #[test]
        fn pll_no_panic_on_silence(
            bpm_mbpm in 60_000_000u32..200_000_000,
        ) {
            let bpm = Tempo(bpm_mbpm);
            let mut pll = Pll::<S048>::new(PllSettings::DEFAULT, bpm, 24);
            for _ in 0..1000 {
                let _ = pll.step(None);
                // Phase and bpm are integer types — no NaN to worry about.
                // Just assert no panic; integer invariants hold trivially.
            }
        }
    }

    #[test]
    fn pll_bandwidth_monotone() {
        let pulses_to_settle = |omega_n: f64| -> usize {
            let kp = 2.0 * omega_n;
            let ki = omega_n * omega_n;
            let cfg = PllSettings {
                kp,
                ki,
                clamp_hz: 1000.0,
                interp: 0.0,
            };
            let nominal = Tempo::from_bpm_integer(120);
            let actual = Tempo::from_bpm_integer(130);
            let ppq = 24u32;
            let (_, peaks): (Vec<f32>, Vec<S048>) =
                pulse_train::<S048>(actual, ppq, Pico(0), 800, 1);
            let mut pll = Pll::<S048>::new(cfg, nominal, ppq);
            for (i, &p) in peaks.iter().enumerate() {
                let out = pll.step(Some(p));
                let err = out.bpm.abs_diff(actual);
                if err < 500_000 {
                    // 0.5 BPM
                    return i;
                }
            }
            usize::MAX
        };
        let n_slow = pulses_to_settle(0.05);
        let n_fast = pulses_to_settle(0.15);
        assert!(
            n_fast < n_slow,
            "expected higher bandwidth to settle faster: ω_n=0.15→{} pulses vs ω_n=0.05→{}",
            n_fast,
            n_slow
        );
    }
}
