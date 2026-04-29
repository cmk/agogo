//! Galois connections for `Tick`, `Time`, `Rational`, and `Grid`.
//!
//! Five `Conn<A, B>` values port the Haskell Cirklon connections:
//!
//! | Rust            | Haskell      | Shape                        |
//! |-----------------|--------------|------------------------------|
//! | [`TICKTIME`]    | `ticks`      | `Conn<Tick, Time>`           |
//! | [`WHOLTICK`]    | `ratTick`    | `Conn<Whole, Tick>`          |
//! | [`quantize_at`] | `quantizeAt` | `Conn<Tick, Time>` per Grid  |
//! | [`TIMETIME`]    | `time`       | `Conn<(Time, Time), Time>`   |
//! | [`GRIDGRID`]    | `tbase`      | `Conn<(Grid, Grid), Grid>`   |
//!
//! Naming: per CLAUDE.md, Conn accessors are 8-char identifiers
//! built from two 4-char side names. Single-type-side Conns
//! (`TICKTIME`, `WHOLTICK`) follow the rule directly. Pair-side Conns
//! (`TIMETIME`, `GRIDGRID`) duplicate the side name. `quantize_at` is
//! a Conn *constructor* (parametric family), not a Conn constant —
//! exempt from the 8-char rule, since each instance is named by the
//! parameter `g: Grid`.
//!
//! All use bare `fn` pointers from [`connections::conn::Conn`] — no
//! closure capture, tempo-independent. `Conn::new` is `const fn`
//! upstream, so the four single-type-side Conns are exposed as
//! `pub const` constants matching the convention used by upstream's
//! `F032F016` / `F064FD12` / similar. `quantize_at` stays a function
//! because its inner / ceil / floor pointers vary per `Grid` value.
//!
//! **Orientation of `timetime` and `gridgrid`.** These are lattice
//! connections: the pair side carries the divisibility product order,
//! not magnitude. Following the Haskell convention,
//! `ceil = meet (GCD)` and `floor = join (LCM)`. The generic
//! adjoint-law tests from `connections/src/conn.rs` use a single
//! `PartialOrd` and therefore need a *divisibility* `≤` on the
//! input/output side — `Time`'s and `Grid`'s magnitude order would
//! give a non-adjoint structure. The tests below build ad-hoc
//! `*_refine_le` helpers for that reason.

use connections::conn::Conn;
use num_rational::Rational64;

use crate::time::grid::Grid;
use crate::time::tick::{PPQN, Tick, Time, from_ticks, from_ticks_floor, time_to_tick};
use connections::lattice::{Join, Meet};

/// A rational whole-note duration. `Whole::new(1, 4)` = quarter note.
pub type Whole = Rational64;

/// Ticks per whole note at 960 PPQN. `4 * PPQN`.
const TPW: i64 = (4 * PPQN) as i64;

// ── ticktime: Conn<Tick, Time> ───────────────────────────────────

/// Precondition: `n.0 ≤ u32::MAX × Grid::T1.tick_count()`. The
/// `ticktime` Conn unwraps `from_ticks` here because `Conn::ceil` is
/// total `fn(Tick) -> Time`. Proptest callers stay inside the
/// horizon via [`arb_tick`](crate::arb::arb_tick); runtime callers
/// (transport, scheduler) call `from_ticks` directly and pick their
/// own out-of-range semantics.
fn ticktime_ceil(n: Tick) -> Time {
    from_ticks(n).expect("ticktime Conn requires n ≤ u32::MAX × Grid::T1.tick_count()")
}

fn ticktime_inner(t: Time) -> Tick {
    time_to_tick(t)
}

fn ticktime_floor(n: Tick) -> Time {
    from_ticks_floor(n).expect("ticktime Conn requires n ≤ u32::MAX × Grid::T1.tick_count()")
}

/// Master `Tick ↔ Time` connection. Ceiling rounds up to the
/// `Grid::T512P` grid (= 1 tick at 960 PPQN, so every tick is
/// already aligned) then canonicalises; floor rounds down; embed is
/// exact.
pub const TICKTIME: Conn<Tick, Time> =
    Conn::new(ticktime_ceil, ticktime_inner, ticktime_floor);

// ── wholtick: Conn<Whole, Tick> ──────────────────────────────────

fn tpw_rational() -> Rational64 {
    Rational64::new(TPW, 1)
}

// Clamp an `i64` into the non-negative `u64` range. Saturates at 0
// so a negative rational produces `Tick(0)` rather than wrapping.
// `i64::MAX` maps to `Tick(i64::MAX as u64)`, well inside Tick's u64
// horizon.
fn i64_to_tick(n: i64) -> Tick {
    Tick(n.max(0) as u64)
}

