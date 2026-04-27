//! `Tick` (960 PPQN master counter) and canonical `Time { beats, base }`.
//!
//! `Time` equality is by tick count, *not* structural — two
//! representations of the same duration (`Time { 240, T512P }` and
//! `Time { 1, T16 }` are both 240 ticks at 960 PPQN) compare equal.
//!
//! `from_ticks` is the ceiling side of the `ticks` Galois connection:
//! it rounds the input up to the [`Grid::T512P`] grid (1 tick = the
//! lattice bottom) then picks the nicest representation — coarsest
//! `Grid` whose tick count divides the rounded value, giving the
//! smallest `beats`. For aligned ticks (every tick at 960 PPQN, since
//! T512P = 1) it's an exact canonicalisation.

use std::cmp::Ordering;
use std::hash::{Hash, Hasher};

use crate::preorder::Ple;

use crate::time::grid::Grid;

/// Ticks per quarter note. 960 PPQN master resolution.
pub const PPQN: u32 = 960;

/// Master tick counter. Opaque `u32` newtype; one tick is `1/960` of a
/// quarter note.
#[repr(transparent)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, PartialOrd, Ord, Default)]
pub struct Tick(pub u32);

impl Ple for Tick {
    fn ple(&self, other: &Self) -> bool {
        self.0 <= other.0
    }
}

/// Musical time as (count × grid): `beats` positions on a `Grid` of
/// resolution `base`.
///
/// Equality and ordering are by tick count, so distinct
/// `(beats, base)` pairs denoting the same duration are equal. Use
/// [`from_ticks`] to get the canonical (coarsest-base, smallest-beat)
/// representation.
#[derive(Copy, Clone, Debug)]
pub struct Time {
    pub beats: u32,
    pub base: Grid,
}

/// Convert a musical `Time` to absolute ticks. Exact: no rounding.
///
/// # Panics
///
/// Panics if `beats * tick_count` overflows `u32`. The largest
/// possible product is `u32::MAX * 3840`, so callers constructing
/// `Time` with beats ≤ `u32::MAX / 3840 = 1_118_481` are always
/// safe. `arb_time` bounds beats well inside that. This path backs
/// `Time`'s `Eq`/`Ord`/`Hash`, so silent wrap would corrupt
/// equivalence-class semantics — a checked multiply fails loudly
/// instead.
pub fn time_to_tick(t: Time) -> Tick {
    Tick(
        t.beats
            .checked_mul(t.base.tick_count())
            .expect("time_to_tick overflow: beats * tick_count exceeds u32::MAX"),
    )
}

/// Round `n` up to the nicest `Time` representation.
///
/// At 960 PPQN with the full 36-element lattice, the bottom is
/// `Grid::T512P` = 1 tick, so every input is already aligned. The
/// `from_ticks` ceiling reduces to a pure canonicalisation: pick the
/// coarsest `Grid` whose tick count divides `n` exactly.
///
/// For `n = u32::MAX` we still need to handle overflow defensively —
/// the precision floor is 1, so `rounded_up = n`, no risk of overflow,
/// but we keep the same shape for symmetry with possible future
/// changes.
pub fn from_ticks(n: Tick) -> Time {
    let prec = u64::from(Grid::T512P.tick_count()); // = 1 at 960 PPQN
    let rounded_up = u64::from(n.0).div_ceil(prec) * prec;
    let max_aligned = (u64::from(u32::MAX) / prec) * prec;
    nicest_from_tick_count(rounded_up.min(max_aligned) as u32)
}

/// Round `n` down to the nicest `Time` representation (floor side of
/// the `ticks` Galois connection). At 960 PPQN with `T512P = 1` this
/// equals [`from_ticks`] for every input.
pub fn from_ticks_floor(n: Tick) -> Time {
    let prec = Grid::T512P.tick_count();
    let aligned = (n.0 / prec) * prec;
    nicest_from_tick_count(aligned)
}

/// Pick the coarsest `Grid` whose tick count divides `n`, and return
/// the corresponding `Time`. `Grid::ALL` is ordered coarsest-first
/// (binary T1→T256, then triplet, then quintuplet, then p), so the
/// first divisor wins.
///
/// For `n = 0` this returns `Time { beats: 0, base: T1 }` (every tick
/// count divides 0).
fn nicest_from_tick_count(n: u32) -> Time {
    for g in Grid::ALL {
        let tc = g.tick_count();
        if n % tc == 0 {
            return Time { beats: n / tc, base: g };
        }
    }
    unreachable!("Grid::T512P (tick_count = 1) divides every u32 value");
}

// Equality / ordering / hashing by tick count, not structurally.

impl PartialEq for Time {
    fn eq(&self, other: &Self) -> bool {
        time_to_tick(*self) == time_to_tick(*other)
    }
}

impl Eq for Time {}

