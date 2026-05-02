//! MIDI domain newtypes.
//!
//! `U7` (`0..=127`) covers MIDI note numbers, velocities, CC values,
//! channel-pressure values, and similar 7-bit data bytes. `U4`
//! (`0..=15`) covers MIDI channel numbers (zero-based; user-facing
//! 1..=16 is mapped at the spec parser).
//!
//! These types are MIDI-specific by design — they live in
//! `agogo-core` rather than the general-purpose `connections` crate.
//!
//! The `U007U008` / `U004U008` connections adapt the Haskell `Cast 'L`
//! saturating pattern from `Data.Connection.Word` (`conn = CastL f g`
//! where `f = fromIntegral . max 0` and `g = fromIntegral . min (f
//! maxBound)`). Since `U7` / `U4` are unsigned, `max 0` is a no-op:
//! `ceil` is the exact embedding into `u8`, and `inner` saturates a
//! `u8` to the newtype's `MAX`.

use connections::conn::{Conn, ConnL};

// ── U7 — 7-bit unsigned (0..=127). ──

#[repr(transparent)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct U7(pub u8);

impl U7 {
    pub const MAX: u8 = 127;
    pub const ZERO: Self = Self(0);

    /// Returns `Some(U7(x))` iff `x <= 127`.
    pub const fn new(x: u8) -> Option<Self> {
        if x <= Self::MAX { Some(Self(x)) } else { None }
    }
}

impl From<U7> for u8 {
    fn from(x: U7) -> u8 {
        x.0
    }
}

impl core::fmt::Display for U7 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.0.fmt(f)
    }
}

// ── U4 — 4-bit unsigned (0..=15). ──

#[repr(transparent)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct U4(pub u8);

impl U4 {
    pub const MAX: u8 = 15;
    pub const ZERO: Self = Self(0);

    /// Returns `Some(U4(x))` iff `x <= 15`.
    pub const fn new(x: u8) -> Option<Self> {
        if x <= Self::MAX { Some(Self(x)) } else { None }
    }
}

impl From<U4> for u8 {
    fn from(x: U4) -> u8 {
        x.0
    }
}

impl core::fmt::Display for U4 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.0.fmt(f)
    }
}

// ── Saturating Galois connections (Haskell `Cast 'L`). ──
//
// For each (narrow, u8) pair:
// - `ceil:  Narrow → u8` is the exact embedding (the narrow type's
//   domain is a subset of u8's, so no rounding is needed).
// - `inner: u8 → Narrow` saturates to `Narrow::MAX`.
// - `Conn::new_l` builds the one-sided `'L` shape; no `floor`
//   operation is exposed because there is no right adjoint.

pub const U007U008: ConnL<U7, u8> = {
    fn ceil(x: U7) -> u8 {
        x.0
    }
    fn inner(x: u8) -> U7 {
        U7(if x <= U7::MAX { x } else { U7::MAX })
    }
    Conn::new_l(ceil, inner)
};

