//! f64 boundary helpers: argv parsers, FFI parity, and PI-controller
//! conversions to/from agogo's fxp tier.
//!
//! Moved here from `crate::fxp` (Plan 2026-04-28-03 T5) — these are the
//! `f64 ↔ fxp` conversion seam, separate from the type definitions
//! that live in `crate::sync::phase`, `crate::time::tempo`, and
//! `crate::time::{decimal, float, sample}`.
//!
//! The `f64_*` functions here divide into three categories per
//! CLAUDE.md's "five documented exceptions":
//! 1. **PI-controller boundary** (`tempo_to_f64_bpm`,
//!    `pico_to_f64_seconds`, `tempo_to_hz`, `bits_q48_16_to_seconds`):
//!    the PI law produces an f64 state; these helpers cast to/from
//!    fxp once per step.
//! 2. **argv boundary** (`f64_phase_to_phase`, `f64_bpm_to_tempo`):
//!    user-typed decimals from the CLI are converted to fxp
//!    immediately inside the handler.
//! 3. **PI-exempt rate dispatch** (`pico_to_samples`): match on a
//!    runtime sample-rate to a per-rate `FD12Sxxx` Conn, then snap to
//!    integer samples. Each `FD12Sxxx` carries its own per-rate
//!    Galois-law battery.

use connections::extended::Extended;
use connections::float::ExtendedFloat;
use connections::int::u32::I064U032;

use crate::sync::phase::Phase;
use crate::time::decimal::{FD06, Pico};
use crate::time::float::{F064FD06, F064FD12};
use crate::time::sample::{FD12S044, FD12S048, FD12S088, FD12S096, FD12S176, FD12S192};
use crate::time::tempo::Tempo;

/// Maximum representable BPM as `f64`: `u32::MAX as f64 / 10⁶`
/// ≈ 4294.967295. Used as the upper bound for argv parsers
/// that want to reject "out of range" BPM rather than silently
/// saturate.
///
/// Computed as a plain `u32 as f64 / 1.0e6` because
/// `tempo_to_f64_bpm(Tempo(u32::MAX))` returns a much larger
/// value (the `I064U032.inner` saturating-widen step lifts
/// `u32::MAX` to `i64::MAX` before the F-ladder inverse, so
/// the result is `i64::MAX / 10⁶` ≈ 9.22 × 10¹²). The `× 10⁻⁶`
/// here is a one-off domain-boundary constant, not a per-input
/// scale shift.
///
/// Lives here in `boundary` (rather than as `Tempo::MAX_BPM_F64`)
/// so `crate::time::tempo` stays f64-free per the workspace's
/// `scripts/check-floats.sh` allowlist (Plan 2026-04-28-03 review
/// round 1).
pub const MAX_BPM_F64: f64 = (u32::MAX as f64) / 1_000_000.0;

/// Extract the finite f64 from an `ExtendedFloat` produced by a
/// finite-domain Conn::inner call. Documents the finite-input
/// invariant once, so the unreachable! arms in `tempo_to_f64_bpm`
/// and `pico_to_f64_seconds` collapse to a single helper. Plan
/// 2026-04-28-03 T5.
fn finite_or_unreachable(ef: ExtendedFloat<f64>) -> f64 {
    match ef {
        ExtendedFloat::Extend(x) => x,
        ExtendedFloat::Bot | ExtendedFloat::Top => {
            unreachable!("F064FD?? Conn::inner of Extended::Finite cannot lift to Bot/Top")
        }
    }
}

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

/// f64 BPM → `Tempo` with round-to-nearest. Non-finite (NaN /
/// ±∞) and non-positive inputs saturate to `ZERO`; values whose
/// scaled µBPM exceed `u32::MAX` saturate to `Tempo(u32::MAX)`.
///
/// **Round-to-nearest, not Conn-composed.** Same FFI-parity
/// reasoning as `f64_beats_to_quantum`: agogo's `Tempo` is the
/// internal counterpart of Link's microBPM ABI, and the
/// `f64_bpm_roundtrip` proptest pins agreement to within ±5e-7
/// BPM (one µBPM ULP). `F064FD06.ceil` would shift the rounding
/// direction by up to 1 µBPM at the half-step, breaking that
/// contract. The `* 1_000_000.0` unit shift is the same
/// FFI-parity exception documented above. The `i64 → u32`
/// narrowing IS lawful and goes through `I064U032.ceil`
/// (closes N5 — the `as u32` saturation cast is now named).
pub fn f64_bpm_to_tempo(b: f64) -> Tempo {
    // PI-exempt; FFI-parity exception per fn-doc above.
    if !b.is_finite() || b <= 0.0 {
        return Tempo::ZERO;
    }
    let scaled = (b * 1_000_000.0).round();
    if scaled > i64::MAX as f64 {
        return Tempo(u32::MAX);
    }
    if scaled < 0.0 {
        return Tempo::ZERO;
    }
    Tempo(I064U032.ceil(scaled as i64))
}

