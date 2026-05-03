//! `Tick` (960 PPQN master counter) and canonical musical `Time`.
//!
//! `Time` equality is by tick count, *not* structural — two
//! finite representations of the same duration (`Time::At { beats:
//! 240, base: T512P }` and `Time::At { beats: 1, base: T16 }` are
//! both 240 ticks at 960 PPQN) compare equal.
//!
//! `from_ticks` is the fallible finite half of the `ticktime` Galois
//! connection: it picks the nicest finite representation when the
//! tick count is exactly representable by `Time`. The total
//! connection uses [`Time::End`] so ticks above the finite `Time`
//! horizon remain explicit instead of being silently clamped.

use std::cmp::Ordering;
use std::hash::{Hash, Hasher};

use crate::time::grid::Grid;

/// Ticks per quarter note. 960 PPQN master resolution.
pub const PPQN: u32 = 960;

/// Master tick counter. Opaque `u64` newtype; one tick is `1/960` of a
/// quarter note. The width is `u64` so `time_to_tick`'s
/// `beats: u32 × tick_count: u32` arithmetic can never overflow — the
/// product of two `u32` values fits in `u64` with ~32 bits of headroom.
#[repr(transparent)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, PartialOrd, Ord, Default)]
pub struct Tick(pub u64);

/// Musical time as either a finite `(count × grid)` value or `End`,
/// the synthetic top value above every finite time.
///
/// Equality and ordering are by tick count, so distinct
/// finite `(beats, base)` pairs denoting the same duration are equal.
/// Use [`from_ticks`] to get the canonical finite representation.
#[derive(Copy, Clone, Debug)]
pub enum Time {
    At { beats: u32, base: Grid },
    End,
}

/// Convert a musical `Time` to absolute ticks. Exact: no rounding,
/// no overflow for finite values — the product `beats × tick_count`
/// is `u32 × u32`, which fits in `Tick`'s `u64` with ~32 bits of
/// headroom. `Time::End` embeds as `Tick(u64::MAX)`.
pub fn time_to_tick(t: Time) -> Tick {
    match t {
        Time::At { beats, base } => Tick(u64::from(beats) * u64::from(base.tick_count())),
        Time::End => Tick(u64::MAX),
    }
}

/// Convert `n` to the nicest exact `Time` representation, when one
/// exists.
///
/// At 960 PPQN with the full 36-element lattice, the bottom is
/// `Grid::T512P` = 1 tick, so every in-range input is already aligned.
/// This helper canonicalises exact finite values by picking the
/// coarsest `Grid` whose tick count divides `n` exactly.
///
/// Returns `None` when `n` has no exact finite representation. Runtime
/// callers (e.g. transport) are free to interpret `None` according to
/// their own boundary semantics; [`TICKTIME`](crate::time::conn::TICKTIME)
/// maps upward overflow to [`Time::End`] and floors high overflow to
/// the greatest finite `Time`.
pub fn from_ticks(n: Tick) -> Option<Time> {
    let prec = u64::from(Grid::T512P.tick_count()); // = 1 at 960 PPQN
    let rounded_up = n.0.div_ceil(prec) * prec;
    nicest_from_tick_count(rounded_up)
}

/// Convert `n` down to the nicest exact `Time` representation at the
/// current bottom-grid alignment, when one exists. At 960 PPQN with
/// `T512P = 1` this equals [`from_ticks`] for exact finite inputs.
pub fn from_ticks_floor(n: Tick) -> Option<Time> {
    let prec = u64::from(Grid::T512P.tick_count());
    let aligned = (n.0 / prec) * prec;
    nicest_from_tick_count(aligned)
}

