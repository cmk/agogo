//! Fixed-point arithmetic for agogo DSP.
//!
//! Re-exports the decimal time ladder (`FD00..FD12`) and the
//! rate-typed sample tier (`S044 / S048 / S088 / S096 / S176 /
//! S192`) from agogo's own vendored modules at
//! [`crate::time::{decimal, sample}`]. Adds two agogo-local
//! fixed-point types — `Phase` (Q0.32 cycles, wrapping-add =
//! modular reduction) and `Tempo` (BPM × 10⁶) — plus integer
//! `linear_u8` / `smoothstep_u8` primitives and a handful of
//! narrow f32/f64 → fxp conversions for the CLI-parser and
//! PI-controller boundaries.
//!
//! Two intentional **domain aliases** for time-unit readability
//! at the FFI seams:
//! - [`Micro`] = [`FD06`] — used in `host-link`, `channel::scheduler`,
//!   `Quantum(Micro)`, and `ChannelCommon::{delay, offset}`.
//! - [`Pico`] = [`FD12`] — used in `pico_to_samples`, the cpal seam,
//!   `arb::pulse_train`, `sync::pll` jitter math.
//!
//! Use the canonical `FDxx` / `Sxxx` names everywhere else.
//!
//! Everything else in the workspace should consume these types
//! rather than `f32`/`f64` directly, with the exceptions documented
//! in each module:
//!
//! - PCM sample slices (`&[f32]`): the cpal audio I/O ABI.
//! - PI controller internals in `sync::pll` (`PllSettings.kp`,
//!   `PllState.freq_hz`, etc): genuine analog-DSP arithmetic.
//! - Short-lived arithmetic locals for parabolic fit and PI law:
//!   contained, never stored, converted to fxp at the first
//!   exit boundary.

// Float-boundary types (`ExtendedFloat<f64>`, `Extended<T>`) still
// come from the connections crate — they're the algebra primitives
// the time tier is built on.
pub use connections::conn::float::ExtendedFloat;
pub use connections::extended::Extended;

// Time tier (decimal SI ladder + sample-indexed Q48.16) is now
// vendored under `crate::time::{decimal, sample}`. Re-export the
// agogo-local types under the same names every workspace caller
// already uses.
pub use crate::time::decimal::{
    F064FD00, F064FD01, F064FD02, F064FD03, F064FD06, F064FD09, F064FD12, FD00, FD01, FD02, FD03,
    FD06, FD09, FD12, FD12FD00, FD12FD03, FD12FD06, FD12FD09, HasResolution,
};
pub use crate::time::sample::{
    FD12S044, FD12S048, FD12S088, FD12S096, FD12S176, FD12S192, Q48_16, S044, S048, S088, S096,
    S176, S192, SampleRate,
};

// ────────────────────────────────────────────────────────────────────
// Domain aliases.
//
// Two time-unit words kept alongside the canonical FD06 / FD12
// names because they read more naturally at FFI seams (host-link
// session arming, channel::scheduler delay/offset arithmetic, the
// cpal seam, jitter math). Every other workspace site uses the
// canonical FDxx / Sxxx names directly.
// ────────────────────────────────────────────────────────────────────

/// Domain alias for µs. Used at FFI seams: `Quantum(Micro)`,
/// `host-link::session`, `ChannelCommon::{delay, offset}`,
/// `channel::scheduler` arithmetic.
pub use crate::time::decimal::FD06 as Micro;

/// Domain alias for ps. Used in `pico_to_samples`, the cpal seam,
/// `arb::pulse_train`, `sync::pll` jitter math.
pub use crate::time::decimal::FD12 as Pico;

// ────────────────────────────────────────────────────────────────────
// SampleTime — agogo-local convenience trait over the rate types.
//
// Provides uniform `from_bits` / `to_bits` / `from_sample` / `sample`
// methods so generic code (notably `arb::pulse_train` and `sync::*`)
// can construct and read any rate type without a match arm.
// ────────────────────────────────────────────────────────────────────