impl PartialOrd for Time {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Time {
    fn cmp(&self, other: &Self) -> Ordering {
        time_to_tick(*self).cmp(&time_to_tick(*other))
    }
}

impl Hash for Time {
    fn hash<H: Hasher>(&self, state: &mut H) {
        time_to_tick(*self).hash(state);
    }
}

impl Ple for Time {
    fn ple(&self, other: &Self) -> bool {
        time_to_tick(*self).ple(&time_to_tick(*other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arb::{arb_grid, arb_tick, arb_time};
    use proptest::prelude::*;

    // ── Spot checks ───────────────────────────────────────────────

    #[test]
    fn ppqn_is_960() {
        assert_eq!(PPQN, 960);
    }

    #[test]
    fn time_to_tick_quarter_note() {
        assert_eq!(
            time_to_tick(Time { beats: 1, base: Grid::T4 }),
            Tick(960)
        );
    }

    #[test]
    fn time_to_tick_two_eighths() {
        assert_eq!(
            time_to_tick(Time { beats: 2, base: Grid::T8 }),
            Tick(960)
        );
    }

    #[test]
    fn from_ticks_240_is_one_sixteenth() {
        assert_eq!(
            from_ticks(Tick(240)),
            Time { beats: 1, base: Grid::T16 }
        );
    }

    #[test]
    fn from_ticks_192_is_one_quintuplet_eighth() {
        // T8Q = 192 ticks (5-per-quarter quintuplet).
        assert_eq!(
            from_ticks(Tick(192)),
            Time { beats: 1, base: Grid::T8Q }
        );
    }

    #[test]
    fn from_ticks_160_is_one_triplet_sixteenth() {
        assert_eq!(
            from_ticks(Tick(160)),
            Time { beats: 1, base: Grid::T16T }
        );
    }

    #[test]
    fn from_ticks_960_is_one_quarter() {
        assert_eq!(
            from_ticks(Tick(960)),
            Time { beats: 1, base: Grid::T4 }
        );
    }

    #[test]
    fn from_ticks_1_is_one_t512p() {
        // 1 tick = T512P. Coarsest divisor is T512P itself.
        assert_eq!(
            from_ticks(Tick(1)),
            Time { beats: 1, base: Grid::T512P }
        );
    }

    #[test]
    fn from_ticks_unaligned_round_trip_at_t512p_grid() {
        // At PPQN=960, every tick aligns to T512P (= 1), so floor and
        // ceiling collapse — every tick is its own canonical form.
        for n in [0, 1, 2, 50, 100, 1000, 1234, 100_000] {
            assert_eq!(from_ticks(Tick(n)), from_ticks_floor(Tick(n)));
            assert_eq!(time_to_tick(from_ticks(Tick(n))).0, n);
        }
    }

    #[test]
    fn from_ticks_zero_is_top() {
        // 0 % 3840 == 0, so the coarsest grid wins.
        assert_eq!(
            from_ticks(Tick(0)),
            Time { beats: 0, base: Grid::T1 }
        );
    }

    #[test]
    fn time_eq_by_tick_count() {
        // Different (beats, base) pairs but same duration.
        // At 960 PPQN: T64 = 60, T32 = 120. 2 × 60 = 1 × 120.
        assert_eq!(
            Time { beats: 2, base: Grid::T64 },
            Time { beats: 1, base: Grid::T32 }
        );
        assert_eq!(
            Time { beats: 2, base: Grid::T8 },
            Time { beats: 1, base: Grid::T4 }
        );
    }

    #[test]
    fn time_ne_when_different_durations() {
        assert_ne!(
            Time { beats: 1, base: Grid::T4 },
            Time { beats: 1, base: Grid::T8 }
        );
    }

    // ── Property tests ───────────────────────────────────────────

    proptest! {
        /// `time_to_tick` is exact by definition.
        #[test]
        fn time_to_tick_exact(beats in 0u32..=100_000, base in arb_grid()) {
            prop_assert_eq!(
                time_to_tick(Time { beats, base }).0,
                beats * base.tick_count()
            );
        }

        /// `from_ticks` on aligned ticks (every tick at 960 PPQN since
        /// T512P = 1) round-trips exactly.
        #[test]
        fn from_ticks_round_trip_on_aligned(q in 0u32..=1_000_000) {
            // T512P = 1, so q itself is the tick count.
            let n = Tick(q);
            prop_assert_eq!(time_to_tick(from_ticks(n)), n);
        }

        /// `from_ticks` is the identity on tick counts at 960 PPQN.
        #[test]
        fn from_ticks_is_identity_on_ticks(n in arb_tick()) {
            prop_assert_eq!(time_to_tick(from_ticks(n)).0, n.0);
        }

        /// At 960 PPQN, `from_ticks` rounds by 0 (T512P = 1).
        #[test]
        fn from_ticks_rounds_within_t512p(n in arb_tick()) {
            let delta = time_to_tick(from_ticks(n)).0 - n.0;
            prop_assert!(delta < Grid::T512P.tick_count());
        }

        /// The chosen `base` is the coarsest `Grid` whose tick count
        /// divides `n`. No coarser base (larger tick count, earlier in
        /// `Grid::ALL`'s coarsest-first order) divides it.
        #[test]
        fn from_ticks_picks_coarsest_base(n in arb_tick()) {
            let t = from_ticks(n);
            let aligned = time_to_tick(t).0;
            for g in Grid::ALL {
                if g == t.base { break; }
                prop_assert!(
                    aligned % g.tick_count() != 0,
                    "{g:?} (tc={}) also divides {aligned}; should have been picked before {:?} (tc={})",
                    g.tick_count(), t.base, t.base.tick_count()
                );
            }
        }

        /// `Time` equality is tick-count-based: `from_ticks` produces a
        /// representative of the equivalence class.
        #[test]
        fn from_ticks_idempotent(n in arb_tick()) {
            let t1 = from_ticks(n);
            let t2 = from_ticks(time_to_tick(t1));
            prop_assert_eq!(t1, t2);
        }

        /// Any two `Time` values with the same tick count are equal.
        #[test]
        fn time_eq_iff_same_tick_count(t1 in arb_time(), t2 in arb_time()) {
            prop_assert_eq!(t1 == t2, time_to_tick(t1) == time_to_tick(t2));
        }
    }
}
