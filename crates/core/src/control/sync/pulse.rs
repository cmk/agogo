//! Synthetic Hann-bell pulse-train signal generator.
//!
//! Used by `cli/sync_trace` to feed the sync detector with
//! deterministic test signals, and by [`crate::control::sync::pll`] /
//! [`crate::control::sync::detect`] tests to drive the PLL and peak detector
//! against ground-truth pulse positions.
//!
//! Not testkit-gated — the `pulse_train_sxxx` functions are runtime APIs, not
//! proptest strategy. It lives in `sync/` because every consumer
//! sits in the sync subsystem.

use crate::conn::boundary::{pico_to_f64_seconds, tempo_to_f64_bpm};
use crate::conn::fixed::Pico;
use crate::conn::sample::{S044, S048, S088, S096, S176, S192, SampleRate};
use crate::conn::tempo::Tempo;
use rand::SeedableRng;
use rand_distr::{Distribution, Normal};

/// Width of a synthetic pulse, in picoseconds. Matches the design
/// brief's "~1.5 ms wide" target (1 500 000 ns = 1 500 000 000 ps).
pub const PULSE_WIDTH_PS: Pico = Pico(1_500_000_000);

/// Generate a synthetic audio buffer containing `n_pulses` Hann-bell
/// pulses arriving at the given BPM/PPQ, with optional Gaussian timing
/// jitter, and return both the buffer and the ground-truth pulse-centre
/// positions (post-jitter) in the target rate's Q48.16.
///
/// Pulses are Hann bells `cos²(π · dx / W)` across a window of
/// `W = R::HZ · PULSE_WIDTH_PS / 10¹²` samples.
///
/// `seed` deterministically seeds a PCG-64 PRNG used to draw Gaussian
/// timing offsets via `rand_distr::Normal`. Passing `seed = 0` selects
/// a fixed non-zero fallback.
///
/// # Panics
/// Panics if `bpm == 0`, `ppq == 0`, or `jitter_sigma.0 < 0`.
fn pulse_train_with<R>(
    bpm: Tempo,
    ppq: u32,
    jitter_sigma: Pico,
    n_pulses: u32,
    seed: u64,
    sr: u32,
    from_bits: fn(i64) -> R,
) -> (Vec<f32>, Vec<R>) {
    assert!(bpm.0 > 0, "bpm must be positive, got {:?}", bpm);
    assert!(ppq > 0, "ppq must be positive");
    assert!(
        jitter_sigma.0 >= 0,
        "jitter sigma must be non-negative, got {:?}",
        jitter_sigma
    );

    if n_pulses == 0 {
        return (Vec::new(), Vec::new());
    }

    // Test-fixture f64 derived quantities. These don't escape this
    // function; the returned peaks are already in Q48.16. All
    // unit-shift conversions go through the lawful Conn-inverse
    // helpers (`tempo_to_f64_bpm` and `pico_to_f64_seconds`),
    // not open-coded `× 10⁻⁶` / `× 10⁻¹²`.
    let bpm_f = tempo_to_f64_bpm(bpm);
    let pulse_rate_hz = bpm_f * ppq as f64 / 60.0;
    let spacing_samples = sr as f64 / pulse_rate_hz;
    let width_samples = (sr as f64 * pico_to_f64_seconds(PULSE_WIDTH_PS)).max(4.0);
    let half_width = width_samples * 0.5;
    // σ in samples: σ-seconds × sr (Conn-inverse for the Pico → f64).
    let sigma_samples = pico_to_f64_seconds(jitter_sigma) * sr as f64;

    let last_nominal = spacing_samples * n_pulses as f64;
    let pad = half_width + 6.0 * sigma_samples + 256.0;
    let total_len = (last_nominal + pad).ceil() as usize;
    let mut samples = vec![0.0_f32; total_len];
    let mut peaks = Vec::with_capacity(n_pulses as usize);

    let seed = if seed == 0 {
        0xdead_beef_cafe_babe
    } else {
        seed
    };
    let mut rng = rand_pcg::Pcg64::seed_from_u64(seed);
    let normal = if sigma_samples > 0.0 {
        Some(Normal::new(0.0_f64, sigma_samples).expect("finite sigma"))
    } else {
        None
    };

    for i in 1..=n_pulses {
        let nominal_centre = spacing_samples * i as f64;
        let jitter = normal.as_ref().map(|n| n.sample(&mut rng)).unwrap_or(0.0);
        let centre = nominal_centre + jitter;

        // Q48.16 bits of the (possibly fractional) centre position.
        let centre_bits = (centre * 65_536.0).round() as i64;
        peaks.push(from_bits(centre_bits));

        // Waveform synthesis still writes into f32 PCM (cpal ABI).
        let start = ((centre - half_width).floor() as i64).max(0) as usize;
        let end = ((centre + half_width).ceil() as i64).max(0) as usize;
        for n in start..=end.min(samples.len().saturating_sub(1)) {
            let dx = n as f64 - centre;
            if dx.abs() > half_width {
                continue;
            }
            let v = (std::f64::consts::PI * dx / width_samples).cos();
            samples[n] += (v * v) as f32;
        }
    }

    (samples, peaks)
}

