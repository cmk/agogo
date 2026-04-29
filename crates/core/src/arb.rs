//! Shared proptest strategies and synthetic test signals.

use crate::boundary::{pico_to_f64_seconds, tempo_to_f64_bpm};
use crate::time::decimal::Pico;
use crate::time::sample::{SampleRate, SampleTime};
use crate::time::tempo::Tempo;
use rand::SeedableRng;
use rand_distr::{Distribution, Normal};

/// Width of a synthetic pulse, in picoseconds. Matches the design brief's
/// "~1.5 ms wide" target (1 500 000 ns = 1 500 000 000 ps).
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
pub fn pulse_train<R: SampleTime>(
    bpm: Tempo,
    ppq: u32,
    jitter_sigma: Pico,
    n_pulses: u32,
    seed: u64,
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
    let sr = R::HZ;
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
        peaks.push(R::from_bits_q48_16(centre_bits));

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

// ---------------------------------------------------------------------
// Proptest strategies (testkit-gated).
// ---------------------------------------------------------------------

#[cfg(any(test, feature = "testkit"))]
mod strategies {
    use crate::time::decimal::Pico;
    use crate::time::tempo::Tempo;
    use num_rational::Rational64;
    use proptest::prelude::*;

    use crate::time::grid::Grid;
    use crate::time::swing::SwingConfig;
    use crate::time::tbase::TBase;
    use crate::time::tick::{Tick, Time};

    /// BPM strategy as `Tempo` (BPM × 10⁶). Biased toward common
    /// musical tempos with some boundary spice.
    pub fn arb_bpm() -> impl Strategy<Value = Tempo> {
        prop_oneof![
            1 => Just(Tempo::from_bpm_integer(60)),
            1 => Just(Tempo::from_bpm_integer(120)),
            1 => Just(Tempo::from_bpm_integer(200)),
            5 => (60_000_000u32..200_000_000).prop_map(Tempo),
            1 => (30_000_000u32..400_000_000).prop_map(Tempo),
        ]
    }

    /// Sample rate strategy: standard audio rates only. (u32 so it can
    /// be used by callers that pick a rate type at the callsite; the
    /// typed variants S044/S048/... expose the same values via
    /// `SampleRate::HZ`.)
    pub fn arb_sample_rate() -> impl Strategy<Value = u32> {
        prop_oneof![
            Just(44_100u32),
            Just(48_000u32),
            Just(96_000u32),
            Just(192_000u32),
        ]
    }

    /// Jitter σ as `Pico`. Heavy bias toward small values so the PLL
    /// convergence properties usually fire on inputs they can lock to.
    pub fn arb_jitter_sigma() -> impl Strategy<Value = Pico> {
        prop_oneof![
            1 => Just(Pico(0)),
            5 => (0i64..50_000_000).prop_map(Pico),          // 0..50 µs in ps
            2 => (50_000_000i64..200_000_000).prop_map(Pico),
            1 => (200_000_000i64..500_000_000).prop_map(Pico),
        ]
    }

    /// Binary subdivision strategy (9 variants). Used wherever a
    /// `TBase`-typed value is required — most prominently
    /// `SwingConfig.resolution`.
    pub fn arb_tbase() -> impl Strategy<Value = TBase> {
        prop_oneof![
            1 => Just(TBase::T1),
            1 => Just(TBase::T256),
            4 => prop::sample::select(TBase::ALL.as_slice()),
        ]
    }

    /// Full 36-element Grid lattice strategy. Used wherever the
    /// channel divider, `quantize_at` argument, or `Time::base`
    /// crosses the test surface.
    pub fn arb_grid() -> impl Strategy<Value = Grid> {
        prop_oneof![
            1 => Just(Grid::T1),
            1 => Just(Grid::T512P),
            4 => prop::sample::select(Grid::ALL.as_slice()),
        ]
    }

    /// Per CLAUDE.md's full-domain rule, named boundaries (0,
    /// `Grid::T512P` = 1, `Grid::T1` = 3840 ticks per bar, the
    /// `from_ticks` horizon at `u32::MAX × Grid::T1.tick_count()`)
    /// get explicit `Just` arms with elevated frequency. The 4-weighted
    /// uniform arm covers the 0..=1M range where most musically-
    /// meaningful tick values live; the upper bound is the largest
    /// `Tick` for which [`from_ticks`](crate::time::tick::from_ticks)
    /// returns `Some(_)`, so `arb_tick` never produces a value the
    /// [`TICKTIME`](crate::time::conn::TICKTIME) Conn cannot
    /// canonicalise.
    pub fn arb_tick() -> impl Strategy<Value = Tick> {
        let horizon: u64 = u64::from(u32::MAX) * u64::from(Grid::T1.tick_count());
        prop_oneof![
            1 => Just(Tick(0)),
            1 => Just(Tick(u64::from(Grid::T512P.tick_count()))),
            1 => Just(Tick(u64::from(Grid::T1.tick_count()))),
            1 => Just(Tick(horizon)),
            4 => (0u64..=1_000_000).prop_map(Tick),
        ]
    }

    pub fn arb_time() -> impl Strategy<Value = Time> {
        (any::<u32>(), arb_grid()).prop_map(|(beats, base)| Time { beats, base })
    }

    pub fn arb_small_time() -> impl Strategy<Value = Time> {
        (0u32..=50, arb_grid()).prop_map(|(beats, base)| Time { beats, base })
    }

    pub fn arb_rational_nonneg() -> impl Strategy<Value = Rational64> {
        prop_oneof![
            1 => Just(Rational64::new(0, 1)),
            1 => Just(Rational64::new(1, 4)),
            1 => Just(Rational64::new(1, 1)),
            4 => (0i64..=10_000, 1i64..=3840).prop_map(|(n, d)| Rational64::new(n, d)),
        ]
    }

    /// Swing strategy. `amount` ranges over `i8` with bias toward
    /// musically-meaningful magnitudes (0, MPC full-shuffle ±80,
    /// Linn ±40); `resolution` ranges over the binary chain.
    pub fn arb_swing() -> impl Strategy<Value = SwingConfig> {
        prop_oneof![
            1 => Just(SwingConfig { resolution: TBase::T16, amount: 0 }),
            1 => Just(SwingConfig { resolution: TBase::T16, amount: 80 }),
            1 => Just(SwingConfig { resolution: TBase::T16, amount: 40 }),
            1 => Just(SwingConfig { resolution: TBase::T16, amount: -40 }),
            1 => Just(SwingConfig { resolution: TBase::T8, amount: 0 }),
            5 => (arb_tbase(), -120i8..=120)
                 .prop_map(|(resolution, amount)| SwingConfig { resolution, amount }),
            1 => (arb_tbase(), any::<i8>())
                 .prop_map(|(resolution, amount)| SwingConfig { resolution, amount }),
        ]
    }
}

#[cfg(any(test, feature = "testkit"))]
pub use strategies::{
    arb_bpm, arb_grid, arb_jitter_sigma, arb_rational_nonneg, arb_sample_rate, arb_small_time,
    arb_swing, arb_tbase, arb_tick, arb_time,
};

// Fallback to satisfy the unused-trait import on non-testkit builds.
#[allow(dead_code)]
fn _sample_rate_sealed() -> u32 {
    S048_HZ
}
const S048_HZ: u32 = <crate::time::sample::S048 as SampleRate>::HZ;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::sample::S048;
    use proptest::prelude::*;

    #[test]
    fn pulse_train_shape_basic() {
        let bpm = Tempo::from_bpm_integer(120);
        let (samples, peaks): (Vec<f32>, Vec<S048>) = pulse_train::<S048>(bpm, 24, Pico(0), 4, 1);
        assert_eq!(peaks.len(), 4);
        // 120 BPM × 24 PPQ = 48 pps → 1000 samples between pulses at 48 kHz.
        let expected_spacing_bits = 1000i64 << 16;
        for w in peaks.windows(2) {
            let diff = w[1].to_bits_q48_16() - w[0].to_bits_q48_16();
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
        let (samples, peaks): (Vec<f32>, Vec<S048>) =
            pulse_train::<S048>(Tempo::from_bpm_integer(120), 24, Pico(0), 0, 0);
        assert!(samples.is_empty());
        assert!(peaks.is_empty());
    }

    #[test]
    fn pulse_train_is_deterministic_in_seed() {
        let bpm = Tempo::from_bpm_integer(140);
        let jitter = Pico(100_000_000); // 100 µs
        let a: (Vec<f32>, Vec<S048>) = pulse_train::<S048>(bpm, 24, jitter, 8, 42);
        let b: (Vec<f32>, Vec<S048>) = pulse_train::<S048>(bpm, 24, jitter, 8, 42);
        assert_eq!(a.0, b.0);
        assert_eq!(a.1, b.1);
    }

    proptest! {
        #[test]
        fn arb_bpm_in_range(bpm in arb_bpm()) {
            prop_assert!((30_000_000..=400_000_000).contains(&bpm.0));
        }

        #[test]
        fn arb_sample_rate_is_standard(sr in arb_sample_rate()) {
            prop_assert!(matches!(sr, 44_100 | 48_000 | 96_000 | 192_000));
        }

        #[test]
        fn arb_jitter_in_range(j in arb_jitter_sigma()) {
            prop_assert!((0..=500_000_000).contains(&j.0));
        }
    }
}
