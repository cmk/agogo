//! `Tick` (192 PPQN master counter) and canonical `Time { beats, base }`.
//!
//! Port of the `Tick` newtype and `Time` record from the Cirklon Haskell
//! module. `Time` equality is by tick count, *not* structural — two
//! representations of the same duration (`Time { 12, T128t }` and
//! `Time { 1, T16 }` are both 48 ticks) compare equal.
//!
//! `from_ticks` matches Haskell `fromTicks = ceiling ticks`: it rounds
//! the input up to the T128t grid (4 ticks) then picks the nicest
//! representation — coarsest `TBase` whose tick count divides the
//! rounded value, giving the smallest `beats`.

use std::cmp::Ordering;
use std::hash::{Hash, Hasher};

use connections::order::Ple;

use crate::time::tbase::TBase;

/// Ticks per quarter note. 192-PPQN master resolution.
pub const PPQN: u32 = 192;

/// Master tick counter. Opaque `u32` newtype; one tick is `1/192` of a
/// quarter note.
#[repr(transparent)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, PartialOrd, Ord, Default)]
pub struct Tick(pub u32);

impl Ple for Tick {
    fn ple(&self, other: &Self) -> bool {
        self.0 <= other.0
    }
}

/// Musical time as (count × grid): `beats` positions on a grid of
/// resolution `base`.
///
/// Equality and ordering are by tick count, so distinct
/// `(beats, base)` pairs denoting the same duration are equal. Use
/// [`from_ticks`] to get the canonical representation.
#[derive(Copy, Clone, Debug)]
pub struct Time {
    pub beats: u32,
    pub base: TBase,
}

/// Convert a musical `Time` to absolute ticks. Exact: no rounding.
pub fn time_to_tick(t: Time) -> Tick {
    Tick(t.beats * t.base.tick_count())
}

/// Round `n` up to the nicest `Time` representation.
///
/// The input is first aligned to the T128t grid (the finest resolution,
/// 4 ticks); then the coarsest `TBase` whose tick count divides the
/// aligned value is selected, giving the smallest possible `beats`.
///
/// For aligned `n` (multiples of 4) this is an exact canonicalisation;
/// for unaligned `n` it rounds up.
pub fn from_ticks(n: Tick) -> Time {
    let prec = TBase::T128t.tick_count();
    let aligned = n.0.div_ceil(prec) * prec;
    nicest_from_tick_count(aligned)
}

