//! Galois connections for `Tick`, `Time`, `Rational`, and `Grid`.
//!
//! Five `Conn<A, B>` values port the Haskell Cirklon connections:
//!
//! | Rust              | Haskell      | Shape                        |
//! |-------------------|--------------|------------------------------|
//! | [`ticks`]         | `ticks`      | `Conn<Tick, Time>`           |
//! | [`rat_tick`]      | `ratTick`    | `Conn<Whole, Tick>`          |
//! | [`quantize_at`]   | `quantizeAt` | `Conn<Tick, Time>` per Grid  |
//! | [`time`]          | `time`       | `Conn<(Time, Time), Time>`   |
//! | [`grid`]          | `tbase`      | `Conn<(Grid, Grid), Grid>`   |
//!
//! All use bare `fn` pointers from [`connections::conn::Conn`] — no
//! closure capture, tempo-independent. `Conn::new` isn't `const fn`
//! upstream, so each accessor returns a freshly-built `Conn` (still
//! cheap: three `fn` pointers).
//!
//! **Orientation of `time` and `grid`.** These are lattice
//! connections: the pair side carries the divisibility product order,
//! not magnitude. Following the Haskell convention,
//! `ceil = meet (GCD)` and `floor = join (LCM)`. The generic
//! adjoint-law tests from `connections/src/conn.rs` that use a single
//! preorder (e.g. via `Ple`) therefore need a *divisibility* `≤` on
//! the input/output side — applying Time's magnitude `Ple` would give
//! a different (and non-adjoint) structure. The tests below build
//! ad-hoc `div_le` helpers for that reason.

use connections::conn::Conn;
use num_rational::Rational64;

use crate::time::grid::Grid;
use connections::lattice::{Join, Meet};
use crate::time::tick::{Tick, Time, from_ticks, from_ticks_floor, time_to_tick, PPQN};

/// A rational whole-note duration. `Whole::new(1, 4)` = quarter note.
pub type Whole = Rational64;

/// Ticks per whole note at 960 PPQN. `4 * PPQN`.
const TPW: i64 = (4 * PPQN) as i64;

// ── ticks: Conn<Tick, Time> ──────────────────────────────────────

fn ticks_ceil(n: Tick) -> Time {
    from_ticks(n)
}

fn ticks_inner(t: Time) -> Tick {
    time_to_tick(t)
}

fn ticks_floor(n: Tick) -> Time {
    from_ticks_floor(n)
}

/// Master `Tick ↔ Time` connection. Ceiling rounds up to the
/// `Grid::T512P` grid (= 1 tick at 960 PPQN, so every tick is
/// already aligned) then canonicalises; floor rounds down; embed is
/// exact.
pub fn ticks() -> Conn<Tick, Time> {
    Conn::new(ticks_ceil, ticks_inner, ticks_floor)
}

// ── rat_tick: Conn<Whole, Tick> ──────────────────────────────────

fn tpw_rational() -> Rational64 {
    Rational64::new(TPW, 1)
}

// Clamp an `i64` into the `u32` range. Saturates at both ends so a
// huge rational produces `u32::MAX` ticks (not a wrapped value) and
// a negative one produces 0.
fn i64_to_tick(n: i64) -> Tick {
    Tick(n.clamp(0, i64::from(u32::MAX)) as u32)
}

fn rt_ceil(r: Whole) -> Tick {
    let ceil = (r * tpw_rational()).ceil().to_integer();
    i64_to_tick(ceil)
}

fn rt_inner(n: Tick) -> Whole {
    Rational64::new(i64::from(n.0), TPW)
}

fn rt_floor(r: Whole) -> Tick {
    let floor = (r * tpw_rational()).floor().to_integer();
    i64_to_tick(floor)
}

/// Galois connection between rational whole-note durations and ticks.
/// Floor rounds down, ceiling rounds up, embed is exact:
/// `rt_inner(Tick(n)) = n / 3840` at 960 PPQN.
pub fn rat_tick() -> Conn<Whole, Tick> {
    Conn::new(rt_ceil, rt_inner, rt_floor)
}

// ── quantize_at: Conn<Tick, Time> per Grid ───────────────────────

fn qa_inner(t: Time) -> Tick {
    time_to_tick(t)
}