pub const U004U008: ConnL<U4, u8> = {
    fn ceil(x: U4) -> u8 {
        x.0
    }
    fn inner(x: u8) -> U4 {
        U4(if x <= U4::MAX { x } else { U4::MAX })
    }
    Conn::new_l(ceil, inner)
};

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    // ── Spot checks ──────────────────────────────────────────────

    #[test]
    fn u7_new_at_boundary() {
        assert_eq!(U7::new(127), Some(U7(127)));
        assert_eq!(U7::new(128), None);
        assert_eq!(U7::new(255), None);
        assert_eq!(U7::new(0), Some(U7(0)));
    }

    #[test]
    fn u4_new_at_boundary() {
        assert_eq!(U4::new(15), Some(U4(15)));
        assert_eq!(U4::new(16), None);
        assert_eq!(U4::new(255), None);
        assert_eq!(U4::new(0), Some(U4(0)));
    }

    #[test]
    fn u7u8_inner_at_127() {
        assert_eq!(U007U008.inner(127), U7(127));
    }

    #[test]
    fn u7u8_inner_at_max_u8_saturates() {
        assert_eq!(U007U008.inner(255), U7(127));
        assert_eq!(U007U008.inner(128), U7(127));
    }

    #[test]
    fn u7u8_ceil_zero() {
        assert_eq!(U007U008.ceil(U7(0)), 0);
    }

    #[test]
    fn u4u8_inner_at_max_u8_saturates() {
        assert_eq!(U004U008.inner(255), U4(15));
        assert_eq!(U004U008.inner(16), U4(15));
        assert_eq!(U004U008.inner(15), U4(15));
        assert_eq!(U004U008.inner(0), U4(0));
    }

    #[test]
    fn from_impls_are_lossless() {
        assert_eq!(u8::from(U7(76)), 76);
        assert_eq!(u8::from(U4(9)), 9);
    }

    #[test]
    fn display_forwards_to_inner_u8() {
        assert_eq!(format!("{}", U7(76)), "76");
        assert_eq!(format!("{}", U4(9)), "9");
    }

    #[test]
    fn le_compares_inner() {
        assert!(U7(10) <= U7(20));
        assert!(U7(20) <= U7(20));
        assert!(U7(20) > U7(10));
    }

    // ── Property tests ───────────────────────────────────────────
    //
    // Generators span the full `u8` domain (`any::<u8>()`) per
    // CLAUDE.md's coverage-faking rule: bounding to "keep things in
    // range" would hide the saturating boundary, which is precisely
    // what the connection's `inner` is supposed to enforce.

    proptest! {
        /// `U7::new` agrees with the documented predicate `x <= 127`
        /// for every possible u8 input.
        #[test]
        fn u7_new_iff_in_range(x in any::<u8>()) {
            prop_assert_eq!(U7::new(x).is_some(), x <= U7::MAX);
        }

        /// Same for `U4`.
        #[test]
        fn u4_new_iff_in_range(x in any::<u8>()) {
            prop_assert_eq!(U4::new(x).is_some(), x <= U4::MAX);
        }

        /// Round trip on the U7 side: `inner ∘ ceil = id` for any U7.
        /// Generator covers the full U7 domain; values outside it
        /// are unrepresentable in U7 by construction.
        #[test]
        fn u7u8_inner_round_trip_on_u7(x in 0u8..=U7::MAX) {
            let u = U7(x);
            prop_assert_eq!(U007U008.inner(U007U008.ceil(u)), u);
        }

        /// Saturation property on the u8 side: `ceil ∘ inner` clamps
        /// any u8 to `min(b, 127)`. Generator is the full u8 domain
        /// — bounding to `0..=127` would skip the saturation
        /// boundary, the point of the test.
        #[test]
        fn u7u8_ceil_inner_saturates(b in any::<u8>()) {
            prop_assert_eq!(U007U008.ceil(U007U008.inner(b)), b.min(U7::MAX));
        }

        /// Galois adjoint law: `ceil(a) ≤ b ⟺ a ≤ inner(b)`. Pairs
        /// span (full U7, full u8) so the saturating region above
        /// 127 on the u8 side is exercised.
        #[test]
        fn u7u8_galois_law(
            a in 0u8..=U7::MAX,
            b in any::<u8>(),
        ) {
            let a = U7(a);
            let lhs = U007U008.ceil(a) <= b;
            let rhs = a.0 <= U007U008.inner(b).0;
            prop_assert_eq!(lhs, rhs);
        }

        /// Same round-trip for U4. Generator covers the full U4
        /// domain (`0..=15`).
        #[test]
        fn u4u8_inner_round_trip_on_u4(x in 0u8..=U4::MAX) {
            let u = U4(x);
            prop_assert_eq!(U004U008.inner(U004U008.ceil(u)), u);
        }

        /// Saturation for U4 spans the full u8 domain.
        #[test]
        fn u4u8_ceil_inner_saturates(b in any::<u8>()) {
            prop_assert_eq!(U004U008.ceil(U004U008.inner(b)), b.min(U4::MAX));
        }

        /// Galois adjoint law for U4.
        #[test]
        fn u4u8_galois_law(
            a in 0u8..=U4::MAX,
            b in any::<u8>(),
        ) {
            let a = U4(a);
            let lhs = U004U008.ceil(a) <= b;
            let rhs = a.0 <= U004U008.inner(b).0;
            prop_assert_eq!(lhs, rhs);
        }
    }
}