/// Pick the coarsest `TBase` whose tick count divides `n`, and return
/// the corresponding `Time`. `TBase::ALL` is ordered coarsest-first
/// (straight coarse→fine, then triplet coarse→fine), so the first
/// divisor wins.
///
/// For `n = 0` this returns `Time { beats: 0, base: T1 }` (every
/// tick count divides 0, so the loop halts on the first element).
fn nicest_from_tick_count(n: u32) -> Time {
    for tb in TBase::ALL {
        let tc = tb.tick_count();
        if n % tc == 0 {
            return Time {
                beats: n / tc,
                base: tb,
            };
        }
    }
    unreachable!("T128t tick_count (4) always divides an aligned tick count");
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
    use crate::arb::{arb_tbase, arb_tick, arb_time};
    use proptest::prelude::*;

    // ── Spot checks ───────────────────────────────────────────────

    #[test]
    fn ppqn_is_192() {
        assert_eq!(PPQN, 192);
    }

    #[test]
    fn time_to_tick_quarter_note() {
        assert_eq!(
            time_to_tick(Time {
                beats: 1,
                base: TBase::T4
            }),
            Tick(192)
        );
    }

    #[test]
    fn time_to_tick_two_eighths() {
        assert_eq!(
            time_to_tick(Time {
                beats: 2,
                base: TBase::T8
            }),
            Tick(192)
        );
    }

    #[test]
    fn from_ticks_48_is_one_sixteenth() {
        assert_eq!(
            from_ticks(Tick(48)),
            Time {
                beats: 1,
                base: TBase::T16
            }
        );
    }

    #[test]
    fn from_ticks_64_is_one_eighth_triplet() {
        assert_eq!(
            from_ticks(Tick(64)),
            Time {
                beats: 1,
                base: TBase::T8t
            }
        );
    }

    #[test]
    fn from_ticks_192_is_one_quarter() {
        assert_eq!(
            from_ticks(Tick(192)),
            Time {
                beats: 1,
                base: TBase::T4
            }
        );
    }

    #[test]
    fn from_ticks_unaligned_rounds_up_to_t128t_grid() {
        // 50 ticks: round up to 52 (next multiple of 4), then nicest.
        // 52 is not divisible by any TBase coarser than T128t.
        assert_eq!(
            from_ticks(Tick(50)),
            Time {
                beats: 13,
                base: TBase::T128t
            }
        );
    }

    #[test]
    fn from_ticks_zero_is_nicest_top() {
        // 0 % 768 == 0, so the coarsest grid picks itself.
        assert_eq!(
            from_ticks(Tick(0)),
            Time {
                beats: 0,
                base: TBase::T1
            }
        );
    }

    #[test]
    fn time_eq_by_tick_count() {
        // Structurally different, same duration.
        assert_eq!(
            Time {
                beats: 12,
                base: TBase::T128t
            },
            Time {
                beats: 1,
                base: TBase::T16
            }
        );
        assert_eq!(
            Time {
                beats: 2,
                base: TBase::T8
            },
            Time {
                beats: 1,
                base: TBase::T4
            }
        );
    }

    #[test]
    fn time_ne_when_different_durations() {
        assert_ne!(
            Time {
                beats: 1,
                base: TBase::T4
            },
            Time {
                beats: 1,
                base: TBase::T8
            }
        );
    }

    // ── Property tests ───────────────────────────────────────────

    proptest! {
        /// `time_to_tick` is exact by definition: beats × base-tick-count.
        #[test]
        fn time_to_tick_exact(beats in 0u32..=100_000, base in arb_tbase()) {
            prop_assert_eq!(
                time_to_tick(Time { beats, base }).0,
                beats * base.tick_count()
            );
        }

        /// `from_ticks` on aligned ticks round-trips exactly.
        #[test]
        fn from_ticks_round_trip_on_aligned(q in 0u32..=100_000) {
            let n = Tick(q * TBase::T128t.tick_count()); // aligned to T128t
            prop_assert_eq!(time_to_tick(from_ticks(n)), n);
        }

        /// `from_ticks` never loses ticks: the returned `Time` covers at
        /// least `n` (ceiling rounding).
        #[test]
        fn from_ticks_is_ceiling(n in arb_tick()) {
            prop_assert!(time_to_tick(from_ticks(n)).0 >= n.0);
        }

        /// `from_ticks` rounds by at most `T128t.tick_count() - 1` ticks.
        #[test]
        fn from_ticks_rounds_within_t128t(n in arb_tick()) {
            let delta = time_to_tick(from_ticks(n)).0 - n.0;
            prop_assert!(delta < TBase::T128t.tick_count());
        }

        /// The chosen `base` is the coarsest `TBase` whose tick count
        /// divides the T128t-aligned tick count of `n`. No coarser base
        /// (larger tick count in `TBase::ALL`'s coarsest-first order)
        /// divides it.
        #[test]
        fn from_ticks_picks_coarsest_base(n in arb_tick()) {
            let t = from_ticks(n);
            let aligned = time_to_tick(t).0;
            for tb in TBase::ALL {
                if tb == t.base { break; }
                prop_assert!(
                    aligned % tb.tick_count() != 0,
                    "{tb:?} (tc={}) also divides {aligned}; should have been picked before {:?} (tc={})",
                    tb.tick_count(),
                    t.base,
                    t.base.tick_count()
                );
            }
        }

        /// `Time` equality is tick-count-based: `from_ticks` produces a
        /// representative of the equivalence class, so putting it
        /// through the round trip leaves it fixed.
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