fn wholtick_ceil(r: Whole) -> Tick {
    let ceil = (r * tpw_rational()).ceil().to_integer();
    i64_to_tick(ceil)
}

fn wholtick_inner(n: Tick) -> Whole {
    // Tick is u64; Whole's numerator is i64. Saturate at i64::MAX so
    // Tick values past i64's positive range produce a finite (large)
    // rational rather than wrapping. In practice arb_tick is capped
    // well below i64::MAX.
    let num = i64::try_from(n.0).unwrap_or(i64::MAX);
    Rational64::new(num, TPW)
}

fn wholtick_floor(r: Whole) -> Tick {
    let floor = (r * tpw_rational()).floor().to_integer();
    i64_to_tick(floor)
}

/// Galois connection between rational whole-note durations and ticks.
/// Floor rounds down, ceiling rounds up, embed is exact:
/// `wholtick_inner(Tick(n)) = n / 3840` at 960 PPQN.
pub const WHOLTICK: Conn<Whole, Tick> =
    Conn::new(wholtick_ceil, wholtick_inner, wholtick_floor);

// ── quantize_at: Conn<Tick, Time> per Grid ───────────────────────

fn qa_inner(t: Time) -> Tick {
    time_to_tick(t)
}

macro_rules! qa_variant {
    ($variant:ident, $ceil:ident, $floor:ident) => {
        fn $ceil(n: Tick) -> Time {
            let tc = u64::from(Grid::$variant.tick_count());
            let beats = n.0.div_ceil(tc);
            Time {
                beats: u32::try_from(beats).expect(
                    "quantize_at Conn requires n.0.div_ceil(tc) ≤ u32::MAX",
                ),
                base: Grid::$variant,
            }
        }
        fn $floor(n: Tick) -> Time {
            let tc = u64::from(Grid::$variant.tick_count());
            let beats = n.0 / tc;
            Time {
                beats: u32::try_from(beats).expect(
                    "quantize_at Conn requires n.0 / tc ≤ u32::MAX",
                ),
                base: Grid::$variant,
            }
        }
    };
}

// Binary track (9)
qa_variant!(T1, qa_t1_ceil, qa_t1_floor);
qa_variant!(T2, qa_t2_ceil, qa_t2_floor);
qa_variant!(T4, qa_t4_ceil, qa_t4_floor);
qa_variant!(T8, qa_t8_ceil, qa_t8_floor);
qa_variant!(T16, qa_t16_ceil, qa_t16_floor);
qa_variant!(T32, qa_t32_ceil, qa_t32_floor);
qa_variant!(T64, qa_t64_ceil, qa_t64_floor);
qa_variant!(T128, qa_t128_ceil, qa_t128_floor);
qa_variant!(T256, qa_t256_ceil, qa_t256_floor);

// Triplet track (9)
qa_variant!(T2T, qa_t2t_ceil, qa_t2t_floor);
qa_variant!(T4T, qa_t4t_ceil, qa_t4t_floor);
qa_variant!(T8T, qa_t8t_ceil, qa_t8t_floor);
qa_variant!(T16T, qa_t16t_ceil, qa_t16t_floor);
qa_variant!(T32T, qa_t32t_ceil, qa_t32t_floor);
qa_variant!(T64T, qa_t64t_ceil, qa_t64t_floor);
qa_variant!(T128T, qa_t128t_ceil, qa_t128t_floor);
qa_variant!(T256T, qa_t256t_ceil, qa_t256t_floor);
qa_variant!(T512T, qa_t512t_ceil, qa_t512t_floor);

// Quintuplet track (9)
qa_variant!(T2Q, qa_t2q_ceil, qa_t2q_floor);
qa_variant!(T4Q, qa_t4q_ceil, qa_t4q_floor);
qa_variant!(T8Q, qa_t8q_ceil, qa_t8q_floor);
qa_variant!(T16Q, qa_t16q_ceil, qa_t16q_floor);
qa_variant!(T32Q, qa_t32q_ceil, qa_t32q_floor);
qa_variant!(T64Q, qa_t64q_ceil, qa_t64q_floor);
qa_variant!(T128Q, qa_t128q_ceil, qa_t128q_floor);
qa_variant!(T256Q, qa_t256q_ceil, qa_t256q_floor);
qa_variant!(T512Q, qa_t512q_ceil, qa_t512q_floor);

