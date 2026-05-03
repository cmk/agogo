//! Marker-backed Galois connections for `Tick`, `Time`, and `Grid`.
//!
//! Three static marker values port the Haskell Cirklon conversions:
//!
//! | Rust            | Haskell      | Public API                                |
//! |-----------------|--------------|-------------------------------------------|
//! | [`TICKTIME`]    | `ticks`      | marker with `ViewL<Tick, Time>` + `ViewR<Tick, Time>` |
//! | [`TIMETIME`]    | `time`       | marker with `ViewL<(Time, Time), Time>` + `ViewR<(Time, Time), Time>` |
//! | [`GRIDGRID`]    | `tbase`      | marker with `ViewL<(Grid, Grid), Grid>` + `ViewR<(Grid, Grid), Grid>` |
//!
//! Naming: per CLAUDE.md, Conn accessors are 8-char identifiers
//! built from two 4-char side names. Single-type-side Conns
//! (`TICKTIME`) follow the rule directly. Pair-side Conns
//! (`TIMETIME`, `GRIDGRID`) duplicate the side name. The Haskell
//! `quantizeAt` helper is intentionally not ported as an agogo Conn:
//! its laws require a `Time` codomain restricted to the selected grid,
//! while agogo callers can use [`TICKTIME`] plus an explicit resolution
//! [`Time`] when they need fixed-grid binning.
//!
//! The three static connections are zero-sized marker values matching
//! upstream's triple API. Their inherent `.ceil()`, `.inner()`, and
//! `.floor()` methods forward to kind-tagged
//! [`connections::conn::ConnL`] / [`connections::conn::ConnR`] views
//! built from bare `fn` pointers.
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

use connections::conn::{ViewL, ViewR};
use connections::fixed::u64::U128U064;

use crate::conn::tempo::Tempo;
use crate::time::grid::Grid;
use crate::time::tick::{Tick, Time, from_ticks, time_to_tick};
use connections::lattice::{Join, Meet};

// ── ticktime: Tick ↔ Time marker ─────────────────────────────────

fn finite_time_ceil(n: Tick) -> Option<Time> {
    let mut best: Option<(u64, Time)> = None;
    for g in Grid::ALL {
        let tc = u64::from(g.tick_count());
        let beats = n.0.div_ceil(tc);
        if beats <= u64::from(u32::MAX) {
            let ticks = beats * tc;
            if best.is_none_or(|(best_ticks, _)| ticks < best_ticks) {
                best = Some((
                    ticks,
                    Time::At {
                        beats: beats as u32,
                        base: g,
                    },
                ));
            }
        }
    }
    best.map(|(_, time)| time)
}

fn finite_time_floor(n: Tick) -> Time {
    let mut best_ticks = 0;
    let mut best_time = Time::At {
        beats: 0,
        base: Grid::T1,
    };
    for g in Grid::ALL {
        let tc = u64::from(g.tick_count());
        let beats = (n.0 / tc).min(u64::from(u32::MAX));
        let ticks = beats * tc;
        if ticks > best_ticks {
            best_ticks = ticks;
            best_time = Time::At {
                beats: beats as u32,
                base: g,
            };
        }
    }
    best_time
}

fn ticktime_ceil(n: Tick) -> Time {
    finite_time_ceil(n).unwrap_or(Time::End)
}

fn ticktime_inner(t: Time) -> Tick {
    time_to_tick(t)
}

fn ticktime_floor(n: Tick) -> Time {
    if n.0 == u64::MAX {
        Time::End
    } else {
        finite_time_floor(n)
    }
}

// Master `Tick ↔ Time` connection. Finite values canonicalise to
// the nearest representable `Time`; values above the finite horizon
// ceil to `End`. The right adjoint maps only `Tick::MAX` to `End`;
// lower overflow ticks floor to the greatest finite `Time`.
connections::triple! {
    #[allow(non_camel_case_types)]
    #[derive(Copy, Clone, Debug, Default)]
    pub TICKTIME : Tick => Time {
        ceil:  ticktime_ceil,
        inner: ticktime_inner,
        floor: ticktime_floor,
    }
}

impl TICKTIME {
    pub fn ceil(self, x: Tick) -> Time {
        <Self as ViewL<Tick, Time>>::L.ceil(x)
    }

    pub fn inner(self, x: Time) -> Tick {
        <Self as ViewL<Tick, Time>>::L.inner(x)
    }

