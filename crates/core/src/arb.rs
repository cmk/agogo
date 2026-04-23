//! Shared proptest strategies and synthetic test signals.
//!
//! Two kinds of items live here:
//!
//! - **Strategies** (`arb_bpm`, `arb_sample_rate`, `arb_jitter_sigma_us`,
//!   `arb_tbase`, `arb_tick`, `arb_time`, `arb_small_time`,
//!   `arb_rational_nonneg`, `arb_swing`) return
//!   `impl proptest::strategy::Strategy<...>` and are gated behind
//!   `#[cfg(any(test, feature = "testkit"))]` so production builds
//!   don't pull in proptest. Downstream crates that want them in their
//!   own tests should depend on `agogo-core` with the `testkit`
//!   feature.
//! - **Synthetic generators** (`pulse_train`) are pure functions of
//!   `(params, seed)` and ship unconditionally so non-test code (e.g.
//!   the CLI) can use the same fixture as the proptests.
//!
//! Define strategies as functions returning `impl Strategy<Value = T>`,
//! not via `Arbitrary` derive. Use `prop_oneof!` with frequency weights
//! to bias toward boundary values and edge cases.

// ---------------------------------------------------------------------
// Synthetic pulse train (always available).
// ---------------------------------------------------------------------
use num_rational::Rational64;
use proptest::prelude::*;

/// Width of a synthetic pulse, in seconds. Matches the design brief's
/// "~1.5 ms wide" target — wide enough to span several samples at every
/// sample rate we test, narrow enough that pulses don't overlap at the
/// fastest reasonable BPM.
pub const PULSE_WIDTH_SECS: f64 = 0.0015;

/// Generate a synthetic audio buffer containing `n_pulses` Hann-bell
/// pulses arriving at the given BPM/PPQ, with optional Gaussian timing
/// jitter, and return both the buffer and the ground-truth pulse-centre
/// positions (post-jitter, in samples).
///
/// Pulses are Hann bells `0.5 * (1 - cos(2πt/W))` across a window of
/// `W = sr * PULSE_WIDTH_SECS` samples. (The plan called for triangles;
/// Hann bells share the width but have a smooth apex, which is
/// necessary for parabolic sub-sample interpolation to converge —
/// see the sprint Review for details.)
///
/// `seed` deterministically seeds an internal xorshift64 PRNG used to
/// draw Box-Muller Gaussian timing offsets. Passing `seed = 0` selects
/// a fixed non-zero fallback.
///
/// # Panics
/// Panics if `bpm <= 0`, `sr == 0`, `ppq == 0`, or `jitter_sigma_us < 0`.
pub fn pulse_train(
    bpm: f32,
    sr: u32,
    ppq: u32,
    jitter_sigma_us: f32,
    n_pulses: u32,
    seed: u64,
) -> (Vec<f32>, Vec<f64>) {
    assert!(bpm > 0.0, "bpm must be positive, got {bpm}");
    assert!(sr > 0, "sample rate must be positive");
    assert!(ppq > 0, "ppq must be positive");
    assert!(
        jitter_sigma_us >= 0.0,
        "jitter sigma must be non-negative, got {jitter_sigma_us}"
    );

    if n_pulses == 0 {
        return (Vec::new(), Vec::new());
    }

    let pulse_rate_hz = bpm as f64 * ppq as f64 / 60.0;
    let spacing_samples = sr as f64 / pulse_rate_hz;
    let width_samples = (sr as f64 * PULSE_WIDTH_SECS).max(4.0);
    let half_width = width_samples * 0.5;
    let sigma_samples = jitter_sigma_us as f64 * sr as f64 / 1e6;

    // Reserve enough buffer for the last (jittered) pulse plus its tail.
    let last_nominal = spacing_samples * n_pulses as f64;
    // Tail headroom: enough for late-jittered last pulse + parabolic
    // interp window + a few extra samples. Generous on purpose so
    // detector callers can pass `samples` straight through without
    // worrying about end-of-buffer truncation.
    let pad = half_width + 6.0 * sigma_samples + 256.0;
    let total_len = (last_nominal + pad).ceil() as usize;
    let mut samples = vec![0.0_f32; total_len];
    let mut peaks = Vec::with_capacity(n_pulses as usize);

    let mut rng = if seed == 0 { 0xdead_beef_cafe_babe } else { seed };

    for i in 1..=n_pulses {
        let nominal_centre = spacing_samples * i as f64;
        let jitter = if sigma_samples > 0.0 {
            next_gaussian(&mut rng) * sigma_samples
        } else {
            0.0
        };
        let centre = nominal_centre + jitter;
        peaks.push(centre);

        let start = ((centre - half_width).floor() as i64).max(0) as usize;
        let end = ((centre + half_width).ceil() as i64).max(0) as usize;
        for n in start..=end.min(samples.len().saturating_sub(1)) {
            let dx = n as f64 - centre;
            if dx.abs() > half_width {
                continue;
            }
            // Hann bell: 0.5 * (1 - cos(2π * (dx + half_width) / W))
            //         = sin²(π * (dx + half_width) / W)
            // Simplify centred form: cos²(π * dx / W).
            let v = (std::f64::consts::PI * dx / width_samples).cos();
            samples[n] += (v * v) as f32;
        }
    }

    (samples, peaks)
}

