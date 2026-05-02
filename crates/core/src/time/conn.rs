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
//! All use bare `fn` pointers through kind-tagged
//! [`connections::conn::ConnL`] / [`connections::conn::ConnR`] views —
//! no closure capture, tempo-independent. Static connections are
//! zero-sized marker values matching upstream's triple API.
//! `quantize_at` stays a function because its inner / ceil / floor
//! pointers vary per `Grid` value.
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

use connections::conn::{Conn, ConnL, ConnR, ViewL, ViewR};
use connections::fixed::u64::{I064U064, U128U064};
use num_rational::Rational64;

use crate::conn::tempo::Tempo;
use crate::time::grid::Grid;
use crate::time::tick::{PPQN, Tick, Time, from_ticks, from_ticks_floor, time_to_tick};
use connections::lattice::{Join, Meet};

/// A rational whole-note duration. `Whole::new(1, 4)` = quarter note.
pub type Whole = Rational64;

/// Ticks per whole note at 960 PPQN. `4 * PPQN`.
const TPW: i64 = (4 * PPQN) as i64;

macro_rules! def_conn_marker {
    ($name:ident, $A:ty, $B:ty, $ceil:path, $inner:path, $floor:path) => {
        #[allow(non_camel_case_types)]
        #[derive(Copy, Clone, Debug, Default)]
        pub struct $name;

        impl $name {
            const L: ConnL<$A, $B> = Conn::new_l($ceil, $inner);
            const R: ConnR<$A, $B> = Conn::new_r($inner, $floor);

            pub fn ceil(self, x: $A) -> $B {
                Self::L.ceil(x)
            }

            pub fn inner(self, x: $B) -> $A {
                Self::L.inner(x)
            }

            pub fn floor(self, x: $A) -> $B {
                Self::R.floor(x)
            }
        }

        impl ViewL<$A, $B> for $name {
            const L: ConnL<$A, $B> = Self::L;
        }

        impl ViewR<$A, $B> for $name {
            const R: ConnR<$A, $B> = Self::R;
        }
    };
}

/// Runtime-selected triple for parametric connection families.
#[derive(Copy, Clone)]
pub struct RuntimeConn<A, B> {
    l: ConnL<A, B>,
    r: ConnR<A, B>,
}

impl<A, B> RuntimeConn<A, B> {
    const fn new(ceil: fn(A) -> B, inner: fn(B) -> A, floor: fn(A) -> B) -> Self {
        Self {
            l: Conn::new_l(ceil, inner),
            r: Conn::new_r(inner, floor),
        }
    }
}

impl<A: Copy, B: Copy> RuntimeConn<A, B> {
    pub fn ceil(self, x: A) -> B {
        self.l.ceil(x)
    }

    pub fn inner(self, x: B) -> A {
        self.l.inner(x)
    }

    pub fn floor(self, x: A) -> B {
        self.r.floor(x)
    }
}

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

// Master `Tick ↔ Time` connection. Ceiling rounds up to the
// `Grid::T512P` grid (= 1 tick at 960 PPQN, so every tick is
// already aligned) then canonicalises; floor rounds down; embed is
// exact.
def_conn_marker!(
    TICKTIME,
    Tick,
    Time,
    ticktime_ceil,
    ticktime_inner,
    ticktime_floor
);

// ── wholtick: Conn<Whole, Tick> ──────────────────────────────────

fn tpw_rational() -> Rational64 {
    Rational64::new(TPW, 1)
}