/// Common Q48.16-bits interface over the `Sxx` rate types from
/// `connections::conn::sample`. Lets generic DSP code accept an arbitrary
/// `R: SampleTime` rather than committing to a single rate.
pub trait SampleTime: SampleRate + Copy + Default + Ord + core::fmt::Debug {
    /// Construct from raw Q48.16 bits.
    fn from_bits_q48_16(bits: i64) -> Self;
    /// Extract raw Q48.16 bits.
    fn to_bits_q48_16(self) -> i64;

    /// Construct from an integer sample count.
    fn from_sample(n: i64) -> Self {
        Self::from_bits_q48_16(n << 16)
    }

    /// Integer sample part (arithmetic shift, rounds toward −∞ for negatives).
    fn sample(self) -> i64 {
        self.to_bits_q48_16() >> 16
    }
}

macro_rules! impl_sample_time {
    ($Rate:ident) => {
        impl SampleTime for $Rate {
            fn from_bits_q48_16(bits: i64) -> Self {
                <$Rate>::from_bits(bits)
            }
            fn to_bits_q48_16(self) -> i64 {
                self.to_bits()
            }
        }
    };
}

impl_sample_time!(S044);
impl_sample_time!(S048);
impl_sample_time!(S088);
impl_sample_time!(S096);
impl_sample_time!(S176);
impl_sample_time!(S192);

// ────────────────────────────────────────────────────────────────────
// Phase — Q0.32 cycles.
//
// The whole u32 range maps to [0, 1) cycles; wrapping_add IS modular
// reduction, which is the load-bearing property of the NCO hot path.
// ────────────────────────────────────────────────────────────────────

/// A phase in `[0, 1)` cycles as unsigned Q0.32.
#[repr(transparent)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub struct Phase(pub u32);

impl Phase {
    pub const ZERO: Self = Self(0);

    pub const fn wrapping_add(self, rhs: Self) -> Self {
        Self(self.0.wrapping_add(rhs.0))
    }

    pub const fn wrapping_sub(self, rhs: Self) -> Self {
        Self(self.0.wrapping_sub(rhs.0))
    }
}

// ────────────────────────────────────────────────────────────────────
// Tempo — BPM × 10⁶ stored as u32.
//
// Range 0..≈4295 BPM (plenty for music). Resolution 10⁻⁶ BPM, well
// below any human perceptual threshold.
// ────────────────────────────────────────────────────────────────────

/// Beats per minute × 10⁶.
#[repr(transparent)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Tempo(pub u32);

impl Tempo {
    pub const ZERO: Self = Self(0);

    /// Construct from an integer BPM. Panics if `n > 4294` (`n × 10⁶`
    /// overflows `u32`). `checked_mul` avoids the silent release-build
    /// wrap that plain `n * 1_000_000` would produce.
    pub const fn from_bpm_integer(n: u32) -> Self {
        match n.checked_mul(1_000_000) {
            Some(v) => Self(v),
            None => panic!("Tempo::from_bpm_integer: n must be ≤ 4294"),
        }
    }
}

// ────────────────────────────────────────────────────────────────────
// Quantum — Link's quantum as microbeats.
//
// Ableton Link represents quantum internally as `std::int64_t`
// microbeats (see ext/rusty_link/link/include/ableton/link/Beats.hpp).
// Its public `double quantum` ABI converts via `std::llround(q * 1e6)`
// on the first line of every API body. Wrapping a `Micro` (10⁻⁶ rung
// of the decimal ladder, `i64` backing) gives agogo's `Quantum` the
// same integer representation Link's C++ side stores — zero
// disagreement at the FFI boundary.
// ────────────────────────────────────────────────────────────────────

/// Link quantum in microbeats. `Quantum::from_bars(4)` = one bar in
/// 4/4 = 4 000 000 microbeats.
#[repr(transparent)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Hash, Default)]
pub struct Quantum(pub Micro);

impl Quantum {
    pub const ZERO: Self = Self(Micro::ZERO);

    /// Exact integer-bar constructor. Panics if `n × 10⁶` overflows
    /// `i64` (`n > 9.2 × 10¹²`); realistic callers use `n` ≤ 64 or so.
    pub const fn from_bars(n: u32) -> Self {
        match (n as i64).checked_mul(1_000_000) {
            Some(v) => Self(Micro(v)),
            None => panic!("Quantum::from_bars: n × 10⁶ overflows i64"),
        }
    }
}