macro_rules! qa_variant {
    ($variant:ident, $ceil:ident, $floor:ident) => {
        fn $ceil(n: Tick) -> Time {
            Time {
                beats: n.0.div_ceil(Grid::$variant.tick_count()),
                base: Grid::$variant,
            }
        }
        fn $floor(n: Tick) -> Time {
            Time {
                beats: n.0 / Grid::$variant.tick_count(),
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
/// [`ticks`]). `fn` pointers can't close over `g`, so dispatch is a
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

// ── time: Conn<(Time, Time), Time> ───────────────────────────────

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

fn time_pair_ceil(ab: (Time, Time)) -> Time {
    let (a, b) = ab;
    let g = gcd_u64(u64::from(time_to_tick(a).0), u64::from(time_to_tick(b).0));
    // GCD of u32 values fits in u32.
    from_ticks(Tick(g as u32))
}

fn time_pair_inner(t: Time) -> (Time, Time) {
    (t, t)
}

fn time_pair_floor(ab: (Time, Time)) -> Time {
    let (a, b) = ab;
    let l = lcm_u64(u64::from(time_to_tick(a).0), u64::from(time_to_tick(b).0));
    // LCM can overflow u32 in general; property tests bound inputs via
    // `arb_small_time` so `l <= u32::MAX`.
    from_ticks(Tick(
        u32::try_from(l).expect("LCM of tick counts overflows u32"),
    ))
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
/// `u32::MAX`. For musically-bounded `Time` values this is
/// unreachable; tests use `arb_small_time` (tick counts ≤ 192_000,
/// LCM well inside `u32`) to stay safely bounded.
pub fn time() -> Conn<(Time, Time), Time> {
    Conn::new(time_pair_ceil, time_pair_inner, time_pair_floor)
}

// ── grid: Conn<(Grid, Grid), Grid> ───────────────────────────────

fn grid_pair_ceil(ab: (Grid, Grid)) -> Grid {
    let (a, b) = ab;
    a.meet(&b)
}

fn grid_pair_inner(t: Grid) -> (Grid, Grid) {
    (t, t)
}

fn grid_pair_floor(ab: (Grid, Grid)) -> Grid {
    let (a, b) = ab;
    a.join(&b)
}

/// Divisibility-lattice connection on `Grid`. `ceil = meet (GCD of
/// tick counts)`, `floor = join (LCM)`, `inner = diagonal`.
pub fn grid() -> Conn<(Grid, Grid), Grid> {
    Conn::new(grid_pair_ceil, grid_pair_inner, grid_pair_floor)
}

// ── SampleTickConn: Sample ↔ Tick (runtime-parameterised) ────────
//
// `connections::Conn<A, B>` uses bare `fn` pointers that cannot close
// over runtime state, so the natural `Conn<Sample, Tick>` parameterised
// on `(sr, bpm, ppqn)` is not expressible today. The pragmatic
// workaround is a struct mirroring `Conn`'s `(ceil, inner, floor)`
// shape with the same adjoint-law guarantees, all integer-valued.

/// Sample ↔ Tick bridge parameterised by sample rate, tempo, and PPQN.
///
/// Mirrors `connections::Conn<Sample, Tick>`'s `(ceil, inner, floor)`
/// triple. The laws — round-trip on aligned inputs, monotonicity — are
/// verified by proptest. Not a real `Conn` because its conversion
/// depends on runtime `(sr, bpm, ppqn)` — would require a closure-
/// capturing variant upstream.
///
/// All arithmetic is integer (`Tempo` for tempo, `u128` intermediate).
/// No floating-point.
#[derive(Copy, Clone, Debug)]
pub struct SampleTickConn {
    sr: u32,
    bpm: crate::fxp::Tempo,
    ppqn: u32,
}

impl SampleTickConn {
    /// # Panics
    ///
    /// Panics if `sr == 0`, `ppqn == 0`, or `bpm.0 == 0`. These are
    /// programming errors — every call site either ships fixed
    /// constants or validates at a CLI/config boundary.
    pub fn new(sr: u32, bpm: crate::fxp::Tempo, ppqn: u32) -> Self {
        assert!(sr > 0, "sample rate must be positive");
        assert!(ppqn > 0, "ppqn must be positive");
        assert!(bpm.0 > 0, "bpm must be positive, got {:?}", bpm);
        Self { sr, bpm, ppqn }
    }

    pub fn sr(&self) -> u32 {
        self.sr
    }
    pub fn bpm(&self) -> crate::fxp::Tempo {
        self.bpm
    }
    pub fn ppqn(&self) -> u32 {
        self.ppqn
    }

    /// Tick → Sample. Exact when `tick × sr × 60 × 10⁶` is divisible
    /// by `bpm_µ × ppqn` (e.g. 48 kHz / 120 BPM / 960 PPQN is exact);
    /// otherwise rounded to the nearest `u64` (half-away-from-zero —
    /// both quantities are non-negative).
    pub fn inner(&self, tick: Tick) -> u64 {
        // sample = tick · sr · 60 · 10⁶ / (bpm_µ · ppqn)
        let num = u128::from(tick.0) * u128::from(self.sr) * 60 * 1_000_000;
        let denom = u128::from(self.bpm.0) * u128::from(self.ppqn);
        // Round to nearest: (num + denom/2) / denom. Half-up because
        // both num and denom are non-negative.
        ((num + denom / 2) / denom) as u64
    }

    /// Sample → Tick, rounding down (latest tick at-or-before `sample`).
    pub fn floor(&self, sample: u64) -> Tick {
        // tick = sample · bpm_µ · ppqn / (sr · 60 · 10⁶)   (floor)
        let num = u128::from(sample) * u128::from(self.bpm.0) * u128::from(self.ppqn);
        let denom = u128::from(self.sr) * 60 * 1_000_000;
        Self::to_tick(num / denom)
    }

    /// Sample → Tick, rounding up (next tick at-or-after `sample`).
    pub fn ceil(&self, sample: u64) -> Tick {
        let num = u128::from(sample) * u128::from(self.bpm.0) * u128::from(self.ppqn);
        let denom = u128::from(self.sr) * 60 * 1_000_000;
        Self::to_tick(num.div_ceil(denom))
    }

    fn to_tick(x: u128) -> Tick {
        Tick(x.min(u128::from(u32::MAX)) as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arb::{arb_grid, arb_rational_nonneg, arb_small_time, arb_tick, arb_time};
    use connections::lattice::Ple;
    use proptest::prelude::*;

    // ── Spot checks ──────────────────────────────────────────────

    #[test]
    fn ticks_inner_is_exact() {
        let c = ticks();
        let t = Time {
            beats: 3,
            base: Grid::T16,
        };
        // T16 = 240 ticks at 960 PPQN; 3 × 240 = 720.
        assert_eq!(c.inner(t), Tick(720));
    }

    #[test]
    fn ticks_ceil_aligned() {
        let c = ticks();
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
    fn ticks_floor_unaligned() {
        let c = ticks();
        // 50 ticks: T512P = 1, so exact (no rounding); coarsest divisor
        // of 50 in the lattice.  50 = 2 · 5² → factors out 5 (q-flag);
        // 50 / Grid::T256Q.tick_count() should hit. T256Q = 6, doesn't
        // divide 50. Walk the list: T512P=1 divides everything → 50 T512P.
        let t = c.floor(Tick(50));
        assert_eq!(time_to_tick(t).0, 50);
    }

    #[test]
    fn ticks_ceil_unaligned() {
        let c = ticks();
        // At 960 PPQN every Tick is on Grid::T512P (=1). So ceil and
        // floor both yield the canonical form for `n` itself.
        let t = c.ceil(Tick(50));
        assert_eq!(time_to_tick(t).0, 50);
    }

    #[test]
    fn rat_tick_quarter_is_960() {
        let c = rat_tick();
        assert_eq!(c.floor(Rational64::new(1, 4)), Tick(960));
        assert_eq!(c.ceil(Rational64::new(1, 4)), Tick(960));
    }

    #[test]
    fn rat_tick_three_sixteenths_is_720() {
        let c = rat_tick();
        // 3/16 × 3840 = 720.
        assert_eq!(c.floor(Rational64::new(3, 16)), Tick(720));
    }

    #[test]
    fn rat_tick_one_seventh_ceils_correctly() {
        // 3840 / 7 = 548.57…, ceil = 549, floor = 548.
        let c = rat_tick();
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
    fn time_ceil_gcd_of_t4_t8() {
        let c = time();
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
    fn time_floor_lcm_of_t16_and_t16t() {
        let c = time();
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
    fn grid_ceil_meet_of_t4_t8() {
        let c = grid();
        // gcd of tick counts: gcd(960, 480) = 480 = T8.
        assert_eq!(c.ceil((Grid::T4, Grid::T8)), Grid::T8);
    }

    #[test]
    fn grid_floor_join_of_t4_t8t() {
        let c = grid();
        // T4 = 960, T8T = 320; lcm(960, 320) = 960 = T4.
        assert_eq!(c.floor((Grid::T4, Grid::T8T)), Grid::T4);
    }

    // ── Generic connections-tests laws for magnitude connections ──

    proptest! {
        // ── ticks ────────────────────────────────────────────────

        #[test]
        fn ticks_adjoint(a in arb_tick(), b in arb_time()) {
            let c = ticks();
            let lhs = c.ceil(a).ple(&b);
            let rhs = a.ple(&c.inner(b));
            prop_assert_eq!(lhs, rhs);
        }

        #[test]
        fn ticks_closed(a in arb_tick()) {
            let c = ticks();
            prop_assert!(a.ple(&c.inner(c.ceil(a))));
        }

        #[test]
        fn ticks_kernel(b in arb_time()) {
            let c = ticks();
            prop_assert!(c.ceil(c.inner(b)).ple(&b));
        }

        #[test]
        fn ticks_monotonic(
            a1 in arb_tick(), a2 in arb_tick(),
            b1 in arb_time(), b2 in arb_time(),
        ) {
            let c = ticks();
            if a1.ple(&a2) {
                prop_assert!(c.ceil(a1).ple(&c.ceil(a2)));
            }
            if b1.ple(&b2) {
                prop_assert!(c.inner(b1).ple(&c.inner(b2)));
            }
        }

        #[test]
        fn ticks_idempotent(a in arb_tick()) {
            let c = ticks();
            let once = c.inner(c.ceil(a));
            let twice = c.inner(c.ceil(once));
            prop_assert_eq!(once, twice);
        }

        /// At 960 PPQN, every tick is on `Grid::T512P` (= 1) so this
        /// is the identity on every tick.
        #[test]
        fn ticks_round_trip_on_aligned(q in 0u32..=1_000_000) {
            let c = ticks();
            let n = Tick(q * Grid::T512P.tick_count());
            prop_assert_eq!(c.inner(c.floor(n)), n);
        }

        // ── rat_tick ─────────────────────────────────────────────

        #[test]
        fn rat_tick_adjoint(a in arb_rational_nonneg(), b in arb_tick()) {
            let c = rat_tick();
            let lhs = c.ceil(a).ple(&b);
            let rhs = a <= c.inner(b);
            prop_assert_eq!(lhs, rhs);
        }

        #[test]
        fn rat_tick_closed(a in arb_rational_nonneg()) {
            let c = rat_tick();
            prop_assert!(a <= c.inner(c.ceil(a)));
        }

        #[test]
        fn rat_tick_kernel(b in arb_tick()) {
            let c = rat_tick();
            prop_assert!(c.ceil(c.inner(b)).ple(&b));
        }

        #[test]
        fn rat_tick_monotonic(
            a1 in arb_rational_nonneg(), a2 in arb_rational_nonneg(),
            b1 in arb_tick(), b2 in arb_tick(),
        ) {
            let c = rat_tick();
            if a1 <= a2 {
                prop_assert!(c.ceil(a1).ple(&c.ceil(a2)));
            }
            if b1.ple(&b2) {
                prop_assert!(c.inner(b1) <= c.inner(b2));
            }
        }

        #[test]
        fn rat_tick_idempotent(a in arb_rational_nonneg()) {
            let c = rat_tick();
            let once = c.inner(c.ceil(a));
            let twice = c.inner(c.ceil(once));
            prop_assert_eq!(once, twice);
        }

        #[test]
        fn rat_tick_floor_monotone(
            a in arb_rational_nonneg(), b in arb_rational_nonneg(),
        ) {
            let c = rat_tick();
            if a <= b {
                prop_assert!(c.floor(a).ple(&c.floor(b)));
            }
        }

        // ── quantize_at ──────────────────────────────────────────
        //
        // `c.ceil(n)` returns `Time { beats: n.0.div_ceil(tc), base: g }`
        // where `tc = g.tick_count()`. For `n` near `u32::MAX`, the
        // *Time*'s `beats × tc` can exceed `u32::MAX` (`time_to_tick`
        // panics on `checked_mul` overflow). The `arb_tick()`
        // distribution includes `u32::MAX` per CLAUDE.md's full-domain
        // rule, so any property that subsequently calls
        // `time_to_tick(c.ceil(n))` (directly or via `.ple`) must
        // `prop_assume!` away the overflow corner. Spot checks at the
        // saturation boundary live in `time::swing::tests`.

        #[test]
        fn quantize_at_brackets_input(
            g in arb_grid(), n in arb_tick(),
        ) {
            let c = quantize_at(g);
            prop_assume!(ceil_fits(n, g));
            let lo = time_to_tick(c.floor(n));
            let hi = time_to_tick(c.ceil(n));
            prop_assert!(lo.ple(&n));
            prop_assert!(n.ple(&hi));
        }

        #[test]
        fn quantize_at_aligned_inner_round_trip(
            g in arb_grid(), q in 0u32..=10_000,
        ) {
            let c = quantize_at(g);
            let n = Tick(q * g.tick_count());
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
            let lhs = c.ceil(n).ple(&t);
            let rhs = n.ple(&c.inner(t));
            prop_assert_eq!(lhs, rhs);
        }

        #[test]
        fn quantize_at_closed(g in arb_grid(), n in arb_tick()) {
            let c = quantize_at(g);
            prop_assume!(ceil_fits(n, g));
            prop_assert!(n.ple(&c.inner(c.ceil(n))));
        }

        #[test]
        fn quantize_at_kernel(g in arb_grid(), k in 0u32..=10_000) {
            let c = quantize_at(g);
            let t = Time { beats: k, base: g };
            prop_assert!(c.ceil(c.inner(t)).ple(&t));
        }

        #[test]
        fn quantize_at_monotonic(
            g in arb_grid(),
            a1 in arb_tick(), a2 in arb_tick(),
        ) {
            let c = quantize_at(g);
            prop_assume!(ceil_fits(a1, g) && ceil_fits(a2, g));
            if a1.ple(&a2) {
                prop_assert!(c.ceil(a1).ple(&c.ceil(a2)));
                prop_assert!(c.floor(a1).ple(&c.floor(a2)));
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

    // ── Lattice-connection laws for `time` and `grid` ────────────
    //
    // The adjoint structure `meet ⊣ diag ⊣ join` holds under the
    // "refine-to" order: `a ≤ b ⟺ tc(b) divides tc(a)` (i.e. "b is at
    // least as fine as a"). Standard divisibility order (our `Ple` impl)
    // orients the other way around and would give a non-adjoint
    // structure here, so we use ad-hoc `refine_le` helpers.

    fn grid_refine_le(a: Grid, b: Grid) -> bool {
        a.tick_count() % b.tick_count() == 0
    }

    fn time_refine_le(a: Time, b: Time) -> bool {
        let ta = time_to_tick(a).0;
        let tb = time_to_tick(b).0;
        if tb == 0 { ta == 0 } else { ta % tb == 0 }
    }

    /// True when `quantize_at(g).ceil(n)` fits back through
    /// `time_to_tick` without `checked_mul` overflow. Used to skip
    /// `arb_tick()`'s `u32::MAX` boundary in proptests that compose
    /// `time_to_tick` on the ceil result.
    fn ceil_fits(n: Tick, g: Grid) -> bool {
        let tc = g.tick_count();
        n.0.div_ceil(tc).checked_mul(tc).is_some()
    }

    proptest! {
        // ── grid connection ──────────────────────────────────────

        #[test]
        fn grid_ceil_is_meet(a in arb_grid(), b in arb_grid()) {
            let c = grid();
            prop_assert_eq!(c.ceil((a, b)), a.meet(&b));
        }

        #[test]
        fn grid_floor_is_join(a in arb_grid(), b in arb_grid()) {
            let c = grid();
            prop_assert_eq!(c.floor((a, b)), a.join(&b));
        }

        #[test]
        fn grid_inner_is_diagonal(t in arb_grid()) {
            let c = grid();
            prop_assert_eq!(c.inner(t), (t, t));
        }

        #[test]
        fn grid_adjoint(
            a in arb_grid(), b in arb_grid(), z in arb_grid(),
        ) {
            let c = grid();
            let lhs = grid_refine_le(c.ceil((a, b)), z);
            let rhs = grid_refine_le(a, z) && grid_refine_le(b, z);
            prop_assert_eq!(lhs, rhs);
        }

        #[test]
        fn grid_closed(a in arb_grid(), b in arb_grid()) {
            let c = grid();
            let (x, y) = c.inner(c.ceil((a, b)));
            prop_assert!(grid_refine_le(a, x));
            prop_assert!(grid_refine_le(b, y));
        }

        #[test]
        fn grid_kernel(z in arb_grid()) {
            let c = grid();
            prop_assert!(grid_refine_le(c.ceil(c.inner(z)), z));
        }

        #[test]
        fn grid_monotonic(
            a1 in arb_grid(), a2 in arb_grid(),
            b1 in arb_grid(), b2 in arb_grid(),
            z1 in arb_grid(), z2 in arb_grid(),
        ) {
            let c = grid();
            if grid_refine_le(a1, a2) && grid_refine_le(b1, b2) {
                prop_assert!(grid_refine_le(c.ceil((a1, b1)), c.ceil((a2, b2))));
            }
            if grid_refine_le(z1, z2) {
                let (x1, y1) = c.inner(z1);
                let (x2, y2) = c.inner(z2);
                prop_assert!(grid_refine_le(x1, x2));
                prop_assert!(grid_refine_le(y1, y2));
            }
        }

        #[test]
        fn grid_idempotent(a in arb_grid(), b in arb_grid()) {
            let c = grid();
            let once = c.inner(c.ceil((a, b)));
            let twice = c.inner(c.ceil(once));
            prop_assert_eq!(once, twice);
        }

        // ── time connection ──────────────────────────────────────

        #[test]
        fn time_ceil_is_gcd_on_ticks(
            a in arb_small_time(), b in arb_small_time(),
        ) {
            let c = time();
            let ta = u64::from(time_to_tick(a).0);
            let tb = u64::from(time_to_tick(b).0);
            prop_assert_eq!(time_to_tick(c.ceil((a, b))).0 as u64, gcd_u64(ta, tb));
        }

        #[test]
        fn time_floor_is_lcm_on_ticks(
            a in arb_small_time(), b in arb_small_time(),
        ) {
            let c = time();
            let ta = u64::from(time_to_tick(a).0);
            let tb = u64::from(time_to_tick(b).0);
            prop_assert_eq!(time_to_tick(c.floor((a, b))).0 as u64, lcm_u64(ta, tb));
        }

        #[test]
        fn time_inner_is_diagonal(t in arb_small_time()) {
            let c = time();
            prop_assert_eq!(c.inner(t), (t, t));
        }

        #[test]
        fn time_adjoint(
            a in arb_small_time(), b in arb_small_time(), z in arb_small_time(),
        ) {
            let c = time();
            let lhs = time_refine_le(c.ceil((a, b)), z);
            let rhs = time_refine_le(a, z) && time_refine_le(b, z);
            prop_assert_eq!(lhs, rhs);
        }

        #[test]
        fn time_closed(a in arb_small_time(), b in arb_small_time()) {
            let c = time();
            let (x, y) = c.inner(c.ceil((a, b)));
            prop_assert!(time_refine_le(a, x));
            prop_assert!(time_refine_le(b, y));
        }

        #[test]
        fn time_kernel(z in arb_small_time()) {
            let c = time();
            prop_assert!(time_refine_le(c.ceil(c.inner(z)), z));
        }

        #[test]
        fn time_idempotent(a in arb_small_time(), b in arb_small_time()) {
            let c = time();
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
            let c = time();
            if time_refine_le(a1, a2) && time_refine_le(b1, b2) {
                prop_assert!(
                    time_refine_le(c.ceil((a1, b1)), c.ceil((a2, b2)))
                );
            }
            if time_refine_le(z1, z2) {
                let (x1, y1) = c.inner(z1);
                let (x2, y2) = c.inner(z2);
                prop_assert!(time_refine_le(x1, x2));
                prop_assert!(time_refine_le(y1, y2));
            }
        }
    }

    // ── SampleTickConn ───────────────────────────────────────────

    fn mbpm(b: u32) -> crate::fxp::Tempo {
        crate::fxp::Tempo::from_bpm_integer(b)
    }

    #[test]
    fn sample_tick_inner_120bpm_48k_one_beat_at_960ppqn() {
        // 120 BPM, 960 PPQN, 48 kHz: one quarter note (tick 960) is
        // 0.5 s = 24 000 samples.
        let stc = SampleTickConn::new(48_000, mbpm(120), 960);
        assert_eq!(stc.inner(Tick(960)), 24_000);
    }

    #[test]
    fn sample_tick_floor_and_ceil_bracket_inner() {
        let stc = SampleTickConn::new(48_000, mbpm(120), 960);
        // 24 000 samples is exactly tick 960. Floor and ceil both
        // 960; one sample later (24 001) → floor still 960, ceil 961.
        assert_eq!(stc.floor(24_000), Tick(960));
        assert_eq!(stc.ceil(24_000), Tick(960));
        assert_eq!(stc.floor(24_001), Tick(960));
        assert_eq!(stc.ceil(24_001), Tick(961));
    }

    #[test]
    fn sample_tick_zero_is_zero() {
        let stc = SampleTickConn::new(48_000, mbpm(120), 960);
        assert_eq!(stc.inner(Tick(0)), 0);
        assert_eq!(stc.floor(0), Tick(0));
        assert_eq!(stc.ceil(0), Tick(0));
    }

    /// Sample-rate / BPM / PPQN combinations that keep integer
    /// samples-per-tick exact (`sr · 60 · 10⁶` divisible by
    /// `bpm_µ · ppqn`), needed for the round-trip property.
    fn arb_integer_stc() -> impl Strategy<Value = SampleTickConn> {
        prop_oneof![
            // 960 PPQN: at 48 kHz, BPMs that divide 60·48000/960 = 3000
            // are integer-exact. 60, 120, 240, 300 all qualify.
            Just(SampleTickConn::new(48_000, mbpm(60), 960)),
            Just(SampleTickConn::new(48_000, mbpm(120), 960)),
            Just(SampleTickConn::new(48_000, mbpm(240), 960)),
            Just(SampleTickConn::new(48_000, mbpm(300), 960)),
            // 96 kHz: 60·96000/960 = 6000.
            Just(SampleTickConn::new(96_000, mbpm(120), 960)),
            // 192 kHz: 60·192000/960 = 12000.
            Just(SampleTickConn::new(192_000, mbpm(120), 960)),
            // Lower-PPQN sanity (24 PPQ MIDI clock cadence).
            Just(SampleTickConn::new(48_000, mbpm(120), 24)),
        ]
    }

    proptest! {
        #[test]
        fn sample_tick_round_trip(
            stc in arb_integer_stc(),
            t in 0u32..=1_000_000,
        ) {
            let tick = Tick(t);
            let sample = stc.inner(tick);
            prop_assert_eq!(stc.floor(sample), tick);
        }

        #[test]
        fn sample_tick_monotonic(
            stc in arb_integer_stc(),
            s1 in 0u64..=10_000_000,
            s2 in 0u64..=10_000_000,
        ) {
            let (lo, hi) = if s1 <= s2 { (s1, s2) } else { (s2, s1) };
            prop_assert!(stc.floor(lo).0 <= stc.floor(hi).0);
        }

        #[test]
        fn sample_tick_ceil_ge_floor(
            stc in arb_integer_stc(),
            s in 0u64..=10_000_000,
        ) {
            prop_assert!(stc.floor(s).0 <= stc.ceil(s).0);
        }
    }

    // ── Pico ↔ Sample agreement with SampleTickConn ──────────────

    use connections::conn::fixed::Pico;

    #[test]
    fn sample_tick_and_pico_to_samples_agree_at_120bpm_48k() {
        // 120 BPM / ppq=960 / 48 kHz: each quarter note = 0.5 s =
        // 24 000 samples = 5×10¹¹ pico. At tick 960 (one beat):
        let stc = SampleTickConn::new(48_000, mbpm(120), 960);

        let via_tick: u64 = stc.inner(Tick(960));
        let pico_at_one_beat = Pico(500_000_000_000);
        let via_pico: i64 = crate::fxp::pico_to_samples(pico_at_one_beat, 48_000)
            .expect("48 kHz is supported");
        assert_eq!(via_tick, 24_000);
        assert_eq!(via_pico, 24_000);
        assert_eq!(via_tick as i64, via_pico);

        // And at tick 1920 (two beats = 1 s = 48 000 samples = 10¹² pico):
        assert_eq!(stc.inner(Tick(1920)), 48_000);
        assert_eq!(
            crate::fxp::pico_to_samples(Pico(1_000_000_000_000), 48_000),
            Some(48_000)
        );
    }

    #[test]
    fn pico_to_samples_rejects_unsupported_rate() {
        assert_eq!(crate::fxp::pico_to_samples(Pico(0), 22_050), None);
        assert_eq!(crate::fxp::pico_to_samples(Pico(0), 0), None);
        assert_eq!(crate::fxp::pico_to_samples(Pico(1_000_000_000_000), 48_000), Some(48_000));
    }
}