// 15-tuplet (p) track (9)
qa_variant!(T2P, qa_t2p_ceil, qa_t2p_floor);
qa_variant!(T4P, qa_t4p_ceil, qa_t4p_floor);
qa_variant!(T8P, qa_t8p_ceil, qa_t8p_floor);
qa_variant!(T16P, qa_t16p_ceil, qa_t16p_floor);
qa_variant!(T32P, qa_t32p_ceil, qa_t32p_floor);
qa_variant!(T64P, qa_t64p_ceil, qa_t64p_floor);
qa_variant!(T128P, qa_t128p_ceil, qa_t128p_floor);
qa_variant!(T256P, qa_t256p_ceil, qa_t256p_floor);
qa_variant!(T512P, qa_t512p_ceil, qa_t512p_floor);

/// Quantise a `Tick` to the nearest `Time` on the `g` grid, keeping
/// the result on that grid (no further nicest-coarsening, unlike
/// [`ticktime`]). `fn` pointers can't close over `g`, so dispatch is a
/// per-const `match`.
pub fn quantize_at(g: Grid) -> Conn<Tick, Time> {
    if g == Grid::T1 {
        Conn::new(qa_t1_ceil, qa_inner, qa_t1_floor)
    } else if g == Grid::T2 {
        Conn::new(qa_t2_ceil, qa_inner, qa_t2_floor)
    } else if g == Grid::T4 {
        Conn::new(qa_t4_ceil, qa_inner, qa_t4_floor)
    } else if g == Grid::T8 {
        Conn::new(qa_t8_ceil, qa_inner, qa_t8_floor)
    } else if g == Grid::T16 {
        Conn::new(qa_t16_ceil, qa_inner, qa_t16_floor)
    } else if g == Grid::T32 {
        Conn::new(qa_t32_ceil, qa_inner, qa_t32_floor)
    } else if g == Grid::T64 {
        Conn::new(qa_t64_ceil, qa_inner, qa_t64_floor)
    } else if g == Grid::T128 {
        Conn::new(qa_t128_ceil, qa_inner, qa_t128_floor)
    } else if g == Grid::T256 {
        Conn::new(qa_t256_ceil, qa_inner, qa_t256_floor)
    } else if g == Grid::T2T {
        Conn::new(qa_t2t_ceil, qa_inner, qa_t2t_floor)
    } else if g == Grid::T4T {
        Conn::new(qa_t4t_ceil, qa_inner, qa_t4t_floor)
    } else if g == Grid::T8T {
        Conn::new(qa_t8t_ceil, qa_inner, qa_t8t_floor)
    } else if g == Grid::T16T {
        Conn::new(qa_t16t_ceil, qa_inner, qa_t16t_floor)
    } else if g == Grid::T32T {
        Conn::new(qa_t32t_ceil, qa_inner, qa_t32t_floor)
    } else if g == Grid::T64T {
        Conn::new(qa_t64t_ceil, qa_inner, qa_t64t_floor)
    } else if g == Grid::T128T {
        Conn::new(qa_t128t_ceil, qa_inner, qa_t128t_floor)
    } else if g == Grid::T256T {
        Conn::new(qa_t256t_ceil, qa_inner, qa_t256t_floor)
    } else if g == Grid::T512T {
        Conn::new(qa_t512t_ceil, qa_inner, qa_t512t_floor)
    } else if g == Grid::T2Q {
        Conn::new(qa_t2q_ceil, qa_inner, qa_t2q_floor)
    } else if g == Grid::T4Q {
        Conn::new(qa_t4q_ceil, qa_inner, qa_t4q_floor)
    } else if g == Grid::T8Q {
        Conn::new(qa_t8q_ceil, qa_inner, qa_t8q_floor)
    } else if g == Grid::T16Q {
        Conn::new(qa_t16q_ceil, qa_inner, qa_t16q_floor)
    } else if g == Grid::T32Q {
        Conn::new(qa_t32q_ceil, qa_inner, qa_t32q_floor)
    } else if g == Grid::T64Q {
        Conn::new(qa_t64q_ceil, qa_inner, qa_t64q_floor)
    } else if g == Grid::T128Q {
        Conn::new(qa_t128q_ceil, qa_inner, qa_t128q_floor)
    } else if g == Grid::T256Q {
        Conn::new(qa_t256q_ceil, qa_inner, qa_t256q_floor)
    } else if g == Grid::T512Q {
        Conn::new(qa_t512q_ceil, qa_inner, qa_t512q_floor)
    } else if g == Grid::T2P {
        Conn::new(qa_t2p_ceil, qa_inner, qa_t2p_floor)
    } else if g == Grid::T4P {
        Conn::new(qa_t4p_ceil, qa_inner, qa_t4p_floor)
    } else if g == Grid::T8P {
        Conn::new(qa_t8p_ceil, qa_inner, qa_t8p_floor)
    } else if g == Grid::T16P {
        Conn::new(qa_t16p_ceil, qa_inner, qa_t16p_floor)
    } else if g == Grid::T32P {
        Conn::new(qa_t32p_ceil, qa_inner, qa_t32p_floor)
    } else if g == Grid::T64P {
        Conn::new(qa_t64p_ceil, qa_inner, qa_t64p_floor)
    } else if g == Grid::T128P {
        Conn::new(qa_t128p_ceil, qa_inner, qa_t128p_floor)
    } else if g == Grid::T256P {
        Conn::new(qa_t256p_ceil, qa_inner, qa_t256p_floor)
    } else if g == Grid::T512P {
        Conn::new(qa_t512p_ceil, qa_inner, qa_t512p_floor)
    } else {
        unreachable!("Grid::ALL is exhaustive — every Grid value matched above")
    }
}