/// f64 beats → `Quantum`. Rounds identically to Link's own
/// `Beats(double)` constructor (`std::llround(q * 1e6)`) so the two
/// sides agree bit-for-bit at the Link FFI boundary. Non-finite
/// input saturates to `Quantum::ZERO`; finite values preserve their
/// sign and saturate on overflow to `i64::MAX` / `i64::MIN`. A noisy
/// return would force the caller to handle an error at every argv
/// boundary without gain, since non-finite quantum is already a user
/// mistake.
pub fn f64_beats_to_quantum(q: f64) -> Quantum {
    // argv boundary — called from the CLI handler's first lines.
    if !q.is_finite() {
        return Quantum::ZERO;
    }
    let scaled = (q * 1_000_000.0).round();
    if scaled > i64::MAX as f64 {
        return Quantum(Micro(i64::MAX));
    }
    if scaled < i64::MIN as f64 {
        return Quantum(Micro(i64::MIN));
    }
    Quantum(Micro(scaled as i64))
}

// ────────────────────────────────────────────────────────────────────
// Boundary conversions.
//
// `f64_*` functions sit at the PI-controller boundary: the PI law
// produces an f64 state; the output is cast to fxp once per step.
// `f32_*` functions sit at the CLI argv boundary: user-typed decimals
// are converted to fxp immediately inside the handler.
// ────────────────────────────────────────────────────────────────────

/// f64 phase → Q0.32. Tolerates inputs outside `[0, 1)` via
/// `rem_euclid`.
pub fn f64_phase_to_phase(p: f64) -> Phase {
    // PI-exempt: this is the cast where the PI state crosses into fxp.
    let wrapped = p.rem_euclid(1.0);
    let scaled = (wrapped * (1u64 << 32) as f64).round();
    // Clamp to [0, 2^32); values very close to 1.0 may round up to 2^32
    // which must wrap to 0 rather than overflow the u32 cast.
    let bits = if !(0.0..(1u64 << 32) as f64).contains(&scaled) {
        0u32
    } else {
        scaled as u32
    };
    Phase(bits)
}

/// f64 BPM → `Tempo` with round-to-nearest. Negative or NaN
/// inputs saturate to `ZERO`.
pub fn f64_bpm_to_tempo(b: f64) -> Tempo {
    // PI-exempt.
    if !b.is_finite() || b <= 0.0 {
        return Tempo::ZERO;
    }
    let scaled = (b * 1_000_000.0).round();
    if scaled >= u32::MAX as f64 {
        Tempo(u32::MAX)
    } else {
        Tempo(scaled as u32)
    }
}

// ────────────────────────────────────────────────────────────────────
// PI-exempt control-law helpers.
//
// The PI controller in `sync::pll` consumes a frequency in Hz and a
// time-in-seconds as f64 — genuine analog-DSP arithmetic. These two
// helpers convert `Tempo` and Q48.16-bit samples into those f64
// inputs in a single well-named site, replacing six open-coded
// copies of the same arithmetic previously scattered across
// `sync::pll` prod and tests.
// ────────────────────────────────────────────────────────────────────

/// Pulse frequency in Hz given tempo and pulses-per-quarter.
///
/// `hz = (bpm_µ / 10⁶) × ppq / 60`, executed entirely in f64 because
/// the PI-controller state is f64 by design. The integer-fxp rounding
/// contracts don't apply here — the PI law is continuous.
pub fn tempo_to_hz(bpm: Tempo, ppq: u32) -> f64 {
    // PI-exempt.
    (bpm.0 as f64 / 1.0e6) * ppq as f64 / 60.0
}

/// Q48.16-bit sample count → seconds at the given rate. Used by the
/// PLL's phase-error computation (observed pulse sample position −
/// expected, in seconds, fed to the PI loop).
pub fn bits_q48_16_to_seconds(bits: i64, sr: u32) -> f64 {
    // PI-exempt.
    (bits as f64) / ((sr as f64) * (1u64 << 16) as f64)
}

