//! `Quantum` — Link's quantum as microbeats.
//!
//! Ableton Link represents quantum internally as `std::int64_t`
//! microbeats (see ext/rusty_link/link/include/ableton/link/Beats.hpp).
//! Its public `double quantum` ABI converts via `std::llround(q * 1e6)`
//! on the first line of every API body. Wrapping a `Micro` (10⁻⁶ rung
//! of the decimal ladder, `i64` backing) gives agogo's `Quantum` the
//! same integer representation Link's C++ side stores — zero
//! disagreement at the FFI boundary.
//!
//! Moved here from `agogo_core::fxp` (Plan 2026-04-28-03 T4): the
//! type is purely Link-FFI-shaped — every production caller feeds it
//! into `LinkSession::snap_offset_for` or constructs it from argv to
//! do so. Pulling it out of `core` removes a Link-specific concept
//! from a Link-agnostic crate.
//!
//! This module is ungated (always compiled) even though the rest of
//! `host-link` is `#[cfg(feature = "rusty-link")]`. The Quantum type
//! is just a small newtype + integer arithmetic; no FFI dependency.
//! Default-feature builds get the type for free; the gating is on
//! the actual `rusty_link` integration, not on the parameter shape.

use agogo_core::time::decimal::Micro;

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
    //
    // **Round-to-nearest, not Conn-composed.** Link's C++ side does
    // `std::llround(q × 1e6)` (round-half-away-from-zero) on every
    // microbeat construction; agogo's `Quantum` must agree
    // bit-for-bit at the FFI seam. `F064FD06.ceil` and `.floor` are
    // Galois adjoints (round up / round down), but
    // round-half-away-from-zero is **not** a Galois adjoint and has
    // no `Conn` equivalent. The `* 1_000_000.0` unit shift is
    // documented here as an **FFI-parity exception** to the
    // Conn-discipline rule. The `f64qnt_matches_link_beats`
    // proptest pins the bit-exact agreement.
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

/// bpaf parser: beats `<f64>` → `Quantum` at the argv-handler
/// boundary. Used by `--link-quantum` (run) and `--quantum`
/// (link transport); errors are flag-agnostic.
///
/// Moved from `cli/main.rs` (Plan 2026-04-28-03 T4) — the parser
/// belongs alongside the type it produces.
pub fn parse_quantum_from_beats(s: String) -> Result<Quantum, String> {
    let f: f64 = s
        .parse()
        .map_err(|e| format!("quantum value {s}: not a number ({e})"))?;
    if !f.is_finite() || f <= 0.0 {
        return Err(format!(
            "quantum value {f} invalid (must be finite and > 0)"
        ));
    }
    Ok(f64_beats_to_quantum(f))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

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
            // Independent reference: round-half-away-from-zero,
            // saturating cast — exactly what Link's C++ side does
            // via `std::llround(q * 1e6)`. Hand-coded here (not via
            // F064FD06) so the proptest is a true regression gate
            // for `f64_beats_to_quantum`'s composition body, not a
            // tautology comparing the function to itself.
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