// ── timetime: Conn<(Time, Time), Time> ───────────────────────────

fn gcd_u64(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a
}

fn lcm_u64(a: u64, b: u64) -> u64 {
    if a == 0 || b == 0 {
        0
    } else {
        a / gcd_u64(a, b) * b
    }
}

fn timetime_ceil(ab: (Time, Time)) -> Time {
    let (a, b) = ab;
    let g = gcd_u64(time_to_tick(a).0, time_to_tick(b).0);
    from_ticks(Tick(g)).expect("timetime_ceil: GCD of representable ticks is itself representable")
}

fn timetime_inner(t: Time) -> (Time, Time) {
    (t, t)
}

fn timetime_floor(ab: (Time, Time)) -> Time {
    let (a, b) = ab;
    let l = lcm_u64(time_to_tick(a).0, time_to_tick(b).0);
    // LCM can exceed `u32::MAX × Grid::T1.tick_count()` in general;
    // property tests bound inputs via `arb_small_time` so `l` stays
    // representable. For larger inputs `from_ticks` returns `None`
    // and we panic loudly rather than silently picking a wrong value.
    from_ticks(Tick(l)).expect("timetime_floor: LCM exceeds the from_ticks horizon")
}

/// Divisibility-lattice connection on `Time`.
///
/// `ceil = meet (GCD)`, `floor = join (LCM)`, `inner = diagonal`.
/// Following Haskell convention — the relevant order here is
/// divisibility of tick counts, not magnitude.
///
/// # Panics
///
/// `floor` panics if the LCM of the two input tick counts exceeds
/// the `from_ticks` horizon (`u32::MAX × Grid::T1.tick_count()`).
/// For musically-bounded `Time` values this is unreachable; tests
/// use `arb_small_time` (tick counts ≤ 192_000) to stay safely
/// bounded.
pub const TIMETIME: Conn<(Time, Time), Time> =
    Conn::new(timetime_ceil, timetime_inner, timetime_floor);

// ── gridgrid: Conn<(Grid, Grid), Grid> ───────────────────────────

fn gridgrid_ceil(ab: (Grid, Grid)) -> Grid {
    let (a, b) = ab;
    a.meet(&b)
}

fn gridgrid_inner(t: Grid) -> (Grid, Grid) {
    (t, t)
}

fn gridgrid_floor(ab: (Grid, Grid)) -> Grid {
    let (a, b) = ab;
    a.join(&b)
}

/// Divisibility-lattice connection on `Grid`. `ceil = meet (GCD of
/// tick counts)`, `floor = join (LCM)`, `inner = diagonal`.
pub const GRIDGRID: Conn<(Grid, Grid), Grid> =
    Conn::new(gridgrid_ceil, gridgrid_inner, gridgrid_floor);

