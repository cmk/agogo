//! Proptest strategies for the `time`-tier types
//! ([`Tick`](crate::time::tick::Tick),
//! [`Time`](crate::time::tick::Time),
//! [`Grid`](crate::time::grid::Grid),
//! [`TBase`](crate::time::tbase::TBase),
//! [`SwingConfig`](crate::time::swing::SwingConfig)).
//!
//! Consolidated by Plan 2026-04-29-01 T4 from the per-module
//! `arb.rs` files (`grid/arb.rs`, `swing/arb.rs`, `tbase/arb.rs`,
//! `tick/arb.rs`). Same shape as the upstream
//! `Test/Data/Connection/{Float,Int,…}.hs` test layout in the
//! Haskell library — strategies grouped per top-level module rather
//! than per type.
//!
//! The previous `time/arb.rs` content (FD-fixed strategies vendored
//! from `connections`) moved to `crate::conn::arb` along with its
//! parent types in Plan 2026-04-29-01 T2 / T4.
//!
//! Gated on `cfg(any(test, feature = "testkit"))` so downstream
//! crates can pull these strategies into their own proptest blocks
//! without forcing the rest of the workspace to compile proptest.

use proptest::prelude::*;

use crate::time::grid::Grid;
use crate::time::swing::SwingConfig;
use crate::time::tbase::TBase;
use crate::time::tick::{Tick, Time};

// ── Grid ──────────────────────────────────────────────────────────

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

// ── TBase ─────────────────────────────────────────────────────────

/// Binary subdivision strategy (9 variants). Used wherever a
/// `TBase`-typed value is required — most prominently
/// [`SwingConfig::resolution`](crate::time::swing::SwingConfig).
pub fn arb_tbase() -> impl Strategy<Value = TBase> {
    prop_oneof![
        1 => Just(TBase::T1),
        1 => Just(TBase::T256),
        4 => prop::sample::select(TBase::ALL.as_slice()),
    ]
}

// ── Swing ─────────────────────────────────────────────────────────

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

// ── Tick / Time ───────────────────────────────────────────────────

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
