//! Proptest strategy for [`SwingConfig`](super::SwingConfig).
//!
//! Gated on `cfg(any(test, feature = "testkit"))` so downstream
//! crates can pull this strategy into their own proptest blocks
//! without forcing the rest of the workspace to compile proptest.

use proptest::prelude::*;

use crate::time::swing::SwingConfig;
use crate::time::tbase::TBase;
use crate::time::tbase::arb::arb_tbase;

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