/// Pico → whole sample count at a runtime sample rate.
///
/// Dispatches on `sr` to the lawful `FD12Sxxx` Conn for that rate
/// (defined in `crate::time::sample`, re-exported above), calls
/// its `ceil` (Pico → Q48.16), then rounds to the nearest integer
/// sample. Returns `None` for non-audio rates — the supported set
/// is the six standard rates `{S044, S048, S088, S096, S176, S192}`.
///
/// Replaces the runtime `PicoSampleConn` Conn-lookalike: since the
/// set of audio sample rates is small and compile-time known, a
/// match dispatch to the lawful pre-composed constants is cleaner
/// than a runtime-parameterised struct, and each `FD12Sxxx` carries
/// its own per-rate Galois-law battery (in `crate::time::sample::tests`).
pub fn pico_to_samples(p: Pico, sr: u32) -> Option<i64> {
    // Each `FD12Sxxx.ceil(pico)` returns the rate-specific Sxxx
    // newtype; `.0` unwraps to the underlying `Q48_16`, `.round()`
    // snaps to a whole-sample Q48_16, and `.to_num::<i64>()`
    // extracts the integer sample count.
    Some(match sr {
        44_100 => FD12S044.ceil(p).0.round().to_num(),
        48_000 => FD12S048.ceil(p).0.round().to_num(),
        88_200 => FD12S088.ceil(p).0.round().to_num(),
        96_000 => FD12S096.ceil(p).0.round().to_num(),
        176_400 => FD12S176.ceil(p).0.round().to_num(),
        192_000 => FD12S192.ceil(p).0.round().to_num(),
        _ => return None,
    })
}

// ────────────────────────────────────────────────────────────────────
// Integer ramp + smoothstep.
//
// Both take integer inputs `t <= n` (with `n > 0`) and return u8.
// Bit-exact, no float. Used by `time::envelope` in T2 and potentially
// anywhere else a 0..255 envelope is needed.
// ────────────────────────────────────────────────────────────────────

/// Linear ramp `t/n` rendered as `u8`. Endpoints: `linear_u8(0, n) = 0`,
/// `linear_u8(n, n) = 255`. Degenerate `n = 0` returns 255 (treat
/// "no span" as fully open — matches `opening(0, 0) = 255` in
/// `time::envelope`).
pub fn linear_u8(t: u32, n: u32) -> u8 {
    if n == 0 {
        return 255;
    }
    if t >= n {
        return 255;
    }
    // round-nearest: (t * 255 + n/2) / n
    let num = u64::from(t) * 255 + u64::from(n) / 2;
    (num / u64::from(n)) as u8
}

