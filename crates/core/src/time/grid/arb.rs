//! Proptest strategy for [`Grid`](super::Grid).
//!
//! Gated on `cfg(any(test, feature = "testkit"))` so downstream
//! crates can pull this strategy into their own proptest blocks
//! without forcing the rest of the workspace to compile proptest.

use proptest::prelude::*;

use crate::time::grid::Grid;

/// Full 36-element [`Grid`] lattice strategy. Used wherever the
/// channel divider, [`quantize_at`](crate::time::conn::quantize_at)
/// argument, or [`Time::base`](crate::time::tick::Time::base) crosses
/// the test surface.
pub fn arb_grid() -> impl Strategy<Value = Grid> {
    prop_oneof![
        1 => Just(Grid::T1),
        1 => Just(Grid::T512P),
        4 => prop::sample::select(Grid::ALL.as_slice()),
    ]
}