    pub fn floor(self, x: Tick) -> Time {
        <Self as ViewR<Tick, Time>>::R.floor(x)
    }
}

// ── timetime: (Time, Time) ↔ Time marker ─────────────────────────

fn gcd_u64(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a
}

fn checked_lcm_u64(a: u64, b: u64) -> Option<u64> {
    if a == 0 || b == 0 {
        Some(0)
    } else {
        (a / gcd_u64(a, b)).checked_mul(b)
    }
}

fn timetime_ceil(ab: (Time, Time)) -> Time {
    let (a, b) = ab;
    if matches!(a, Time::End) || matches!(b, Time::End) {
        return Time::End;
    }
    let g = gcd_u64(time_to_tick(a).0, time_to_tick(b).0);
    from_ticks(Tick(g)).unwrap_or(Time::End)
}

fn timetime_inner(t: Time) -> (Time, Time) {
    (t, t)
}

fn timetime_floor(ab: (Time, Time)) -> Time {
    let (a, b) = ab;
    match (a, b) {
        (Time::End, Time::End) => return Time::End,
        (Time::End, finite) | (finite, Time::End) => return finite,
        _ => {}
    }
    checked_lcm_u64(time_to_tick(a).0, time_to_tick(b).0)
        .and_then(|l| from_ticks(Tick(l)))
        .unwrap_or(Time::At {
            beats: 0,
            base: Grid::T1,
        })
}

// Divisibility-lattice connection on `Time`.
//
// `ceil = meet (GCD)`, `floor = join (LCM)`, `inner = diagonal`.
// Following Haskell convention — the relevant order here is
// divisibility of tick counts, not magnitude.
//
// `Time::End` is the refinement top. LCM overflow or finite-horizon
// misses map to the existing refinement bottom, finite zero.
connections::triple! {
    #[allow(non_camel_case_types)]
    #[derive(Copy, Clone, Debug, Default)]
    pub TIMETIME : (Time, Time) => Time {
        ceil:  timetime_ceil,
        inner: timetime_inner,
        floor: timetime_floor,
    }
}

impl TIMETIME {
    pub fn ceil(self, x: (Time, Time)) -> Time {
        <Self as ViewL<(Time, Time), Time>>::L.ceil(x)
    }

    pub fn inner(self, x: Time) -> (Time, Time) {
        <Self as ViewL<(Time, Time), Time>>::L.inner(x)
    }

    pub fn floor(self, x: (Time, Time)) -> Time {
        <Self as ViewR<(Time, Time), Time>>::R.floor(x)
    }
}

// ── gridgrid: (Grid, Grid) ↔ Grid marker ─────────────────────────

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
connections::triple! {
    #[allow(non_camel_case_types)]
    #[derive(Copy, Clone, Debug, Default)]
    pub GRIDGRID : (Grid, Grid) => Grid {
        ceil:  gridgrid_ceil,
        inner: gridgrid_inner,
        floor: gridgrid_floor,
    }
}

impl GRIDGRID {
    pub fn ceil(self, x: (Grid, Grid)) -> Grid {
        <Self as ViewL<(Grid, Grid), Grid>>::L.ceil(x)
    }

    pub fn inner(self, x: Grid) -> (Grid, Grid) {
        <Self as ViewL<(Grid, Grid), Grid>>::L.inner(x)
    }