/// Hermite smoothstep `3x² − 2x³` rendered as `u8` with
/// `x = t/n ∈ [0, 1]`. Endpoints: `smoothstep_u8(0, n) = 0`,
/// `smoothstep_u8(n, n) = 255`. Degenerate `n = 0` returns 255.
pub fn smoothstep_u8(t: u32, n: u32) -> u8 {
    if n == 0 {
        return 255;
    }
    if t >= n {
        return 255;
    }
    if t == 0 {
        return 0;
    }
    // x as Q0.24 (guaranteed < 1 here because t < n).
    //   x = t · 2^24 / n
    // y = 3x² − 2x³, with x in Q0.24:
    //   x² in Q0.48, x³ in Q0.72. Work in u128.
    let x: u128 = (u128::from(t) << 24) / u128::from(n);
    let x2: u128 = x * x;                 // Q0.48
    let x3: u128 = x2 * x;                // Q0.72
    // y = 3·x² − 2·x³, both terms scaled to Q0.48 then combined.
    //   3·x² is already Q0.48.
    //   2·x³ in Q0.72 becomes (2·x³) >> 24 in Q0.48 (with rounding).
    let term_a = 3u128 * x2;
    let rounding = 1u128 << 23;
    let term_b = (2u128 * x3 + rounding) >> 24;
    let y: u128 = term_a - term_b;        // Q0.48, always ≤ 2^48
    // scale to u8: (y * 255 + 2^47) >> 48
    let scaled = (y * 255u128 + (1u128 << 47)) >> 48;
    scaled.min(255) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    // ────────────────────────────────────────
    // Phase
    // ────────────────────────────────────────

    proptest! {
        #[test]
        fn phase_wraps_modulo_2_32(x in any::<u32>(), y in any::<u32>()) {
            let sum = Phase(x).wrapping_add(Phase(y));
            prop_assert_eq!(sum.0, x.wrapping_add(y));
        }

        #[test]
        fn phase_add_zero_identity(x in any::<u32>()) {
            prop_assert_eq!(Phase(x).wrapping_add(Phase::ZERO), Phase(x));
        }

        #[test]
        fn phase_add_commutes(x in any::<u32>(), y in any::<u32>()) {
            prop_assert_eq!(
                Phase(x).wrapping_add(Phase(y)),
                Phase(y).wrapping_add(Phase(x))
            );
        }

        #[test]
        fn phase_sub_inverts_add(a in any::<u32>(), b in any::<u32>()) {
            let s = Phase(a).wrapping_add(Phase(b));
            prop_assert_eq!(s.wrapping_sub(Phase(b)), Phase(a));
        }
    }

    // ────────────────────────────────────────
    // smoothstep_u8 / linear_u8
    // ────────────────────────────────────────

    #[test]
    fn smoothstep_endpoints_explicit() {
        assert_eq!(smoothstep_u8(0, 10), 0);
        assert_eq!(smoothstep_u8(10, 10), 255);
        assert_eq!(smoothstep_u8(5, 10), 128); // Hermite midpoint
    }

    #[test]
    fn linear_endpoints_explicit() {
        assert_eq!(linear_u8(0, 10), 0);
        assert_eq!(linear_u8(10, 10), 255);
        assert_eq!(linear_u8(5, 10), 128);
    }

    #[test]
    fn degenerate_n_zero() {
        assert_eq!(smoothstep_u8(0, 0), 255);
        assert_eq!(linear_u8(0, 0), 255);
    }

    proptest! {
        #[test]
        fn smoothstep_u8_endpoints(n in 1u32..u32::MAX) {
            prop_assert_eq!(smoothstep_u8(0, n), 0);
            prop_assert_eq!(smoothstep_u8(n, n), 255);
        }

        #[test]
        fn smoothstep_u8_monotone(t1 in 0u32..=1_000_000, n in 1u32..=1_000_000) {
            let t2 = t1.saturating_add(1);
            let (t1, t2) = if t1 <= n && t2 <= n { (t1, t2) } else { (0, 1.min(n)) };
            prop_assert!(smoothstep_u8(t1, n) <= smoothstep_u8(t2, n));
        }

        #[test]
        fn smoothstep_u8_symmetric(t in 0u32..=10_000, n_extra in 0u32..=10_000) {
            let n = t + n_extra;
            if n == 0 { return Ok(()); }
            let a = smoothstep_u8(t, n) as u32;
            let b = smoothstep_u8(n - t, n) as u32;
            // Hermite is symmetric around x=0.5, so s(t) + s(n-t) = 255,
            // modulo ±1 ULP rounding.
            let sum = a + b;
            prop_assert!(
                (254..=256).contains(&sum),
                "sum={} for t={} n={}",
                sum,
                t,
                n
            );
        }

        #[test]
        fn linear_u8_endpoints(n in 1u32..u32::MAX) {
            prop_assert_eq!(linear_u8(0, n), 0);
            prop_assert_eq!(linear_u8(n, n), 255);
        }

        #[test]
        fn linear_u8_monotone(t1 in 0u32..=1_000_000, n in 1u32..=1_000_000) {
            let t2 = t1.saturating_add(1);
            let (t1, t2) = if t1 <= n && t2 <= n { (t1, t2) } else { (0, 1.min(n)) };
            prop_assert!(linear_u8(t1, n) <= linear_u8(t2, n));
        }
    }

    // ────────────────────────────────────────
    // f64 / f32 boundary conversions
    // ────────────────────────────────────────

    proptest! {
        #[test]
        fn f64_phase_roundtrip(x in -1.0e6..1.0e6_f64) {
            // Implementation is round-nearest on a Q0.32, so worst-case
            // error is 2⁻³³. Plan's Verification table asserts < 2⁻³¹
            // as the contract; pick that (still well-above-implementation)
            // so a regression to half-ULP drift gets caught.
            let p = f64_phase_to_phase(x);
            let roundtrip = p.0 as f64 / (1u64 << 32) as f64;
            let expected = x.rem_euclid(1.0);
            prop_assert!(
                (roundtrip - expected).abs() < 2.0f64.powi(-31),
                "roundtrip={} expected={} for x={}",
                roundtrip,
                expected,
                x
            );
        }

        #[test]
        fn f64_bpm_roundtrip(b in 30.0_f64..=400.0) {
            let u = f64_bpm_to_tempo(b);
            let roundtrip = u.0 as f64 * 1.0e-6;
            // µBPM quantisation: ±5e-7 worst case.
            prop_assert!(
                (roundtrip - b).abs() < 1.0e-6,
                "roundtrip={} input={} u={}",
                roundtrip,
                b,
                u.0
            );
        }

        #[test]
        fn tempo_to_hz_matches_formula(b in 30_u32..=400, ppq in 1_u32..=1_024) {
            let bpm = Tempo::from_bpm_integer(b);
            let got = tempo_to_hz(bpm, ppq);
            let expected = (b as f64) * (ppq as f64) / 60.0;
            prop_assert!((got - expected).abs() < 1e-9);
        }

        #[test]
        fn bits_q48_16_to_seconds_matches_formula(
            bits in -10_000_000_000_i64..=10_000_000_000,
            sr in 1_u32..=192_000,
        ) {
            let got = bits_q48_16_to_seconds(bits, sr);
            let expected = (bits as f64) / ((sr as f64) * (1u64 << 16) as f64);
            prop_assert!((got - expected).abs() < 1e-15 * expected.abs().max(1.0));
        }
    }

    #[test]
    fn f64_bpm_edge_cases() {
        assert_eq!(f64_bpm_to_tempo(120.0), Tempo(120_000_000));
        assert_eq!(f64_bpm_to_tempo(0.0), Tempo::ZERO);
        assert_eq!(f64_bpm_to_tempo(-5.0), Tempo::ZERO);
        assert_eq!(f64_bpm_to_tempo(f64::NAN), Tempo::ZERO);
    }

    // Hand-computed witnesses for the two PI-exempt helpers,
    // independent of the formula-as-test proptests above.
    #[test]
    fn tempo_to_hz_hand_computed() {
        // 120 BPM, 24 PPQ: 120/60 × 24 = 48 Hz pulse rate.
        assert_eq!(tempo_to_hz(Tempo::from_bpm_integer(120), 24), 48.0);
        // 60 BPM, 1 PPQ: 1 Hz.
        assert_eq!(tempo_to_hz(Tempo::from_bpm_integer(60), 1), 1.0);
        // 180 BPM, 4 PPQ: 3 × 4 = 12 Hz.
        assert_eq!(tempo_to_hz(Tempo::from_bpm_integer(180), 4), 12.0);
    }

    #[test]
    fn bits_q48_16_to_seconds_hand_computed() {
        // 48 000 Hz, 48 000 × 2¹⁶ bits = 3_145_728_000 bits ≡ 1 second.
        assert_eq!(bits_q48_16_to_seconds(3_145_728_000, 48_000), 1.0);
        // 48 000 Hz, one sample = 65 536 bits = 1/48000 seconds.
        let one_sample = bits_q48_16_to_seconds(65_536, 48_000);
        assert!((one_sample - 1.0 / 48_000.0).abs() < 1e-15);
        // 44 100 Hz, half-second = 22 050 × 2¹⁶ bits.
        let half_sec = bits_q48_16_to_seconds(22_050 * 65_536, 44_100);
        assert!((half_sec - 0.5).abs() < 1e-15);
        // Negative bits round consistently (no `div_euclid` boundary issue).
        assert_eq!(bits_q48_16_to_seconds(-3_145_728_000, 48_000), -1.0);
    }

    #[test]
    fn f64_phase_saturation_near_one() {
        // Values close enough to 1.0 that scaling reaches 2^32 must wrap
        // to 0 rather than panic on the u32 cast.
        let p = f64_phase_to_phase(1.0 - 1.0e-15);
        assert!(p.0 == 0 || p.0 < u32::MAX);
    }

    // `pico_to_samples` is a hand-written `match` dispatching on `sr`
    // to the lawful `FD12Sxxx` conns from `crate::time::sample`.
    // Each conn's own per-rate Galois-law battery
    // (`time::sample::tests::p_fd12s0??`) catches arithmetic bugs
    // inside the conn itself, but nothing there catches a local
    // wiring mistake like "oops, the 96k arm calls FD12S088 by
    // accident." These tests lock in the dispatch table.

    #[test]
    fn pico_to_samples_one_second_maps_to_sr() {
        // 1 second = 10¹² pico = `sr` samples at every supported rate.
        // Any cross-wired arm (e.g. 96k → FD12S088) would return 88_200
        // instead of 96_000 and fail here.
        let one_second = Pico(1_000_000_000_000);
        for sr in [44_100, 48_000, 88_200, 96_000, 176_400, 192_000] {
            assert_eq!(
                pico_to_samples(one_second, sr),
                Some(sr as i64),
                "sr = {sr}: expected {sr} samples at 1 s"
            );
        }
    }

    #[test]
    fn pico_to_samples_rejects_unsupported_and_zero() {
        assert_eq!(pico_to_samples(Pico(1_000_000_000_000), 22_050), None);
        assert_eq!(pico_to_samples(Pico(1_000_000_000_000), 44_099), None);
        assert_eq!(pico_to_samples(Pico(0), 0), None);
    }

    // ────────────────────────────────────────
    // Quantum
    // ────────────────────────────────────────

    #[test]
    fn quantum_from_bars_integer_hand_computed() {
        assert_eq!(Quantum::from_bars(4).0.0, 4_000_000);
        assert_eq!(Quantum::from_bars(1).0.0, 1_000_000);
        assert_eq!(Quantum::from_bars(0), Quantum::ZERO);
    }

    #[test]
    fn f64_beats_edge_cases() {
        assert_eq!(f64_beats_to_quantum(4.0), Quantum::from_bars(4));
        assert_eq!(f64_beats_to_quantum(3.5), Quantum(Micro(3_500_000)));
        assert_eq!(f64_beats_to_quantum(0.0), Quantum::ZERO);
        assert_eq!(f64_beats_to_quantum(f64::NAN), Quantum::ZERO);
        assert_eq!(
            f64_beats_to_quantum(f64::INFINITY),
            Quantum::ZERO,
            "infinity treated as non-finite"
        );
    }

    // `f64_beats_to_quantum` must produce the same microbeats integer
    // as Link's own `Beats(double)` constructor — `std::llround(q × 1e6)`.
    // Rust's `f64::round` is round-half-away-from-zero, matching C++'s
    // `std::llround`. This property pins that agreement across the
    // realistic ABI range.
    proptest! {
        #[test]
        fn f64qnt_matches_link_beats(q in -1_000_000.0_f64..=1_000_000.0) {
            let got = f64_beats_to_quantum(q).0.0;
            // Reference: round-half-away-from-zero, saturating cast.
            let scaled = (q * 1_000_000.0).round();
            let expected = if scaled > i64::MAX as f64 {
                i64::MAX
            } else if scaled < i64::MIN as f64 {
                i64::MIN
            } else {
                scaled as i64
            };
            prop_assert_eq!(got, expected, "disagreement at q={}", q);
        }

        /// Monotonicity: `q1 <= q2 ⟹ f64_beats_to_quantum(q1).0 <=
        /// f64_beats_to_quantum(q2).0` across finite inputs. This is
        /// the Conn monotone-map surrogate — the full adjoint law
        /// becomes expressible when `F64QNT: Conn<f64, Quantum>`
        /// proper lands (upstream needs a `float_conn!` variant for
        /// i64-backed newtypes; tracked in enforcement's §Deferred).
        #[test]
        fn f64qnt_monotone(
            a in -1_000_000.0_f64..=1_000_000.0,
            b in -1_000_000.0_f64..=1_000_000.0,
        ) {
            let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
            let qlo = f64_beats_to_quantum(lo).0.0;
            let qhi = f64_beats_to_quantum(hi).0.0;
            prop_assert!(qlo <= qhi, "qlo={} > qhi={} for lo={} hi={}", qlo, qhi, lo, hi);
        }
    }
}