// Clamp an `i64` into the non-negative `u64` range. Saturates at 0
// so a negative rational produces `Tick(0)` rather than wrapping.
// `i64::MAX` maps to `Tick(i64::MAX as u64)`, well inside Tick's u64
// horizon.
fn i64_to_tick(n: i64) -> Tick {
    Tick(I064U064.ceil(n))
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

// Galois connection between rational whole-note durations and ticks.
// Floor rounds down, ceiling rounds up, embed is exact:
// `wholtick_inner(Tick(n)) = n / 3840` at 960 PPQN.
def_conn_marker!(
    WHOLTICK,
    Whole,
    Tick,
    wholtick_ceil,
    wholtick_inner,
    wholtick_floor
);

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
                beats: u32::try_from(beats)
                    .expect("quantize_at Conn requires n.0.div_ceil(tc) ≤ u32::MAX"),
                base: Grid::$variant,
            }
        }
        fn $floor(n: Tick) -> Time {
            let tc = u64::from(Grid::$variant.tick_count());
            let beats = n.0 / tc;
            Time {
                beats: u32::try_from(beats).expect("quantize_at Conn requires n.0 / tc ≤ u32::MAX"),
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
/// [`TICKTIME`]). `fn` pointers can't close over `g`, so dispatch is a
/// per-const `match`.
pub fn quantize_at(g: Grid) -> RuntimeConn<Tick, Time> {
    if g == Grid::T1 {
        RuntimeConn::new(qa_t1_ceil, qa_inner, qa_t1_floor)
    } else if g == Grid::T2 {
        RuntimeConn::new(qa_t2_ceil, qa_inner, qa_t2_floor)
    } else if g == Grid::T4 {
        RuntimeConn::new(qa_t4_ceil, qa_inner, qa_t4_floor)
    } else if g == Grid::T8 {
        RuntimeConn::new(qa_t8_ceil, qa_inner, qa_t8_floor)
    } else if g == Grid::T16 {
        RuntimeConn::new(qa_t16_ceil, qa_inner, qa_t16_floor)
    } else if g == Grid::T32 {
        RuntimeConn::new(qa_t32_ceil, qa_inner, qa_t32_floor)
    } else if g == Grid::T64 {
        RuntimeConn::new(qa_t64_ceil, qa_inner, qa_t64_floor)
    } else if g == Grid::T128 {
        RuntimeConn::new(qa_t128_ceil, qa_inner, qa_t128_floor)
    } else if g == Grid::T256 {
        RuntimeConn::new(qa_t256_ceil, qa_inner, qa_t256_floor)
    } else if g == Grid::T2T {
        RuntimeConn::new(qa_t2t_ceil, qa_inner, qa_t2t_floor)
    } else if g == Grid::T4T {
        RuntimeConn::new(qa_t4t_ceil, qa_inner, qa_t4t_floor)
    } else if g == Grid::T8T {
        RuntimeConn::new(qa_t8t_ceil, qa_inner, qa_t8t_floor)
    } else if g == Grid::T16T {
        RuntimeConn::new(qa_t16t_ceil, qa_inner, qa_t16t_floor)
    } else if g == Grid::T32T {
        RuntimeConn::new(qa_t32t_ceil, qa_inner, qa_t32t_floor)
    } else if g == Grid::T64T {
        RuntimeConn::new(qa_t64t_ceil, qa_inner, qa_t64t_floor)
    } else if g == Grid::T128T {
        RuntimeConn::new(qa_t128t_ceil, qa_inner, qa_t128t_floor)
    } else if g == Grid::T256T {
        RuntimeConn::new(qa_t256t_ceil, qa_inner, qa_t256t_floor)
    } else if g == Grid::T512T {
        RuntimeConn::new(qa_t512t_ceil, qa_inner, qa_t512t_floor)
    } else if g == Grid::T2Q {
        RuntimeConn::new(qa_t2q_ceil, qa_inner, qa_t2q_floor)
    } else if g == Grid::T4Q {
        RuntimeConn::new(qa_t4q_ceil, qa_inner, qa_t4q_floor)
    } else if g == Grid::T8Q {
        RuntimeConn::new(qa_t8q_ceil, qa_inner, qa_t8q_floor)
    } else if g == Grid::T16Q {
        RuntimeConn::new(qa_t16q_ceil, qa_inner, qa_t16q_floor)
    } else if g == Grid::T32Q {
        RuntimeConn::new(qa_t32q_ceil, qa_inner, qa_t32q_floor)
    } else if g == Grid::T64Q {
        RuntimeConn::new(qa_t64q_ceil, qa_inner, qa_t64q_floor)
    } else if g == Grid::T128Q {
        RuntimeConn::new(qa_t128q_ceil, qa_inner, qa_t128q_floor)
    } else if g == Grid::T256Q {
        RuntimeConn::new(qa_t256q_ceil, qa_inner, qa_t256q_floor)
    } else if g == Grid::T512Q {
        RuntimeConn::new(qa_t512q_ceil, qa_inner, qa_t512q_floor)
    } else if g == Grid::T2P {
        RuntimeConn::new(qa_t2p_ceil, qa_inner, qa_t2p_floor)
    } else if g == Grid::T4P {
        RuntimeConn::new(qa_t4p_ceil, qa_inner, qa_t4p_floor)
    } else if g == Grid::T8P {
        RuntimeConn::new(qa_t8p_ceil, qa_inner, qa_t8p_floor)
    } else if g == Grid::T16P {
        RuntimeConn::new(qa_t16p_ceil, qa_inner, qa_t16p_floor)
    } else if g == Grid::T32P {
        RuntimeConn::new(qa_t32p_ceil, qa_inner, qa_t32p_floor)
    } else if g == Grid::T64P {
        RuntimeConn::new(qa_t64p_ceil, qa_inner, qa_t64p_floor)
    } else if g == Grid::T128P {
        RuntimeConn::new(qa_t128p_ceil, qa_inner, qa_t128p_floor)
    } else if g == Grid::T256P {
        RuntimeConn::new(qa_t256p_ceil, qa_inner, qa_t256p_floor)
    } else if g == Grid::T512P {
        RuntimeConn::new(qa_t512p_ceil, qa_inner, qa_t512p_floor)
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

// Divisibility-lattice connection on `Time`.
//
// `ceil = meet (GCD)`, `floor = join (LCM)`, `inner = diagonal`.
// Following Haskell convention — the relevant order here is
// divisibility of tick counts, not magnitude.
//
// `floor` panics if the LCM of the two input tick counts exceeds
// the `from_ticks` horizon (`u32::MAX × Grid::T1.tick_count()`).
// For musically-bounded `Time` values this is unreachable; tests
// use `arb_small_time` (tick counts ≤ 192_000) to stay safely
// bounded.
def_conn_marker!(
    TIMETIME,
    (Time, Time),
    Time,
    timetime_ceil,
    timetime_inner,
    timetime_floor
);

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

// Divisibility-lattice connection on `Grid`. `ceil = meet (GCD of
// tick counts)`, `floor = join (LCM)`, `inner = diagonal`.
def_conn_marker!(
    GRIDGRID,
    (Grid, Grid),
    Grid,
    gridgrid_ceil,
    gridgrid_inner,
    gridgrid_floor
);

// ── SampleTickConn: Sample ↔ Tick bridge ─────────────────────────
//
// Conn-shaped struct (not a real `Conn` — its `(ceil, inner, floor)`
// captures runtime `(sr, bpm, ppqn)` state, which `connections::Conn`
// can't express via its bare `fn` pointers). The adjoint laws are
// verified by the `sample_tick_*` proptests below.
//
// Lives here because Sample ↔ Tick is a connection over the `Tick`
// type the rest of this module already owns. Plan 2026-04-29-01 T3
// merged it back from `crate::time::conn` — the "no tempo
// coupling" invariant on `time/` was relaxed in Plan 2026-04-29-01,
// where `Tempo` itself moved to `conn::tempo` (the conn-shaped value
// types) and the layering rule pins the partial order more strictly
// than the prose convention did.

/// Sample ↔ Tick bridge parameterised by sample rate, tempo, and PPQN.
///
/// Mirrors the Galois `(ceil, inner, floor)` shape for `Sample`/`Tick`
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
    bpm: Tempo,
    ppqn: u32,
}

impl SampleTickConn {
    /// # Panics
    ///
    /// Panics if `sr == 0`, `ppqn == 0`, or `bpm.0 == 0`. These are
    /// programming errors — every call site either ships fixed
    /// constants or validates at a CLI/config boundary.
    pub fn new(sr: u32, bpm: Tempo, ppqn: u32) -> Self {
        assert!(sr > 0, "sample rate must be positive");
        assert!(ppqn > 0, "ppqn must be positive");
        assert!(bpm.0 > 0, "bpm must be positive, got {:?}", bpm);
        Self { sr, bpm, ppqn }
    }

    pub fn sr(&self) -> u32 {
        self.sr
    }
    pub fn bpm(&self) -> Tempo {
        self.bpm
    }
    pub fn ppqn(&self) -> u32 {
        self.ppqn
    }

    /// Tick → Sample. Exact when `tick × sr × 60 × 10⁶` is divisible
    /// by `bpm_µ × ppqn` (e.g. 48 kHz / 120 BPM / 960 PPQN is exact);
    /// otherwise rounded to the nearest `u64` (half-away-from-zero —
    /// both quantities are non-negative). Saturates to `u64::MAX`
    /// for pathological inputs whose quotient exceeds `u64::MAX`
    /// (e.g. `Tick(u32::MAX)` with `bpm_µ = 1`, `ppqn = 1`); mirrors
    /// the `to_tick` clamp on the inverse direction. The wrap was
    /// flagged on PR #35; saturation closes it.
    pub fn inner(&self, tick: Tick) -> u64 {
        // sample = tick · sr · 60 · 10⁶ / (bpm_µ · ppqn)
        let num = u128::from(tick.0) * u128::from(self.sr) * 60 * 1_000_000;
        let denom = u128::from(self.bpm.0) * u128::from(self.ppqn);
        // Round to nearest: (num + denom/2) / denom. Half-up because
        // both num and denom are non-negative.
        let q = (num + denom / 2) / denom;
        U128U064.ceil(q)
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
        Tick(U128U064.ceil(x))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conn::arb::arb_rational_nonneg;
    use crate::time::arb::arb_grid;
    use crate::time::arb::{arb_small_time, arb_tick, arb_time};
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

#[cfg(test)]
mod sample_tick_tests {
    //! Tests for `SampleTickConn`, merged in from
    //! `crate::time::conn` (Plan 2026-04-29-01 T3). The tests
    //! live in their own module — separate from `tests` above — so
    //! the `Conn`-flavored Galois tests stay distinct from the
    //! tempo-coupled bridge tests.

    use super::*;
    use crate::conn::fixed::Pico;
    use proptest::prelude::*;

    fn mbpm(b: u32) -> Tempo {
        Tempo::from_bpm_integer(b)
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
            t in 0u64..=1_000_000,
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

        /// Pathological inputs deliberately fish for the u128→u64
        /// narrow inside `SampleTickConn::inner`. With `Tick(u32::MAX)`
        /// (≈ 4.3×10⁹), `sr = 192 kHz`, and tiny `bpm_µ` / `ppqn`,
        /// `num = tick × sr × 60×10⁶ ≈ 5×10²²`. Even at the largest
        /// `bpm_µ × ppqn = 800` denominator, the exact quotient
        /// (~6×10¹⁹) exceeds `u64::MAX` (~1.84×10¹⁹), so every
        /// sampled point hits the saturation branch — without the
        /// clamp the u128→u64 narrow would wrap modulo 2⁶⁴ and
        /// return garbage. Per CLAUDE.md the anti-pattern is bounding
        /// to *avoid* boundaries; here the bounds are set to *reach*
        /// the wrap, which is the legitimate inverse of the rule.
        ///
        /// The realistic-input region is covered by the
        /// `arb_integer_stc()`-driven proptests above. The Tick u64
        /// horizon (values past `u32::MAX`) is exercised by the
        /// `sample_tick_inner_saturates_at_u64_horizon` spot check.
        #[test]
        fn sample_tick_inner_saturates_on_overflow(
            tick in u64::from(u32::MAX / 2)..=u64::from(u32::MAX),
            bpm_u in 1u32..=100,
            ppqn in 1u32..=8,
        ) {
            // Highest-rate sr maximises the numerator and so the
            // wrap region.
            let sr = 192_000u32;
            let stc = SampleTickConn::new(sr, Tempo(bpm_u), ppqn);
            let result = stc.inner(Tick(tick));
            // Independent reference: the exact (un-narrowed)
            // quotient in u128, saturated to u64::MAX.
            let num = u128::from(tick) * u128::from(sr) * 60 * 1_000_000;
            let denom = u128::from(bpm_u) * u128::from(ppqn);
            let exact = (num + denom / 2) / denom;
            let expected = exact.min(u128::from(u64::MAX)) as u64;
            // For this input region, the exact quotient strictly
            // exceeds u64::MAX, so every case must saturate.
            prop_assert_eq!(result, u64::MAX);
            prop_assert_eq!(result, expected);
        }
    }

    /// Tick widening to u64 (Plan 2026-04-28-07 T1) opened a new
    /// horizon above `u32::MAX`. Spot-check that `inner` saturates
    /// cleanly there — without the clamp, `Tick(u64::MAX)` would
    /// wrap modulo 2⁶⁴ inside the u128→u64 narrow.
    #[test]
    fn sample_tick_inner_saturates_at_u64_horizon() {
        let stc = SampleTickConn::new(192_000, Tempo(1), 1);
        // Tick(u64::MAX) × 192_000 × 60 × 1e6 / 1 vastly exceeds
        // u64::MAX after the u128 multiply — saturation is mandatory.
        assert_eq!(stc.inner(Tick(u64::MAX)), u64::MAX);
        // Realistic-tempo case at the same tick — still saturates,
        // but the math is closer to the boundary so a regression
        // narrowing too aggressively would be caught here.
        let stc_120 = SampleTickConn::new(48_000, Tempo::from_bpm_integer(120), 960);
        assert_eq!(stc_120.inner(Tick(u64::MAX)), u64::MAX);
    }

    // ── Pico ↔ Sample agreement with SampleTickConn ──────────────

    #[test]
    fn sample_tick_and_pico_to_samples_agree_at_120bpm_48k() {
        // 120 BPM / ppq=960 / 48 kHz: each quarter note = 0.5 s =
        // 24 000 samples = 5×10¹¹ pico. At tick 960 (one beat):
        let stc = SampleTickConn::new(48_000, mbpm(120), 960);

        let via_tick: u64 = stc.inner(Tick(960));
        let pico_at_one_beat = Pico(500_000_000_000);
        let via_pico: i64 = crate::conn::boundary::pico_to_samples(pico_at_one_beat, 48_000)
            .expect("48 kHz is supported");
        assert_eq!(via_tick, 24_000);
        assert_eq!(via_pico, 24_000);
        assert_eq!(via_tick as i64, via_pico);

        // And at tick 1920 (two beats = 1 s = 48 000 samples = 10¹² pico):
        assert_eq!(stc.inner(Tick(1920)), 48_000);
        assert_eq!(
            crate::conn::boundary::pico_to_samples(Pico(1_000_000_000_000), 48_000),
            Some(48_000)
        );
    }

    #[test]
    fn pico_to_samples_rejects_unsupported_rate() {
        assert_eq!(
            crate::conn::boundary::pico_to_samples(Pico(0), 22_050),
            None
        );
        assert_eq!(crate::conn::boundary::pico_to_samples(Pico(0), 0), None);
        assert_eq!(
            crate::conn::boundary::pico_to_samples(Pico(1_000_000_000_000), 48_000),
            Some(48_000)
        );
    }

    // ──────────────────────────────────────────────────────────────
    // Integer-exactness at 960 PPQN / 48k / 96k for divisor BPMs.
    //
    // Absorbed from `time/exact_rates.rs` (Plan 2026-04-28-03 T3) —
    // that file was 200 lines of tests for `SampleTickConn` mis-located
    // under `time/`, with a misleading name suggesting audio-rate
    // integer-exactness in general. The content is specifically about
    // `SampleTickConn::inner` rounding-free at common DAW configs.
    // ──────────────────────────────────────────────────────────────
    mod exactness {
        use super::*;
        use crate::time::tick::PPQN;

        /// Divisors of `n`, sorted ascending. Used to enumerate the
        /// integer-exact BPMs at a given sample rate.
        fn divisors(n: u32) -> Vec<u32> {
            let mut out = Vec::new();
            let mut k = 1u32;
            while k * k <= n {
                if n % k == 0 {
                    out.push(k);
                    if k != n / k {
                        out.push(n / k);
                    }
                }
                k += 1;
            }
            out.sort_unstable();
            out
        }

        /// `Tempo::from_bpm_integer` caps at `u32::MAX / 1_000_000 = 4294`
        /// because tempo storage is `bpm × 10⁶`. We filter the strategies
        /// to that bound; in practice every musically-meaningful BPM (≤ a
        /// few hundred) falls well below it.
        const BPM_MAX: u32 = 4294;

        #[test]
        fn divisors_of_3000_includes_common_bpms() {
            let d = divisors(3000);
            assert!(d.contains(&60));
            assert!(d.contains(&120));
            assert!(d.contains(&125));
            assert!(d.contains(&250));
            // 137 doesn't divide 3000.
            assert!(!d.contains(&137));
            // 240 does NOT divide 3000 (3000 = 2³·3·5³, lacks 2⁴).
            assert!(!d.contains(&240));
        }

        /// Strategy yielding `(sr, bpm)` pairs where `tick_to_sample` is
        /// integer-exact at 960 PPQN. At 48 kHz that's BPMs dividing 3000;
        /// at 96 kHz, divisors of 6000. Filtered to `bpm ∈ (0, 4294]` to
        /// stay inside `Tempo`'s storage range.
        fn arb_exact_sr_bpm() -> impl Strategy<Value = (u32, u32)> {
            let d48: Vec<u32> = divisors(3000)
                .into_iter()
                .filter(|&b| b > 0 && b <= BPM_MAX)
                .collect();
            let d96: Vec<u32> = divisors(6000)
                .into_iter()
                .filter(|&b| b > 0 && b <= BPM_MAX)
                .collect();
            prop_oneof![
                prop::sample::select(d48).prop_map(|bpm| (48_000u32, bpm)),
                prop::sample::select(d96).prop_map(|bpm| (96_000u32, bpm)),
            ]
        }

        /// Strategy yielding only `(48_000, bpm)` exact pairs.
        fn arb_exact_48k_bpm() -> impl Strategy<Value = u32> {
            let d: Vec<u32> = divisors(3000)
                .into_iter()
                .filter(|&b| b > 0 && b <= BPM_MAX)
                .collect();
            prop::sample::select(d)
        }

        /// Strategy yielding only `(96_000, bpm)` exact pairs.
        fn arb_exact_96k_bpm() -> impl Strategy<Value = u32> {
            let d: Vec<u32> = divisors(6000)
                .into_iter()
                .filter(|&b| b > 0 && b <= BPM_MAX)
                .collect();
            prop::sample::select(d)
        }

        /// "Exact" at the SampleTickConn level means
        /// `inner(tick) * bpm_µ * ppqn == tick * sr * 60 · 10⁶` — i.e.
        /// the half-up rounding step in `SampleTickConn::inner` collapses
        /// to a no-op.
        fn assert_exact(stc: &SampleTickConn, tick: Tick) {
            let sample = stc.inner(tick);
            let lhs = u128::from(sample) * u128::from(stc.bpm().0) * u128::from(stc.ppqn());
            let rhs = u128::from(tick.0) * u128::from(stc.sr()) * 60 * 1_000_000;
            assert_eq!(
                lhs,
                rhs,
                "tick {} not exact at sr={} bpm_µ={} ppqn={} (sample={})",
                tick.0,
                stc.sr(),
                stc.bpm().0,
                stc.ppqn(),
                sample,
            );
        }

        proptest! {
            /// Plan property `stc_samples_per_tick_is_exact_at_48k`: at
            /// `sr=48_000, ppqn=960`, integer BPMs that divide 3000 yield
            /// rounding-free `tick → sample` for any tick.
            #[test]
            fn stc_samples_per_tick_is_exact_at_48k(
                bpm in arb_exact_48k_bpm(),
                t in 0u64..=10_000_000,
            ) {
                let stc = SampleTickConn::new(48_000, Tempo::from_bpm_integer(bpm), PPQN);
                assert_exact(&stc, Tick(t));
            }

            /// Plan property `stc_samples_per_tick_is_exact_at_96k`: same
            /// at 96 kHz, BPMs dividing 6000.
            #[test]
            fn stc_samples_per_tick_is_exact_at_96k(
                bpm in arb_exact_96k_bpm(),
                t in 0u64..=10_000_000,
            ) {
                let stc = SampleTickConn::new(96_000, Tempo::from_bpm_integer(bpm), PPQN);
                assert_exact(&stc, Tick(t));
            }

            /// Plan property `stc_round_trip_identity_48k_96k`: at any
            /// integer-exact `(sr, bpm)` pair, the `Tick → Sample → Tick`
            /// round trip is the identity (both `floor` and `ceil` agree
            /// with `inner`'s exact result on every tick).
            #[test]
            fn stc_round_trip_identity_48k_96k(
                (sr, bpm) in arb_exact_sr_bpm(),
                t in 0u64..=1_000_000,
            ) {
                let stc = SampleTickConn::new(sr, Tempo::from_bpm_integer(bpm), PPQN);
                let tick = Tick(t);
                let sample = stc.inner(tick);
                prop_assert_eq!(stc.floor(sample), tick);
                prop_assert_eq!(stc.ceil(sample), tick);
            }
        }

        // ── Spot checks ──────────────────────────────────────────────

        #[test]
        fn exact_at_120bpm_48k_960ppqn() {
            // 120 BPM × 960 PPQN at 48 kHz: tick 1 = 25 samples.
            let stc = SampleTickConn::new(48_000, Tempo::from_bpm_integer(120), PPQN);
            assert_eq!(stc.inner(Tick(1)), 25);
            assert_exact(&stc, Tick(1));
            assert_exact(&stc, Tick(960));
            assert_exact(&stc, Tick(3840));
        }

        #[test]
        fn exact_at_125bpm_48k_960ppqn() {
            // 125 divides 3000 → exact at 48k. 1 tick = (48000·60·10⁶) /
            // (125·10⁶·960) = 24 samples.
            let stc = SampleTickConn::new(48_000, Tempo::from_bpm_integer(125), PPQN);
            assert_eq!(stc.inner(Tick(1)), 24);
            assert_exact(&stc, Tick(1));
        }

        #[test]
        fn exact_at_60bpm_96k_960ppqn() {
            // 60 divides 6000. 1 tick at 96k / 60 BPM = 100 samples.
            let stc = SampleTickConn::new(96_000, Tempo::from_bpm_integer(60), PPQN);
            assert_eq!(stc.inner(Tick(1)), 100);
            assert_exact(&stc, Tick(1));
        }

        /// Sanity: a non-divisor BPM is *not* exact — the `assert_exact`
        /// helper must be discriminating, not vacuously true.
        #[test]
        fn non_divisor_bpm_is_not_exact() {
            // 137 doesn't divide 3000. tick=1 → 48000·60·10⁶ / (137·10⁶·960)
            // = 21.897… samples; rounded to 22 → not exact.
            let stc = SampleTickConn::new(48_000, Tempo::from_bpm_integer(137), PPQN);
            let sample = stc.inner(Tick(1));
            let lhs = u128::from(sample) * u128::from(stc.bpm().0) * u128::from(stc.ppqn());
            let rhs = u128::from(1u32) * u128::from(stc.sr()) * 60 * 1_000_000;
            assert_ne!(lhs, rhs, "137 BPM should not be integer-exact at 48k/960");
        }
    }
}
