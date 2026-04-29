//! Proptest strategies for [`Tick`](super::Tick) and
//! [`Time`](super::Time).
//!
//! Gated on `cfg(any(test, feature = "testkit"))` so downstream
//! crates can pull these strategies into their own proptest blocks
//! without forcing the rest of the workspace to compile proptest.

use proptest::prelude::*;

use crate::time::grid::Grid;
use crate::time::grid::arb::arb_grid;
use crate::time::tick::{Tick, Time};

/// Per CLAUDE.md's full-domain rule, named boundaries (0,
/// `Grid::T512P` = 1, `Grid::T1` = 3840 ticks per bar, the
/// `from_ticks` horizon at `u32::MAX × Grid::T1.tick_count()`) get
/// explicit `Just` arms with elevated frequency. The 4-weighted
/// uniform arm covers the 0..=1M range where most musically-
/// meaningful tick values live; the upper bound is the largest
/// `Tick` for which [`from_ticks`](crate::time::tick::from_ticks)
/// returns `Some(_)`, so `arb_tick` never produces a value the
/// [`TICKTIME`](crate::time::conn::TICKTIME) Conn cannot
/// canonicalise.
pub fn arb_tick() -> impl Strategy<Value = Tick> {
    let horizon: u64 = u64::from(u32::MAX) * u64::from(Grid::T1.tick_count());
    prop_oneof![
        1 => Just(Tick(0)),
        1 => Just(Tick(u64::from(Grid::T512P.tick_count()))),
        1 => Just(Tick(u64::from(Grid::T1.tick_count()))),
        1 => Just(Tick(horizon)),
        4 => (0u64..=1_000_000).prop_map(Tick),
    ]
}

pub fn arb_time() -> impl Strategy<Value = Time> {
    (any::<u32>(), arb_grid()).prop_map(|(beats, base)| Time { beats, base })
}

pub fn arb_small_time() -> impl Strategy<Value = Time> {
    (0u32..=50, arb_grid()).prop_map(|(beats, base)| Time { beats, base })
}
