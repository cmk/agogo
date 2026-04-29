//! Proptest strategies for the decimal fixed-point ladder
//! ([`Pico`](super::Pico) and friends).
//!
//! Gated on `cfg(any(test, feature = "testkit"))` so downstream
//! crates can pull these strategies into their own proptest blocks
//! without forcing the rest of the workspace to compile proptest.

use proptest::prelude::*;

use crate::time::decimal::Pico;

/// Jitter σ as [`Pico`]. Heavy bias toward small values so the PLL
/// convergence properties usually fire on inputs they can lock to.
pub fn arb_jitter_sigma() -> impl Strategy<Value = Pico> {
    prop_oneof![
        1 => Just(Pico(0)),
        5 => (0i64..50_000_000).prop_map(Pico),          // 0..50 µs in ps
        2 => (50_000_000i64..200_000_000).prop_map(Pico),
        1 => (200_000_000i64..500_000_000).prop_map(Pico),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    proptest! {
        #[test]
        fn arb_jitter_in_range(j in arb_jitter_sigma()) {
            prop_assert!((0..=500_000_000).contains(&j.0));
        }
    }
}
