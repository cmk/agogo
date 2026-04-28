//! `Phase` — Q0.32 NCO accumulator.
//!
//! Whole `u32` range maps to `[0, 1)` cycles; `wrapping_add` IS
//! modular reduction, which is the load-bearing property of the NCO
//! hot path.
//!
//! Moved here from `crate::fxp` (Plan 2026-04-28-03 T4): `Phase` is
//! NCO controller state, not arithmetic primitives. Belongs alongside
//! `sync::pll`, not in `fxp`.

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

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

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
}
