//! Proptest strategy for [`Whole`](super::Whole) (rational whole-note
//! duration; alias for `Rational64`).
//!
//! Gated on `cfg(any(test, feature = "testkit"))` so downstream
//! crates can pull this strategy into their own proptest blocks
//! without forcing the rest of the workspace to compile proptest.

use num_rational::Rational64;
use proptest::prelude::*;

pub fn arb_rational_nonneg() -> impl Strategy<Value = Rational64> {
    prop_oneof![
        1 => Just(Rational64::new(0, 1)),
        1 => Just(Rational64::new(1, 4)),
        1 => Just(Rational64::new(1, 1)),
        4 => (0i64..=10_000, 1i64..=3840).prop_map(|(n, d)| Rational64::new(n, d)),
    ]
}