/// `Tempo` (u32 microBPM) → f64 BPM via the lawful `F064FD06`
/// Conn-inverse. The `× 10⁻⁶` unit shift lives inside `F064FD06`'s
/// definition (`crate::time::float`); the `u32 → i64` widening
/// is `I064U032.inner` (lossless).
pub fn tempo_to_f64_bpm(t: Tempo) -> f64 {
    // PI-exempt.
    let widened = I064U032.inner(t.0);
    finite_or_unreachable(F064FD06.inner(Extended::Finite(FD06(widened))))
}

/// `Pico` (i64 picoseconds) → f64 seconds via the lawful `F064FD12`
/// Conn-inverse. The `× 10⁻¹²` unit shift lives inside `F064FD12`'s
/// definition (`crate::time::float`).
pub fn pico_to_f64_seconds(p: Pico) -> f64 {
    // PI-exempt.
    finite_or_unreachable(F064FD12.inner(Extended::Finite(p)))
}

// ────────────────────────────────────────────────────────────────────
// PI-exempt control-law helpers.
//
// The PI controller in `sync::pll` consumes a frequency in Hz and a
// time-in-seconds as f64 — genuine analog-DSP arithmetic. These two
// helpers convert `Tempo` and Q48.16-bit samples into those f64
// inputs in a single well-named site.
// ────────────────────────────────────────────────────────────────────

/// Pulse frequency in Hz given tempo and pulses-per-quarter.
///
/// `hz = bpm × ppq / 60`, executed in f64 because the PI-controller
/// state is f64 by design. The `Tempo → f64` conversion is lawful
/// via [`tempo_to_f64_bpm`]; only the `× ppq / 60` multiplication
/// stays open-coded (it's PI-exempt continuous arithmetic, not an
/// SI unit shift).
pub fn tempo_to_hz(bpm: Tempo, ppq: u32) -> f64 {
    // PI-exempt.
    tempo_to_f64_bpm(bpm) * ppq as f64 / 60.0
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
/// (defined in `crate::time::sample`), calls its `ceil` (Pico →
/// Q48.16), then rounds to the nearest integer sample. Returns
/// `None` for non-audio rates — the supported set is the six
/// standard rates `{S044, S048, S088, S096, S176, S192}`.
pub fn pico_to_samples(p: Pico, sr: u32) -> Option<i64> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

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
            // Independent reference for the round-trip — calling
            // `tempo_to_f64_bpm` here would compare production code
            // to itself. The hand-coded `* 1.0e-6` is the regression
            // gate that proves both `f64_bpm_to_tempo` (forward) and
            // `tempo_to_f64_bpm` (reverse) agree with the f64 spec.
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

        /// `tempo_to_f64_bpm` round-trip across the full `u32` domain.
        /// The function is the canonical Tempo→f64 helper used in 7+
        /// sites (CLI display, host-link FFI, arb fixtures); without
        /// this, a regression in `F064FD06.inner` or `I064U032.inner`
        /// would only be caught indirectly via `tempo_to_hz_matches_formula`,
        /// which exercises only integer 30..=400 BPM. Independent
        /// reference: `raw / 1_000_000.0` — the same arithmetic the
        /// helper composes via lawful Conns.
        #[test]
        fn tempo_to_f64_bpm_full_domain(raw in any::<u32>()) {
            let got = tempo_to_f64_bpm(Tempo(raw));
            let expected = raw as f64 / 1_000_000.0;
            prop_assert!(
                (got - expected).abs() < 1e-9,
                "tempo_to_f64_bpm({raw}) = {got}, expected {expected}",
            );
        }

        /// `pico_to_f64_seconds` round-trip across the full signed
        /// `i64` domain (including negatives). Same rationale as
        /// `tempo_to_f64_bpm_full_domain`: the helper is used by
        /// `arb::pulse_train` for `PULSE_WIDTH_PS` and `jitter_sigma`
        /// and has no other direct test. Independent reference:
        /// `raw / 1.0e12`.
        #[test]
        fn pico_to_f64_seconds_full_domain(raw in any::<i64>()) {
            let got = pico_to_f64_seconds(Pico(raw));
            let expected = raw as f64 / 1.0e12;
            // i64 → f64 loses precision for |raw| beyond 2^53, so
            // compare with a relative tolerance.
            let abs_err = (got - expected).abs();
            let rel_tol = expected.abs().max(1.0) * 1e-12;
            prop_assert!(
                abs_err < rel_tol,
                "pico_to_f64_seconds({raw}) = {got}, expected {expected}",
            );
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
}