/// Pick the coarsest `Grid` whose tick count divides `n`, and return
/// the corresponding `Time`. `Grid::ALL` is ordered coarsest-first
/// (binary T1→T256, then triplet, then quintuplet, then p), so the
/// first divisor wins.
///
/// For `n = 0` this returns `Some(Time::At { beats: 0, base: T1 })`
/// (every tick count divides 0). Returns `None` if `n / tc` exceeds
/// `u32::MAX` for the chosen divisor, so values past that horizon
/// have no finite representation.
fn nicest_from_tick_count(n: u64) -> Option<Time> {
    for g in Grid::ALL {
        let tc = u64::from(g.tick_count());
        if n % tc == 0 {
            let beats = n / tc;
            return u32::try_from(beats)
                .ok()
                .map(|beats| Time::At { beats, base: g });
        }
    }
    unreachable!("Grid::T512P (tick_count = 1) divides every u64 value");
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::arb::arb_grid;
    use crate::time::arb::{arb_tick, arb_time};
    use proptest::prelude::*;

    // ── Spot checks ───────────────────────────────────────────────

    #[test]
    fn ppqn_is_960() {
        assert_eq!(PPQN, 960);
    }

    #[test]
    fn time_to_tick_quarter_note() {
        assert_eq!(
            time_to_tick(Time::At {
                beats: 1,
                base: Grid::T4
            }),
            Tick(960)
        );
    }

    #[test]
    fn time_to_tick_two_eighths() {
        assert_eq!(
            time_to_tick(Time::At {
                beats: 2,
                base: Grid::T8
            }),
            Tick(960)
        );
    }

    #[test]
    fn from_ticks_240_is_one_sixteenth() {
        assert_eq!(
            from_ticks(Tick(240)),
            Some(Time::At {
                beats: 1,
                base: Grid::T16
            })
        );
    }

    #[test]
    fn from_ticks_192_is_one_quintuplet_eighth() {
        // T8Q = 192 ticks (5-per-quarter quintuplet).
        assert_eq!(
            from_ticks(Tick(192)),
            Some(Time::At {
                beats: 1,
                base: Grid::T8Q
            })
        );
    }

    #[test]
    fn from_ticks_160_is_one_triplet_sixteenth() {
        assert_eq!(
            from_ticks(Tick(160)),
            Some(Time::At {
                beats: 1,
                base: Grid::T16T
            })
        );
    }

    #[test]
    fn from_ticks_960_is_one_quarter() {
        assert_eq!(
            from_ticks(Tick(960)),
            Some(Time::At {
                beats: 1,
                base: Grid::T4
            })
        );
    }

    #[test]
    fn from_ticks_1_is_one_t512p() {
        // 1 tick = T512P. Coarsest divisor is T512P itself.
        assert_eq!(
            from_ticks(Tick(1)),
            Some(Time::At {
                beats: 1,
                base: Grid::T512P
            })
        );
    }

    #[test]
    fn from_ticks_unaligned_round_trip_at_t512p_grid() {
        // At PPQN=960, every tick aligns to T512P (= 1), so floor and
        // ceiling collapse — every tick is its own canonical form.
        for n in [0u64, 1, 2, 50, 100, 1000, 1234, 100_000] {
            assert_eq!(from_ticks(Tick(n)), from_ticks_floor(Tick(n)));
            assert_eq!(time_to_tick(from_ticks(Tick(n)).unwrap()).0, n);
        }
    }

    #[test]
    fn from_ticks_zero_is_top() {
        // 0 % 3840 == 0, so the coarsest grid wins.
        assert_eq!(
            from_ticks(Tick(0)),
            Some(Time::At {
                beats: 0,
                base: Grid::T1
            })
        );
    }

    #[test]
    fn from_ticks_some_at_horizon() {
        // u32::MAX × Grid::T1.tick_count() is the largest tick count
        // that has a representable Time (`beats: u32::MAX, base: T1`).
        let n = u64::from(u32::MAX) * u64::from(Grid::T1.tick_count());
        assert_eq!(
            from_ticks(Tick(n)),
            Some(Time::At {
                beats: u32::MAX,
                base: Grid::T1
            })
        );
    }

    #[test]
    fn from_ticks_none_above_horizon() {
        // One tick past the horizon: not divisible by T1 (3840), so
        // the algorithm falls through to T512P (tc=1) and tries
        // beats = n/1, which exceeds u32::MAX → None.
        let n = u64::from(u32::MAX) * u64::from(Grid::T1.tick_count()) + 1;
        assert_eq!(from_ticks(Tick(n)), None);
    }

    #[test]
    fn from_ticks_none_at_u64_max() {
        assert_eq!(from_ticks(Tick(u64::MAX)), None);
    }

    #[test]
    fn time_eq_by_tick_count() {
        // Different (beats, base) pairs but same duration.
        // At 960 PPQN: T64 = 60, T32 = 120. 2 × 60 = 1 × 120.
        assert_eq!(
            Time::At {
                beats: 2,
                base: Grid::T64
            },
            Time::At {
                beats: 1,
                base: Grid::T32
            }
        );
        assert_eq!(
            Time::At {
                beats: 2,
                base: Grid::T8
            },
            Time::At {
                beats: 1,
                base: Grid::T4
            }
        );
    }

    #[test]
    fn time_ne_when_different_durations() {
        assert_ne!(
            Time::At {
                beats: 1,
                base: Grid::T4
            },
            Time::At {
                beats: 1,
                base: Grid::T8
            }
        );
    }

    // ── Property tests ───────────────────────────────────────────

    proptest! {
        /// `time_to_tick` is exact by definition. With `Tick: u64`,
        /// `beats: u32 × tick_count: u32` cannot overflow.
        #[test]
        fn time_to_tick_exact(beats in any::<u32>(), base in arb_grid()) {
            prop_assert_eq!(
                time_to_tick(Time::At { beats, base }).0,
                u64::from(beats) * u64::from(base.tick_count())
            );
        }

        /// `time_to_tick` never panics for any `(beats: u32, base: Grid)` —
        /// regression for the old `checked_mul().expect()` panic path.
        #[test]
        fn time_to_tick_never_panics(beats in any::<u32>(), base in arb_grid()) {
            let _ = time_to_tick(Time::At { beats, base });
        }

        /// `from_ticks` on aligned ticks (every tick at 960 PPQN since
        /// T512P = 1) round-trips exactly.
        #[test]
        fn from_ticks_round_trip_on_aligned(q in 0u64..=1_000_000) {
            // T512P = 1, so q itself is the tick count.
            let n = Tick(q);
            prop_assert_eq!(time_to_tick(from_ticks(n).unwrap()), n);
        }

        /// `from_ticks` is the identity on tick counts at 960 PPQN.
        #[test]
        fn from_ticks_is_identity_on_ticks(n in arb_tick()) {
            prop_assert_eq!(time_to_tick(from_ticks(n).unwrap()).0, n.0);
        }

        /// At 960 PPQN, `from_ticks` rounds by 0 (T512P = 1).
        #[test]
        fn from_ticks_rounds_within_t512p(n in arb_tick()) {
            let delta = time_to_tick(from_ticks(n).unwrap()).0 - n.0;
            prop_assert!(delta < u64::from(Grid::T512P.tick_count()));
        }

        /// The chosen `base` is the coarsest `Grid` whose tick count
        /// divides `n`. No coarser base (larger tick count, earlier in
        /// `Grid::ALL`'s coarsest-first order) divides it.
        #[test]
        fn from_ticks_picks_coarsest_base(n in arb_tick()) {
            let t = from_ticks(n).unwrap();
            let aligned = time_to_tick(t).0;
            let Time::At { base, .. } = t else {
                unreachable!("from_ticks returns only finite Time::At values");
            };
            for g in Grid::ALL {
                if g == base { break; }
                prop_assert!(
                    aligned % u64::from(g.tick_count()) != 0,
                    "{g:?} (tc={}) also divides {aligned}; should have been picked before {:?} (tc={})",
                    g.tick_count(), base, base.tick_count()
                );
            }
        }

        /// `Time` equality is tick-count-based: `from_ticks` produces a
        /// representative of the equivalence class.
        #[test]
        fn from_ticks_idempotent(n in arb_tick()) {
            let t1 = from_ticks(n).unwrap();
            let t2 = from_ticks(time_to_tick(t1)).unwrap();
            prop_assert_eq!(t1, t2);
        }

        /// Any two `Time` values with the same tick count are equal.
        #[test]
        fn time_eq_iff_same_tick_count(t1 in arb_time(), t2 in arb_time()) {
            prop_assert_eq!(t1 == t2, time_to_tick(t1) == time_to_tick(t2));
        }
    }
}