    pub fn floor(self, x: (Grid, Grid)) -> Grid {
        <Self as ViewR<(Grid, Grid), Grid>>::R.floor(x)
    }
}

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
    use crate::time::arb::arb_grid;
    use crate::time::arb::{arb_any_tick, arb_small_time, arb_time};
    use proptest::prelude::*;

    // ── Spot checks ──────────────────────────────────────────────

    #[test]
    fn ticktime_inner_is_exact() {
        let c = TICKTIME;
        let t = Time::At {
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
            Time::At {
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
        assert_eq!(c.inner(t).0, 50);
    }

    #[test]
    fn ticktime_ceil_unaligned() {
        let c = TICKTIME;
        // At 960 PPQN every Tick is on Grid::T512P (=1). So ceil and
        // floor both yield the canonical form for `n` itself.
        let t = c.ceil(Tick(50));
        assert_eq!(c.inner(t).0, 50);
    }

    #[test]
    fn ticktime_ceil_overflow_is_top() {
        let c = TICKTIME;
        assert_eq!(c.ceil(Tick(u64::MAX)), Time::End);
    }

    #[test]
    fn ticktime_floor_max_is_top() {
        let c = TICKTIME;
        assert_eq!(c.floor(Tick(u64::MAX)), Time::End);
    }

    #[test]
    fn ticktime_floor_high_overflow_is_greatest_finite_below_max() {
        let c = TICKTIME;
        let t = c.floor(Tick(u64::MAX - 1));
        assert_eq!(
            t,
            Time::At {
                beats: u32::MAX,
                base: Grid::T1,
            }
        );
    }

    #[test]
    fn timetime_ceil_gcd_of_t4_t8() {
        let c = TIMETIME;
        let a = Time::At {
            beats: 1,
            base: Grid::T4,
        }; // 960 ticks
        let b = Time::At {
            beats: 1,
            base: Grid::T8,
        }; // 480 ticks
        // gcd(960, 480) = 480 → Time 1 T8
        assert_eq!(
            c.ceil((a, b)),
            Time::At {
                beats: 1,
                base: Grid::T8
            }
        );
    }

    #[test]
    fn timetime_floor_lcm_of_t16_and_t16t() {
        let c = TIMETIME;
        let a = Time::At {
            beats: 1,
            base: Grid::T16,
        }; // 240 ticks
        let b = Time::At {
            beats: 1,
            base: Grid::T16T,
        }; // 160 ticks
        // lcm(240, 160) = 480 → Time 1 T8
        assert_eq!(
            c.floor((a, b)),
            Time::At {
                beats: 1,
                base: Grid::T8
            }
        );
    }

    #[test]
    fn timetime_ceil_end_end_is_end() {
        let c = TIMETIME;
        assert_eq!(c.ceil((Time::End, Time::End)), Time::End);
    }

    #[test]
    fn timetime_floor_end_and_finite_is_finite() {
        let c = TIMETIME;
        let finite = Time::At {
            beats: 1,
            base: Grid::T4,
        };
        assert_eq!(c.floor((Time::End, finite)), finite);
    }

    #[test]
    fn timetime_ceil_end_and_finite_is_end() {
        let c = TIMETIME;
        let finite = Time::At {
            beats: 1,
            base: Grid::T4,
        };
        assert_eq!(c.ceil((Time::End, finite)), Time::End);
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
        fn ticktime_adjoint(a in arb_any_tick(), b in arb_time()) {
            let c = TICKTIME;
            let lhs = c.ceil(a) <= b;
            let rhs = a <= c.inner(b);
            prop_assert_eq!(lhs, rhs);
        }

        #[test]
        fn ticktime_floor_adjoint(a in arb_any_tick(), b in arb_time()) {
            let c = TICKTIME;
            let lhs = c.inner(b) <= a;
            let rhs = b <= c.floor(a);
            prop_assert_eq!(lhs, rhs);
        }

        #[test]
        fn ticktime_closed(a in arb_any_tick()) {
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
            a1 in arb_any_tick(), a2 in arb_any_tick(),
            b1 in arb_time(), b2 in arb_time(),
        ) {
            let c = TICKTIME;
            if a1 <= a2 {
                prop_assert!(c.ceil(a1) <= c.ceil(a2));
                prop_assert!(c.floor(a1) <= c.floor(a2));
            }
            if b1 <= b2 {
                prop_assert!(c.inner(b1) <= c.inner(b2));
            }
        }

        #[test]
        fn ticktime_idempotent(a in arb_any_tick()) {
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

    }

    // ── Lattice-connection laws for `TIMETIME` and `GRIDGRID` ────
    //
    // The adjoint structure `join ⊣ diag ⊣ meet` holds under the
    // "refine-to" order: `a ≤ b ⟺ tc(b) divides tc(a)` (i.e. "b is at
    // least as fine as a"). The standard divisibility `PartialOrd`
    // for `Grid` orients the other way around and would give a non-
    // adjoint structure here, so we use ad-hoc `refine_le` helpers.

    fn gridgrid_refine_le(a: Grid, b: Grid) -> bool {
        a.tick_count() % b.tick_count() == 0
    }

    fn timetime_refine_le(a: Time, b: Time) -> bool {
        if matches!(b, Time::End) {
            return true;
        }
        if matches!(a, Time::End) {
            return false;
        }
        let ta = time_to_tick(a).0;
        let tb = time_to_tick(b).0;
        if tb == 0 { ta == 0 } else { ta % tb == 0 }
    }

    fn timetime_pair_refine_le(a: (Time, Time), b: (Time, Time)) -> bool {
        timetime_refine_le(a.0, b.0) && timetime_refine_le(a.1, b.1)
    }

    macro_rules! law_battery_with_order {
        (
            mod $m:ident,
            fine: $fine:expr,
            coarse: $coarse:expr,
            fine_le: $fine_le:path,
            coarse_le: $coarse_le:path,
            ceil: $ceil:path,
            inner: $inner:path,
            floor: $floor:path $(,)?
        ) => {
            mod $m {
                use super::*;

                proptest! {
                    #[test]
                    fn galois_l(a in $fine, b in $coarse) {
                        prop_assert_eq!($coarse_le($ceil(a), b), $fine_le(a, $inner(b)));
                    }

                    #[test]
                    fn galois_r(a in $fine, b in $coarse) {
                        prop_assert_eq!($fine_le($inner(b), a), $coarse_le(b, $floor(a)));
                    }

                    #[test]
                    fn closure_l(a in $fine) {
                        prop_assert!($fine_le(a, $inner($ceil(a))));
                    }

                    #[test]
                    fn closure_r(a in $fine) {
                        prop_assert!($fine_le($inner($floor(a)), a));
                    }

                    #[test]
                    fn kernel_l(b in $coarse) {
                        prop_assert!($coarse_le($ceil($inner(b)), b));
                    }

                    #[test]
                    fn kernel_r(b in $coarse) {
                        prop_assert!($coarse_le(b, $floor($inner(b))));
                    }

                    #[test]
                    fn monotone_l(a1 in $fine, a2 in $fine) {
                        if $fine_le(a1, a2) {
                            prop_assert!($coarse_le($ceil(a1), $ceil(a2)));
                        }
                    }

                    #[test]
                    fn monotone_r(b1 in $coarse, b2 in $coarse) {
                        if $coarse_le(b1, b2) {
                            prop_assert!($fine_le($inner(b1), $inner(b2)));
                        }
                    }

                    #[test]
                    fn idempotent(a in $fine) {
                        let once = $inner($ceil(a));
                        let twice = $inner($ceil(once));
                        prop_assert_eq!(once, twice);
                    }

                    #[test]
                    fn floor_le_ceil(a in $fine) {
                        prop_assert!($coarse_le($floor(a), $ceil(a)));
                    }

                    #[test]
                    fn order_reflecting(b1 in $coarse, b2 in $coarse) {
                        if $fine_le($inner(b1), $inner(b2)) {
                            prop_assert!($coarse_le(b1, b2));
                        }
                    }
                }
            }
        };
    }

    law_battery_with_order! {
        mod timetime_law_battery,
        fine: (arb_time(), arb_time()),
        coarse: arb_time(),
        fine_le: timetime_pair_refine_le,
        coarse_le: timetime_refine_le,
        ceil: timetime_ceil,
        inner: timetime_inner,
        floor: timetime_floor,
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
            prop_assert_eq!(
                time_to_tick(c.floor((a, b))).0,
                checked_lcm_u64(ta, tb).expect("small Time LCM fits in u64")
            );
        }

        #[test]
        fn time_inner_is_diagonal(t in arb_time()) {
            let c = TIMETIME;
            prop_assert_eq!(c.inner(t), (t, t));
        }

        #[test]
        fn time_adjoint(
            a in arb_time(), b in arb_time(), z in arb_time(),
        ) {
            let c = TIMETIME;
            let lhs = timetime_refine_le(c.ceil((a, b)), z);
            let rhs = timetime_refine_le(a, z) && timetime_refine_le(b, z);
            prop_assert_eq!(lhs, rhs);
        }

        #[test]
        fn time_closed(a in arb_time(), b in arb_time()) {
            let c = TIMETIME;
            let (x, y) = c.inner(c.ceil((a, b)));
            prop_assert!(timetime_refine_le(a, x));
            prop_assert!(timetime_refine_le(b, y));
        }

        #[test]
        fn time_kernel(z in arb_time()) {
            let c = TIMETIME;
            prop_assert!(timetime_refine_le(c.ceil(c.inner(z)), z));
        }

        #[test]
        fn time_idempotent(a in arb_time(), b in arb_time()) {
            let c = TIMETIME;
            let once = c.inner(c.ceil((a, b)));
            let twice = c.inner(c.ceil(once));
            prop_assert_eq!(once, twice);
        }

        #[test]
        fn time_monotonic(
            a1 in arb_time(), a2 in arb_time(),
            b1 in arb_time(), b2 in arb_time(),
            z1 in arb_time(), z2 in arb_time(),
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