#[inline]
fn xorshift64(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}

#[inline]
fn next_uniform(state: &mut u64) -> f64 {
    // Top 53 bits → uniform in [0, 1).
    let bits = xorshift64(state) >> 11;
    bits as f64 * (1.0 / ((1u64 << 53) as f64))
}

#[inline]
fn next_gaussian(state: &mut u64) -> f64 {
    // Single-sample Box-Muller. Discarding the second draw is wasteful
    // but keeps the helper stateless from the caller's perspective.
    let u1 = next_uniform(state).max(f64::MIN_POSITIVE);
    let u2 = next_uniform(state);
    (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
}

// ---------------------------------------------------------------------
// Proptest strategies (testkit-gated).
// ---------------------------------------------------------------------

#[cfg(any(test, feature = "testkit"))]
mod strategies {
    use num_rational::Rational64;
    use proptest::prelude::*;

    use crate::time::swing::SwingConfig;
    use crate::time::tbase::TBase;
    use crate::time::tick::{Tick, Time};

    /// BPM strategy biased toward common musical tempos with some
    /// boundary spice.
    pub fn arb_bpm() -> impl Strategy<Value = f32> {
        prop_oneof![
            1 => Just(60.0_f32),
            1 => Just(120.0_f32),
            1 => Just(200.0_f32),
            5 => 60.0_f32..200.0_f32,
            1 => 30.0_f32..400.0_f32,
        ]
    }

    /// Sample rate strategy: standard audio rates only.
    pub fn arb_sample_rate() -> impl Strategy<Value = u32> {
        prop_oneof![
            Just(44_100u32),
            Just(48_000u32),
            Just(96_000u32),
            Just(192_000u32),
        ]
    }

    /// Jitter σ in microseconds. Heavy bias toward small values so the
    /// PLL convergence properties usually fire on inputs they can lock
    /// to, with rarer excursions toward stress-test territory.
    pub fn arb_jitter_sigma_us() -> impl Strategy<Value = f32> {
        prop_oneof![
            1 => Just(0.0_f32),
            5 => 0.0_f32..50.0_f32,
            2 => 50.0_f32..200.0_f32,
            1 => 200.0_f32..500.0_f32,
        ]
    }

    /// Strategy over all 14 `TBase` variants.
    ///
    /// Biased toward the lattice top (`T1`) and bottom (`T128t`) so
    /// property tests exercising divisibility, join, and meet see
    /// boundary elements regularly. The uniform-sample arm covers the
    /// remaining middle of the lattice.
    pub fn arb_tbase() -> impl Strategy<Value = TBase> {
        prop_oneof![
            1 => Just(TBase::T1),
            1 => Just(TBase::T128t),
            4 => prop::sample::select(TBase::ALL.as_slice()),
        ]
    }

    /// Strategy over `Tick` values. Bounded at 1M so that downstream
    /// arithmetic — including `beats × tick_count` where
    /// `tick_count ≤ 768` — stays well inside `u32`. 1M ticks ≈ 1300
    /// whole notes ≈ 325 bars at 4/4, plenty of musical range for the
    /// property tests.
    pub fn arb_tick() -> impl Strategy<Value = Tick> {
        prop_oneof![
            1 => Just(Tick(0)),
            1 => Just(Tick(TBase::T128t.tick_count())),
            1 => Just(Tick(TBase::T1.tick_count())),
            4 => (0u32..=1_000_000).prop_map(Tick),
        ]
    }

    /// Strategy over `Time` values. Beats bounded at 100K so the tick
    /// count stays inside `u32` for every `TBase` (max product ≈ 77M).
    pub fn arb_time() -> impl Strategy<Value = Time> {
        (0u32..=100_000, arb_tbase()).prop_map(|(beats, base)| Time { beats, base })
    }

    /// Narrower `Time` strategy for lattice tests. Beats bounded at 50
    /// so LCM of any two tick counts stays inside `u32` (max tick
    /// count ≈ 38400, LCM ≤ 1.47e9 ≪ u32::MAX).
    pub fn arb_small_time() -> impl Strategy<Value = Time> {
        (0u32..=50, arb_tbase()).prop_map(|(beats, base)| Time { beats, base })
    }

    /// Non-negative rational whole-note duration for `rat_tick` tests.
    /// Numerator ≤ 10_000 and denominator ∈ [1, 768] keeps the product
    /// `r * 768` well inside `i64` for ceil/floor conversions.
    pub fn arb_rational_nonneg() -> impl Strategy<Value = Rational64> {
        prop_oneof![
            1 => Just(Rational64::new(0, 1)),
            1 => Just(Rational64::new(1, 4)),  // quarter note
            1 => Just(Rational64::new(1, 1)),  // whole note
            4 => (0i64..=10_000, 1i64..=768).prop_map(|(n, d)| Rational64::new(n, d)),
        ]
    }

    /// Strategy over `SwingConfig`. Biased toward boundary values:
    /// `amount = 0` (no swing), `amount = 16` (Cirklon maximum), and
    /// `multiplier = 1` (the finest unit). The sampled arm covers
    /// signed ranges so negative displacements are exercised too.
    pub fn arb_swing() -> impl Strategy<Value = SwingConfig> {
        prop_oneof![
            1 => Just(SwingConfig { amount: 0, multiplier: 1 }),
            1 => Just(SwingConfig { amount: 16, multiplier: 1 }),
            1 => Just(SwingConfig { amount: 8, multiplier: 2 }),
            4 => (-16i32..=16, 1i32..=16)
                 .prop_map(|(amount, multiplier)| SwingConfig { amount, multiplier }),
        ]
    }
}

#[cfg(any(test, feature = "testkit"))]
pub use strategies::{
    arb_bpm, arb_jitter_sigma_us, arb_rational_nonneg, arb_sample_rate, arb_small_time, arb_swing,
    arb_tbase, arb_tick, arb_time,
};

// ---------------------------------------------------------------------
// Self-tests for the synthetic generator.
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn pulse_train_shape_basic() {
        let (samples, peaks) = pulse_train(120.0, 48_000, 24, 0.0, 4, 1);
        assert_eq!(peaks.len(), 4);
        // 120 BPM × 24 PPQ = 48 pps → 1000 samples between pulses at 48 kHz.
        let expected_spacing = 48_000.0 / (120.0 * 24.0 / 60.0);
        for w in peaks.windows(2) {
            assert!((w[1] - w[0] - expected_spacing).abs() < 1e-6);
        }
        // Every centre sits inside the buffer.
        let last = *peaks.last().unwrap();
        assert!(last + 100.0 < samples.len() as f64);
        // Hann apex amplitude should be ≈ 1.0 at the integer nearest each centre.
        for c in &peaks {
            let n = c.round() as usize;
            assert!(samples[n] > 0.95, "amp at {n} = {}", samples[n]);
        }
    }

    #[test]
    fn pulse_train_zero_pulses_is_empty() {
        let (samples, peaks) = pulse_train(120.0, 48_000, 24, 0.0, 0, 0);
        assert!(samples.is_empty());
        assert!(peaks.is_empty());
    }

    #[test]
    fn pulse_train_is_deterministic_in_seed() {
        let a = pulse_train(140.0, 48_000, 24, 100.0, 8, 42);
        let b = pulse_train(140.0, 48_000, 24, 100.0, 8, 42);
        assert_eq!(a.0, b.0);
        assert_eq!(a.1, b.1);
    }

    proptest! {
        // Smoke: strategies produce values in their declared ranges.
        #[test]
        fn arb_bpm_in_range(bpm in arb_bpm()) {
            prop_assert!((30.0..=400.0).contains(&bpm));
        }

        #[test]
        fn arb_sample_rate_is_standard(sr in arb_sample_rate()) {
            prop_assert!(matches!(sr, 44_100 | 48_000 | 96_000 | 192_000));
        }

        #[test]
        fn arb_jitter_in_range(j in arb_jitter_sigma_us()) {
            prop_assert!((0.0..=500.0).contains(&j));
        }
    }
}

/// Narrower `Time` strategy for lattice tests. Beats bounded at 50 so
/// that LCM of any two tick counts stays inside `u32` (max tick count
/// ≈ 38400, LCM ≤ 1.47e9 ≪ u32::MAX).
pub fn arb_small_time() -> impl Strategy<Value = Time> {
    (0u32..=50, arb_tbase()).prop_map(|(beats, base)| Time { beats, base })
}

/// Non-negative rational whole-note duration for `rat_tick` tests.
/// Numerator ≤ 10_000 and denominator ∈ [1, 768] keeps the product
/// `r * 768` well inside `i64` for ceil/floor conversions.
pub fn arb_rational_nonneg() -> impl Strategy<Value = Rational64> {
    prop_oneof![
        1 => Just(Rational64::new(0, 1)),
        1 => Just(Rational64::new(1, 4)),  // quarter note
        1 => Just(Rational64::new(1, 1)),  // whole note
        4 => (0i64..=10_000, 1i64..=768).prop_map(|(n, d)| Rational64::new(n, d)),
    ]
}