// `SampleTickConn` (the tempo-coupled Sample↔Tick bridge) moved to
// `crate::sync::sample_tick` (Plan 2026-04-28-03 T3) to satisfy the
// "no tempo coupling" invariant on the `time/` module.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arb::{arb_grid, arb_rational_nonneg, arb_small_time, arb_tick, arb_time};
    use proptest::prelude::*;

    // ── Spot checks ──────────────────────────────────────────────

    #[test]
    fn ticktime_inner_is_exact() {
        let c = TICKTIME;
        let t = Time {
            beats: 3,
            base: Grid::T16,
        };
        // T16 = 240 ticks at 960 PPQN; 3 × 240 = 720.
        assert_eq!(c.inner(t), Tick(720));
    }

    #[test]
    fn ticktime_ceil_aligned() {
        let c = TICKTIME;
        // 240 ticks → exactly 1 T16.
        assert_eq!(
            c.ceil(Tick(240)),
            Time {
                beats: 1,
                base: Grid::T16
            }
        );
    }

    #[test]
    fn ticktime_floor_unaligned() {
        let c = TICKTIME;
        // 50 ticks: T512P = 1, so exact (no rounding); coarsest divisor
        // of 50 in the lattice.  50 = 2 · 5² → factors out 5 (q-flag);
        // 50 / Grid::T256Q.tick_count() should hit. T256Q = 6, doesn't
        // divide 50. Walk the list: T512P=1 divides everything → 50 T512P.
        let t = c.floor(Tick(50));
        assert_eq!(time_to_tick(t).0, 50);
    }

    #[test]
    fn ticktime_ceil_unaligned() {
        let c = TICKTIME;
        // At 960 PPQN every Tick is on Grid::T512P (=1). So ceil and
        // floor both yield the canonical form for `n` itself.
        let t = c.ceil(Tick(50));
        assert_eq!(time_to_tick(t).0, 50);
    }

    #[test]
    fn wholtick_quarter_is_960() {
        let c = WHOLTICK;
        assert_eq!(c.floor(Rational64::new(1, 4)), Tick(960));
        assert_eq!(c.ceil(Rational64::new(1, 4)), Tick(960));
    }

    #[test]
    fn wholtick_three_sixteenths_is_720() {
        let c = WHOLTICK;
        // 3/16 × 3840 = 720.
        assert_eq!(c.floor(Rational64::new(3, 16)), Tick(720));
    }

    #[test]
    fn wholtick_one_seventh_ceils_correctly() {
        // 3840 / 7 = 548.57…, ceil = 549, floor = 548.
        let c = WHOLTICK;
        assert_eq!(c.ceil(Rational64::new(1, 7)), Tick(549));
        assert_eq!(c.floor(Rational64::new(1, 7)), Tick(548));
    }

    #[test]
    fn quantize_at_t16_aligned() {
        let c = quantize_at(Grid::T16);
        // 240 ticks = 1 T16 step at 960 PPQN.
        assert_eq!(
            c.floor(Tick(240)),
            Time {
                beats: 1,
                base: Grid::T16
            }
        );
        assert_eq!(
            c.ceil(Tick(240)),
            Time {
                beats: 1,
                base: Grid::T16
            }
        );
    }

    #[test]
    fn quantize_at_t16_unaligned_splits_on_grid() {
        let c = quantize_at(Grid::T16);
        // 250 ticks: floor = 240/240 = 1 T16; ceil = 480/240 = 2 T16.
        assert_eq!(
            c.floor(Tick(250)),
            Time {
                beats: 1,
                base: Grid::T16
            }
        );
        assert_eq!(
            c.ceil(Tick(250)),
            Time {
                beats: 2,
                base: Grid::T16
            }
        );
    }

    #[test]
    fn timetime_ceil_gcd_of_t4_t8() {
        let c = TIMETIME;
        let a = Time {
            beats: 1,
            base: Grid::T4,
        }; // 960 ticks
        let b = Time {
            beats: 1,
            base: Grid::T8,
        }; // 480 ticks
        // gcd(960, 480) = 480 → Time 1 T8
        assert_eq!(
            c.ceil((a, b)),
            Time {
                beats: 1,
                base: Grid::T8
            }
        );
    }

    #[test]
    fn timetime_floor_lcm_of_t16_and_t16t() {
        let c = TIMETIME;
        let a = Time {
            beats: 1,
            base: Grid::T16,
        }; // 240 ticks
        let b = Time {
            beats: 1,
            base: Grid::T16T,
        }; // 160 ticks
        // lcm(240, 160) = 480 → Time 1 T8
        assert_eq!(
            c.floor((a, b)),
            Time {
                beats: 1,
                base: Grid::T8
            }
        );
    }

    #[test]
    fn gridgrid_ceil_meet_of_t4_t8() {
        let c = GRIDGRID;
        // gcd of tick counts: gcd(960, 480) = 480 = T8.
        assert_eq!(c.ceil((Grid::T4, Grid::T8)), Grid::T8);
    }

    #[test]
    fn gridgrid_floor_join_of_t4_t8t() {
        let c = GRIDGRID;
        // T4 = 960, T8T = 320; lcm(960, 320) = 960 = T4.
        assert_eq!(c.floor((Grid::T4, Grid::T8T)), Grid::T4);
    }

    // ── Generic connections-tests laws for magnitude connections ──

    proptest! {
        // ── ticktime ─────────────────────────────────────────────

        #[test]
        fn ticktime_adjoint(a in arb_tick(), b in arb_time()) {
            let c = TICKTIME;
            let lhs = c.ceil(a) <= b;
            let rhs = a <= c.inner(b);
            prop_assert_eq!(lhs, rhs);
        }

        #[test]
        fn ticktime_closed(a in arb_tick()) {
            let c = TICKTIME;
            prop_assert!(a <= c.inner(c.ceil(a)));
        }

        #[test]
        fn ticktime_kernel(b in arb_time()) {
            let c = TICKTIME;
            prop_assert!(c.ceil(c.inner(b)) <= b);
        }

        #[test]
        fn ticktime_monotonic(
            a1 in arb_tick(), a2 in arb_tick(),
            b1 in arb_time(), b2 in arb_time(),
        ) {
            let c = TICKTIME;
            if a1 <= a2 {
                prop_assert!(c.ceil(a1) <= c.ceil(a2));
            }
            if b1 <= b2 {
                prop_assert!(c.inner(b1) <= c.inner(b2));
            }
        }

        #[test]
        fn ticktime_idempotent(a in arb_tick()) {
            let c = TICKTIME;
            let once = c.inner(c.ceil(a));
            let twice = c.inner(c.ceil(once));
            prop_assert_eq!(once, twice);
        }

        /// At 960 PPQN, every tick is on `Grid::T512P` (= 1) so this
        /// is the identity on every tick.
        #[test]
        fn ticktime_round_trip_on_aligned(q in 0u64..=1_000_000) {
            let c = TICKTIME;
            let n = Tick(q * u64::from(Grid::T512P.tick_count()));
            prop_assert_eq!(c.inner(c.floor(n)), n);
        }

        // ── wholtick ─────────────────────────────────────────────

        #[test]
        fn wholtick_adjoint(a in arb_rational_nonneg(), b in arb_tick()) {
            let c = WHOLTICK;
            let lhs = c.ceil(a) <= b;
            let rhs = a <= c.inner(b);
            prop_assert_eq!(lhs, rhs);
        }

        #[test]
        fn wholtick_closed(a in arb_rational_nonneg()) {
            let c = WHOLTICK;
            prop_assert!(a <= c.inner(c.ceil(a)));
        }

        #[test]
        fn wholtick_kernel(b in arb_tick()) {
            let c = WHOLTICK;
            prop_assert!(c.ceil(c.inner(b)) <= b);
        }

        #[test]
        fn wholtick_monotonic(
            a1 in arb_rational_nonneg(), a2 in arb_rational_nonneg(),
            b1 in arb_tick(), b2 in arb_tick(),
        ) {
            let c = WHOLTICK;
            if a1 <= a2 {
                prop_assert!(c.ceil(a1) <= c.ceil(a2));
            }
            if b1 <= b2 {
                prop_assert!(c.inner(b1) <= c.inner(b2));
            }
        }

        #[test]
        fn wholtick_idempotent(a in arb_rational_nonneg()) {
            let c = WHOLTICK;
            let once = c.inner(c.ceil(a));
            let twice = c.inner(c.ceil(once));
            prop_assert_eq!(once, twice);
        }

        #[test]
        fn wholtick_floor_monotone(
            a in arb_rational_nonneg(), b in arb_rational_nonneg(),
        ) {
            let c = WHOLTICK;
            if a <= b {
                prop_assert!(c.floor(a) <= c.floor(b));
            }
        }

        // ── quantize_at ──────────────────────────────────────────
        //
        // `c.ceil(n)` returns `Time { beats: n.0.div_ceil(tc), base: g }`
        // where `tc = g.tick_count()`. For `n` near the `arb_tick`
        // horizon and `g` finer than `T1`, `beats` can exceed
        // `u32::MAX` and the macro's `try_from` panics. `arb_tick`
        // is now capped at `u32::MAX × Grid::T1.tick_count()` so the
        // upper anchor only fits at `g == T1`; the per-property
        // `prop_assume!(ceil_fits(n, g))` filters everything else.
        // Composing `time_to_tick(c.ceil(n))` is then panic-free,
        // and `<=` (which means magnitude on `Tick` and `Time` and
        // divisibility on `Grid`) replaces the old `.ple()` calls.

        #[test]
        fn quantize_at_brackets_input(
            g in arb_grid(), n in arb_tick(),
        ) {
            let c = quantize_at(g);
            prop_assume!(ceil_fits(n, g));
            let lo = time_to_tick(c.floor(n));
            let hi = time_to_tick(c.ceil(n));
            prop_assert!(lo <= n);
            prop_assert!(n <= hi);
        }

        #[test]
        fn quantize_at_aligned_inner_round_trip(
            g in arb_grid(), q in 0u32..=10_000,
        ) {
            let c = quantize_at(g);
            let n = Tick(u64::from(q) * u64::from(g.tick_count()));
            prop_assert_eq!(c.inner(c.floor(n)), n);
            prop_assert_eq!(c.inner(c.ceil(n)), n);
        }

        #[test]
        fn quantize_at_adjoint(
            g in arb_grid(), n in arb_tick(), k in 0u32..=10_000,
        ) {
            let c = quantize_at(g);
            prop_assume!(ceil_fits(n, g));
            let t = Time { beats: k, base: g };
            let lhs = c.ceil(n) <= t;
            let rhs = n <= c.inner(t);
            prop_assert_eq!(lhs, rhs);
        }

        #[test]
        fn quantize_at_closed(g in arb_grid(), n in arb_tick()) {
            let c = quantize_at(g);
            prop_assume!(ceil_fits(n, g));
            prop_assert!(n <= c.inner(c.ceil(n)));
        }

        #[test]
        fn quantize_at_kernel(g in arb_grid(), k in 0u32..=10_000) {
            let c = quantize_at(g);
            let t = Time { beats: k, base: g };
            prop_assert!(c.ceil(c.inner(t)) <= t);
        }

        #[test]
        fn quantize_at_monotonic(
            g in arb_grid(),
            a1 in arb_tick(), a2 in arb_tick(),
        ) {
            let c = quantize_at(g);
            prop_assume!(ceil_fits(a1, g) && ceil_fits(a2, g));
            if a1 <= a2 {
                prop_assert!(c.ceil(a1) <= c.ceil(a2));
                prop_assert!(c.floor(a1) <= c.floor(a2));
            }
        }

        #[test]
        fn quantize_at_idempotent(g in arb_grid(), n in arb_tick()) {
            let c = quantize_at(g);
            prop_assume!(ceil_fits(n, g));
            let once = c.inner(c.ceil(n));
            let twice = c.inner(c.ceil(once));
            prop_assert_eq!(once, twice);
        }
    }

    // ── Lattice-connection laws for `TIMETIME` and `GRIDGRID` ────
    //
    // The adjoint structure `meet ⊣ diag ⊣ join` holds under the
    // "refine-to" order: `a ≤ b ⟺ tc(b) divides tc(a)` (i.e. "b is at
    // least as fine as a"). The standard divisibility `PartialOrd`
    // for `Grid` orients the other way around and would give a non-
    // adjoint structure here, so we use ad-hoc `refine_le` helpers.

    fn gridgrid_refine_le(a: Grid, b: Grid) -> bool {
        a.tick_count() % b.tick_count() == 0
    }

    fn timetime_refine_le(a: Time, b: Time) -> bool {
        let ta = time_to_tick(a).0;
        let tb = time_to_tick(b).0;
        if tb == 0 { ta == 0 } else { ta % tb == 0 }
    }

    /// True when `quantize_at(g).ceil(n)` fits back through
    /// `time_to_tick` — i.e. `n.0.div_ceil(tc) ≤ u32::MAX`, the
    /// horizon of `Time.beats`. Used to skip `arb_tick()`'s upper
    /// boundary in proptests that compose `time_to_tick` on the ceil
    /// result.
    fn ceil_fits(n: Tick, g: Grid) -> bool {
        let tc = u64::from(g.tick_count());
        n.0.div_ceil(tc) <= u64::from(u32::MAX)
    }

    proptest! {
        // ── gridgrid connection ──────────────────────────────────

        #[test]
        fn gridgrid_ceil_is_meet(a in arb_grid(), b in arb_grid()) {
            let c = GRIDGRID;
            prop_assert_eq!(c.ceil((a, b)), a.meet(&b));
        }

        #[test]
        fn gridgrid_floor_is_join(a in arb_grid(), b in arb_grid()) {
            let c = GRIDGRID;
            prop_assert_eq!(c.floor((a, b)), a.join(&b));
        }

        #[test]
        fn gridgrid_inner_is_diagonal(t in arb_grid()) {
            let c = GRIDGRID;
            prop_assert_eq!(c.inner(t), (t, t));
        }

        #[test]
        fn gridgrid_adjoint(
            a in arb_grid(), b in arb_grid(), z in arb_grid(),
        ) {
            let c = GRIDGRID;
            let lhs = gridgrid_refine_le(c.ceil((a, b)), z);
            let rhs = gridgrid_refine_le(a, z) && gridgrid_refine_le(b, z);
            prop_assert_eq!(lhs, rhs);
        }

        #[test]
        fn gridgrid_closed(a in arb_grid(), b in arb_grid()) {
            let c = GRIDGRID;
            let (x, y) = c.inner(c.ceil((a, b)));
            prop_assert!(gridgrid_refine_le(a, x));
            prop_assert!(gridgrid_refine_le(b, y));
        }

        #[test]
        fn gridgrid_kernel(z in arb_grid()) {
            let c = GRIDGRID;
            prop_assert!(gridgrid_refine_le(c.ceil(c.inner(z)), z));
        }

        #[test]
        fn gridgrid_monotonic(
            a1 in arb_grid(), a2 in arb_grid(),
            b1 in arb_grid(), b2 in arb_grid(),
            z1 in arb_grid(), z2 in arb_grid(),
        ) {
            let c = GRIDGRID;
            if gridgrid_refine_le(a1, a2) && gridgrid_refine_le(b1, b2) {
                prop_assert!(gridgrid_refine_le(c.ceil((a1, b1)), c.ceil((a2, b2))));
            }
            if gridgrid_refine_le(z1, z2) {
                let (x1, y1) = c.inner(z1);
                let (x2, y2) = c.inner(z2);
                prop_assert!(gridgrid_refine_le(x1, x2));
                prop_assert!(gridgrid_refine_le(y1, y2));
            }
        }

        #[test]
        fn gridgrid_idempotent(a in arb_grid(), b in arb_grid()) {
            let c = GRIDGRID;
            let once = c.inner(c.ceil((a, b)));
            let twice = c.inner(c.ceil(once));
            prop_assert_eq!(once, twice);
        }

        // ── timetime connection ──────────────────────────────────

        #[test]
        fn timetime_ceil_is_gcd_on_ticks(
            a in arb_small_time(), b in arb_small_time(),
        ) {
            let c = TIMETIME;
            let ta = time_to_tick(a).0;
            let tb = time_to_tick(b).0;
            prop_assert_eq!(time_to_tick(c.ceil((a, b))).0, gcd_u64(ta, tb));
        }

        #[test]
        fn timetime_floor_is_lcm_on_ticks(
            a in arb_small_time(), b in arb_small_time(),
        ) {
            let c = TIMETIME;
            let ta = time_to_tick(a).0;
            let tb = time_to_tick(b).0;
            prop_assert_eq!(time_to_tick(c.floor((a, b))).0, lcm_u64(ta, tb));
        }

        #[test]
        fn time_inner_is_diagonal(t in arb_small_time()) {
            let c = TIMETIME;
            prop_assert_eq!(c.inner(t), (t, t));
        }

        #[test]
        fn time_adjoint(
            a in arb_small_time(), b in arb_small_time(), z in arb_small_time(),
        ) {
            let c = TIMETIME;
            let lhs = timetime_refine_le(c.ceil((a, b)), z);
            let rhs = timetime_refine_le(a, z) && timetime_refine_le(b, z);
            prop_assert_eq!(lhs, rhs);
        }

        #[test]
        fn time_closed(a in arb_small_time(), b in arb_small_time()) {
            let c = TIMETIME;
            let (x, y) = c.inner(c.ceil((a, b)));
            prop_assert!(timetime_refine_le(a, x));
            prop_assert!(timetime_refine_le(b, y));
        }

        #[test]
        fn time_kernel(z in arb_small_time()) {
            let c = TIMETIME;
            prop_assert!(timetime_refine_le(c.ceil(c.inner(z)), z));
        }

        #[test]
        fn time_idempotent(a in arb_small_time(), b in arb_small_time()) {
            let c = TIMETIME;
            let once = c.inner(c.ceil((a, b)));
            let twice = c.inner(c.ceil(once));
            prop_assert_eq!(once, twice);
        }

        #[test]
        fn time_monotonic(
            a1 in arb_small_time(), a2 in arb_small_time(),
            b1 in arb_small_time(), b2 in arb_small_time(),
            z1 in arb_small_time(), z2 in arb_small_time(),
        ) {
            let c = TIMETIME;
            if timetime_refine_le(a1, a2) && timetime_refine_le(b1, b2) {
                prop_assert!(
                    timetime_refine_le(c.ceil((a1, b1)), c.ceil((a2, b2)))
                );
            }
            if timetime_refine_le(z1, z2) {
                let (x1, y1) = c.inner(z1);
                let (x2, y2) = c.inner(z2);
                prop_assert!(timetime_refine_le(x1, x2));
                prop_assert!(timetime_refine_le(y1, y2));
            }
        }
    }
}