macro_rules! pulse_train_rate {
    ($func:ident, $Rate:ident) => {
        /// Generate a synthetic audio buffer containing `n_pulses` Hann-bell
        /// pulses at the given BPM/PPQ.
        ///
        /// Returns the PCM buffer and the ground-truth pulse-centre positions
        /// after jitter, represented as Q48.16 values in this function's
        /// concrete sample-rate type.
        ///
        /// `jitter_sigma` is a non-negative Gaussian timing standard deviation
        /// in picoseconds. `seed` deterministically seeds the PRNG; passing
        /// `seed = 0` selects a fixed non-zero fallback.
        ///
        /// # Panics
        ///
        /// Panics if `bpm == 0`, `ppq == 0`, or `jitter_sigma.0 < 0`.
        pub fn $func(
            bpm: Tempo,
            ppq: u32,
            jitter_sigma: Pico,
            n_pulses: u32,
            seed: u64,
        ) -> (Vec<f32>, Vec<$Rate>) {
            pulse_train_with(
                bpm,
                ppq,
                jitter_sigma,
                n_pulses,
                seed,
                $Rate::HZ,
                $Rate::from_bits,
            )
        }
    };
}

pulse_train_rate!(pulse_train_s044, S044);
pulse_train_rate!(pulse_train_s048, S048);
pulse_train_rate!(pulse_train_s088, S088);
pulse_train_rate!(pulse_train_s096, S096);
pulse_train_rate!(pulse_train_s176, S176);
pulse_train_rate!(pulse_train_s192, S192);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pulse_train_shape_basic() {
        let bpm = Tempo::from_bpm_integer(120);
        let (samples, peaks) = pulse_train_s048(bpm, 24, Pico(0), 4, 1);
        assert_eq!(peaks.len(), 4);
        // 120 BPM × 24 PPQ = 48 pps → 1000 samples between pulses at 48 kHz.
        let expected_spacing_bits = 1000i64 << 16;
        for w in peaks.windows(2) {
            let diff = w[1].to_bits() - w[0].to_bits();
            assert!((diff - expected_spacing_bits).abs() < 1);
        }
        let last_samples = peaks.last().unwrap().sample();
        assert!((last_samples as u64) + 100 < samples.len() as u64);
        // Hann apex amplitude should be ≈ 1.0 at the integer nearest each centre.
        for c in &peaks {
            let n = c.sample() as usize;
            assert!(samples[n] > 0.95, "amp at {n} = {}", samples[n]);
        }
    }

    #[test]
    fn pulse_train_zero_pulses_is_empty() {
        let (samples, peaks) = pulse_train_s048(Tempo::from_bpm_integer(120), 24, Pico(0), 0, 0);
        assert!(samples.is_empty());
        assert!(peaks.is_empty());
    }

    #[test]
    fn pulse_train_is_deterministic_in_seed() {
        let bpm = Tempo::from_bpm_integer(140);
        let jitter = Pico(100_000_000); // 100 µs
        let a = pulse_train_s048(bpm, 24, jitter, 8, 42);
        let b = pulse_train_s048(bpm, 24, jitter, 8, 42);
        assert_eq!(a.0, b.0);
        assert_eq!(a.1, b.1);
    }
}
