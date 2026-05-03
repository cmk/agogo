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
//! connections over the same divisibility order used by the channel
//! DSL: `meet` / `&` is GCD, and `join` / `|` is LCM. `TIMETIME` and
//! `GRIDGRID` therefore use `ceil = join (LCM)` and
//! `floor = meet (GCD)`. `TICKTIME` is different: it is a magnitude
//! connection between `Tick` and `Time`, so tests that need magnitude
//! comparison use [`TICKTIME::inner`] explicitly.

use connections::conn::{ViewL, ViewR};
use connections::fixed::u64::{I064U064, U128U064};

use crate::conn::sample::{
    S044, S044I064, S048, S048I064, S088, S088I064, S096, S096I064, S176, S176I064, S192, S192I064,
    SampleRate,
};
use crate::conn::tempo::Tempo;
use crate::time::grid::Grid;
use crate::time::tick::{PPQN, Tick, Time, from_ticks, time_to_tick};
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
    checked_lcm_u64(time_to_tick(a).0, time_to_tick(b).0)
        .and_then(|l| from_ticks(Tick(l)))
        .unwrap_or(Time::End)
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
    let g = gcd_u64(time_to_tick(a).0, time_to_tick(b).0);
    from_ticks(Tick(g)).unwrap_or(Time::End)
}

// Divisibility-lattice connection on `Time`.
//
// `ceil = join (LCM)`, `floor = meet (GCD)`, `inner = diagonal`.
// `Time::End` is top. LCM overflow or finite-horizon misses map to
// `End`, making the upper adjoint total instead of hiding the gap.
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
    a.join(&b)
}

fn gridgrid_inner(t: Grid) -> (Grid, Grid) {
    (t, t)
}

fn gridgrid_floor(ab: (Grid, Grid)) -> Grid {
    let (a, b) = ab;
    a.meet(&b)
}

// Divisibility-lattice connection on `Grid`. `ceil = join (LCM of
// tick counts)`, `floor = meet (GCD)`, `inner = diagonal`.
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

// ── Tick ↔ rate-typed sample time ─────────────────────────────────

fn tick_to_sample_bits<R: SampleRate>(tick: Tick, bpm: Tempo) -> i64 {
    if bpm.0 == 0 {
        return i64::MAX;
    }
    let num = u128::from(tick.0) * u128::from(R::HZ) * (1_u128 << 16) * 60_000_000;
    let den = u128::from(bpm.0) * u128::from(PPQN);
    let bits = num.div_ceil(den);
    bits.min(i64::MAX as u128) as i64
}

fn sample_to_tick_floor_for_rate(sample: u64, bpm: Tempo, sr: u32) -> Option<Tick> {
    if bpm.0 == 0 {
        return None;
    }
    let num = u128::from(sample) * u128::from(bpm.0) * u128::from(PPQN);
    let den = u128::from(sr) * 60_000_000;
    Some(Tick(U128U064.ceil(num / den)))
}

fn sample_to_tick_ceil_for_rate(sample: u64, bpm: Tempo, sr: u32) -> Option<Tick> {
    if bpm.0 == 0 {
        return None;
    }
    let num = u128::from(sample) * u128::from(bpm.0) * u128::from(PPQN);
    let den = u128::from(sr) * 60_000_000;
    Some(Tick(U128U064.ceil(num.div_ceil(den))))
}

macro_rules! tick_sample_fns {
    ($(($to_rate:ident, $Rate:ident, $Whole:ident, $hz:expr)),+ $(,)?) => {
        $(
            pub fn $to_rate(tick: Tick, bpm: Tempo) -> $Rate {
                $Rate::from_bits(tick_to_sample_bits::<$Rate>(tick, bpm))
            }
        )+

        pub fn tick_to_whole_samples(tick: Tick, bpm: Tempo, sr: u32) -> Option<u64> {
            if bpm.0 == 0 {
                return None;
            }
            Some(match sr {
                $(
                    $hz => I064U064.ceil($Whole.ceil($to_rate(tick, bpm))),
                )+
                _ => return None,
            })
        }

        pub fn sample_to_tick_floor(sample: u64, bpm: Tempo, sr: u32) -> Option<Tick> {
            match sr {
                $(
                    $hz => sample_to_tick_floor_for_rate(sample, bpm, $hz),
                )+
                _ => None,
            }
        }

        pub fn sample_to_tick_ceil(sample: u64, bpm: Tempo, sr: u32) -> Option<Tick> {
            match sr {
                $(
                    $hz => sample_to_tick_ceil_for_rate(sample, bpm, $hz),
                )+
                _ => None,
            }
        }
    };
}

tick_sample_fns!(
    (tick_to_s044, S044, S044I064, 44_100),
    (tick_to_s048, S048, S048I064, 48_000),
    (tick_to_s088, S088, S088I064, 88_200),
    (tick_to_s096, S096, S096I064, 96_000),
    (tick_to_s176, S176, S176I064, 176_400),
    (tick_to_s192, S192, S192I064, 192_000),
);

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
    fn timetime_ceil_lcm_of_t4_t8() {
        let c = TIMETIME;
        let a = Time::At {
            beats: 1,
            base: Grid::T4,
        }; // 960 ticks
        let b = Time::At {
            beats: 1,
            base: Grid::T8,
        }; // 480 ticks
        // lcm(960, 480) = 960 → Time 1 T4
        assert_eq!(
            c.ceil((a, b)),
            Time::At {
                beats: 1,
                base: Grid::T4
            }
        );
    }

    #[test]
    fn timetime_floor_gcd_of_t16_and_t16t() {
        let c = TIMETIME;
        let a = Time::At {
            beats: 1,
            base: Grid::T16,
        }; // 240 ticks
        let b = Time::At {
            beats: 1,
            base: Grid::T16T,
        }; // 160 ticks
        // gcd(240, 160) = 80 → Time 1 T32T
        assert_eq!(
            c.floor((a, b)),
            Time::At {
                beats: 1,
                base: Grid::T32T
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
    fn gridgrid_ceil_join_of_t4_t8() {
        let c = GRIDGRID;
        // lcm of tick counts: lcm(960, 480) = 960 = T4.
        assert_eq!(c.ceil((Grid::T4, Grid::T8)), Grid::T4);
    }

    #[test]
    fn gridgrid_floor_meet_of_t4_t8t() {
        let c = GRIDGRID;
        // T4 = 960, T8T = 320; gcd(960, 320) = 320 = T8T.
        assert_eq!(c.floor((Grid::T4, Grid::T8T)), Grid::T8T);
    }

    // ── Generic connections-tests laws for magnitude connections ──

    fn ticktime_magnitude_le(a: Time, b: Time) -> bool {
        TICKTIME.inner(a) <= TICKTIME.inner(b)
    }

    proptest! {
        // ── ticktime ─────────────────────────────────────────────

        #[test]
        fn ticktime_adjoint(a in arb_any_tick(), b in arb_time()) {
            let c = TICKTIME;
            let lhs = ticktime_magnitude_le(c.ceil(a), b);
            let rhs = a <= c.inner(b);
            prop_assert_eq!(lhs, rhs);
        }

        #[test]
        fn ticktime_floor_adjoint(a in arb_any_tick(), b in arb_time()) {
            let c = TICKTIME;
            let lhs = c.inner(b) <= a;
            let rhs = ticktime_magnitude_le(b, c.floor(a));
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
            prop_assert!(ticktime_magnitude_le(c.ceil(c.inner(b)), b));
        }

        #[test]
        fn ticktime_monotonic(
            a1 in arb_any_tick(), a2 in arb_any_tick(),
            b1 in arb_time(), b2 in arb_time(),
        ) {
            let c = TICKTIME;
            if a1 <= a2 {
                prop_assert!(ticktime_magnitude_le(c.ceil(a1), c.ceil(a2)));
                prop_assert!(ticktime_magnitude_le(c.floor(a1), c.floor(a2)));
            }
            if c.inner(b1) <= c.inner(b2) {
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
    // These laws use the same divisibility order as `Grid::PartialOrd`
    // and `Time::PartialOrd`: `a ≤ b` iff `a`'s tick count divides
    // `b`'s tick count.

    fn gridgrid_le(a: Grid, b: Grid) -> bool {
        a <= b
    }

    fn timetime_le(a: Time, b: Time) -> bool {
        a <= b
    }

    fn timetime_pair_le(a: (Time, Time), b: (Time, Time)) -> bool {
        timetime_le(a.0, b.0) && timetime_le(a.1, b.1)
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
        fine_le: timetime_pair_le,
        coarse_le: timetime_le,
        ceil: timetime_ceil,
        inner: timetime_inner,
        floor: timetime_floor,
    }

    proptest! {
        // ── gridgrid connection ──────────────────────────────────

        #[test]
        fn gridgrid_ceil_is_join(a in arb_grid(), b in arb_grid()) {
            let c = GRIDGRID;
            prop_assert_eq!(c.ceil((a, b)), a.join(&b));
        }

        #[test]
        fn gridgrid_floor_is_meet(a in arb_grid(), b in arb_grid()) {
            let c = GRIDGRID;
            prop_assert_eq!(c.floor((a, b)), a.meet(&b));
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
            let lhs = gridgrid_le(c.ceil((a, b)), z);
            let rhs = gridgrid_le(a, z) && gridgrid_le(b, z);
            prop_assert_eq!(lhs, rhs);
        }

        #[test]
        fn gridgrid_closed(a in arb_grid(), b in arb_grid()) {
            let c = GRIDGRID;
            let (x, y) = c.inner(c.ceil((a, b)));
            prop_assert!(gridgrid_le(a, x));
            prop_assert!(gridgrid_le(b, y));
        }

        #[test]
        fn gridgrid_kernel(z in arb_grid()) {
            let c = GRIDGRID;
            prop_assert!(gridgrid_le(c.ceil(c.inner(z)), z));
        }

        #[test]
        fn gridgrid_monotonic(
            a1 in arb_grid(), a2 in arb_grid(),
            b1 in arb_grid(), b2 in arb_grid(),
            z1 in arb_grid(), z2 in arb_grid(),
        ) {
            let c = GRIDGRID;
            if gridgrid_le(a1, a2) && gridgrid_le(b1, b2) {
                prop_assert!(gridgrid_le(c.ceil((a1, b1)), c.ceil((a2, b2))));
            }
            if gridgrid_le(z1, z2) {
                let (x1, y1) = c.inner(z1);
                let (x2, y2) = c.inner(z2);
                prop_assert!(gridgrid_le(x1, x2));
                prop_assert!(gridgrid_le(y1, y2));
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
        fn timetime_ceil_is_lcm_on_ticks(
            a in arb_small_time(), b in arb_small_time(),
        ) {
            let c = TIMETIME;
            let ta = time_to_tick(a).0;
            let tb = time_to_tick(b).0;
            prop_assert_eq!(
                time_to_tick(c.ceil((a, b))).0,
                checked_lcm_u64(ta, tb).expect("small Time LCM fits in u64")
            );
        }

        #[test]
        fn timetime_floor_is_gcd_on_ticks(
            a in arb_small_time(), b in arb_small_time(),
        ) {
            let c = TIMETIME;
            let ta = time_to_tick(a).0;
            let tb = time_to_tick(b).0;
            prop_assert_eq!(time_to_tick(c.floor((a, b))).0, gcd_u64(ta, tb));
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
            let lhs = timetime_le(c.ceil((a, b)), z);
            let rhs = timetime_le(a, z) && timetime_le(b, z);
            prop_assert_eq!(lhs, rhs);
        }

        #[test]
        fn time_closed(a in arb_time(), b in arb_time()) {
            let c = TIMETIME;
            let (x, y) = c.inner(c.ceil((a, b)));
            prop_assert!(timetime_le(a, x));
            prop_assert!(timetime_le(b, y));
        }

        #[test]
        fn time_kernel(z in arb_time()) {
            let c = TIMETIME;
            prop_assert!(timetime_le(c.ceil(c.inner(z)), z));
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
            if timetime_le(a1, a2) && timetime_le(b1, b2) {
                prop_assert!(
                    timetime_le(c.ceil((a1, b1)), c.ceil((a2, b2)))
                );
            }
            if timetime_le(z1, z2) {
                let (x1, y1) = c.inner(z1);
                let (x2, y2) = c.inner(z2);
                prop_assert!(timetime_le(x1, x2));
                prop_assert!(timetime_le(y1, y2));
            }
        }
    }
}

#[cfg(test)]
mod tick_sample_tests {
    use super::*;
    use proptest::prelude::*;

    fn exact_bits(tick: Tick, bpm: Tempo, sr: u32) -> i64 {
        let num = u128::from(tick.0) * u128::from(sr) * (1_u128 << 16) * 60_000_000;
        let den = u128::from(bpm.0) * u128::from(PPQN);
        let bits = num.div_ceil(den);
        bits.min(i64::MAX as u128) as i64
    }

    fn exact_whole(tick: Tick, bpm: Tempo, sr: u32) -> u64 {
        let bits = exact_bits(tick, bpm, sr);
        let whole = bits.div_euclid(1 << 16) + i64::from(bits.rem_euclid(1 << 16) != 0);
        I064U064.ceil(whole)
    }

    macro_rules! props_for_rate {
        ($mod_name:ident, $to_rate:ident, $Rate:ident, $Whole:ident, $sr:expr) => {
            mod $mod_name {
                use super::*;

                proptest! {
                    #[test]
                    fn zero_maps_to_zero(raw_bpm in 1_u32..=u32::MAX) {
                        prop_assert_eq!($to_rate(Tick(0), Tempo(raw_bpm)).to_bits(), 0);
                    }

                    #[test]
                    fn monotone_in_tick(a in any::<u64>(), b in any::<u64>(), raw_bpm in 1_u32..=u32::MAX) {
                        let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
                        let bpm = Tempo(raw_bpm);
                        prop_assert!($to_rate(Tick(lo), bpm).to_bits() <= $to_rate(Tick(hi), bpm).to_bits());
                    }

                    #[test]
                    fn antitone_in_tempo(tick in any::<u64>(), a in 1_u32..=u32::MAX, b in 1_u32..=u32::MAX) {
                        let (slow, fast) = if a <= b { (a, b) } else { (b, a) };
                        prop_assert!($to_rate(Tick(tick), Tempo(fast)).to_bits() <= $to_rate(Tick(tick), Tempo(slow)).to_bits());
                    }

                    #[test]
                    fn exact_formula_bounds(tick in any::<u64>(), raw_bpm in 1_u32..=u32::MAX) {
                        let bpm = Tempo(raw_bpm);
                        prop_assert_eq!($to_rate(Tick(tick), bpm).to_bits(), exact_bits(Tick(tick), bpm, $sr));
                    }
                }

                #[test]
                fn saturates_high_tick_low_bpm() {
                    assert_eq!($to_rate(Tick(u64::MAX), Tempo(1)).to_bits(), i64::MAX);
                }
            }
        };
    }

    props_for_rate!(s044, tick_to_s044, S044, S044I064, 44_100);
    props_for_rate!(s048, tick_to_s048, S048, S048I064, 48_000);
    props_for_rate!(s088, tick_to_s088, S088, S088I064, 88_200);
    props_for_rate!(s096, tick_to_s096, S096, S096I064, 96_000);
    props_for_rate!(s176, tick_to_s176, S176, S176I064, 176_400);
    props_for_rate!(s192, tick_to_s192, S192, S192I064, 192_000);

    #[test]
    fn one_beat_known_values() {
        let bpm = Tempo::from_bpm_integer(120);
        assert_eq!(S048I064.ceil(tick_to_s048(Tick(PPQN.into()), bpm)), 24_000);
        assert_eq!(S096I064.ceil(tick_to_s096(Tick(PPQN.into()), bpm)), 48_000);
        assert_eq!(S192I064.ceil(tick_to_s192(Tick(PPQN.into()), bpm)), 96_000);
    }

    #[test]
    fn non_exact_retains_fraction() {
        let s = tick_to_s048(Tick(1), Tempo::from_bpm_integer(137));
        assert_ne!(s.to_bits().rem_euclid(1 << 16), 0);
    }

    #[test]
    fn whole_sample_dispatch_matches_static_arm() {
        let tick = Tick(97);
        let bpm = Tempo::from_bpm_integer(137);
        assert_eq!(
            tick_to_whole_samples(tick, bpm, 44_100),
            Some(I064U064.ceil(S044I064.ceil(tick_to_s044(tick, bpm))))
        );
        assert_eq!(
            tick_to_whole_samples(tick, bpm, 48_000),
            Some(I064U064.ceil(S048I064.ceil(tick_to_s048(tick, bpm))))
        );
        assert_eq!(
            tick_to_whole_samples(tick, bpm, 88_200),
            Some(I064U064.ceil(S088I064.ceil(tick_to_s088(tick, bpm))))
        );
        assert_eq!(
            tick_to_whole_samples(tick, bpm, 96_000),
            Some(I064U064.ceil(S096I064.ceil(tick_to_s096(tick, bpm))))
        );
        assert_eq!(
            tick_to_whole_samples(tick, bpm, 176_400),
            Some(I064U064.ceil(S176I064.ceil(tick_to_s176(tick, bpm))))
        );
        assert_eq!(
            tick_to_whole_samples(tick, bpm, 192_000),
            Some(I064U064.ceil(S192I064.ceil(tick_to_s192(tick, bpm))))
        );
    }

    #[test]
    fn whole_sample_dispatch_rejects_unsupported_sr() {
        assert_eq!(
            tick_to_whole_samples(Tick(0), Tempo::from_bpm_integer(120), 22_050),
            None
        );
        assert_eq!(
            sample_to_tick_floor(0, Tempo::from_bpm_integer(120), 22_050),
            None
        );
        assert_eq!(
            sample_to_tick_ceil(0, Tempo::from_bpm_integer(120), 22_050),
            None
        );
    }

    proptest! {
        #[test]
        fn whole_samples_match_exact_formula(
            tick in any::<u64>(),
            raw_bpm in 1_u32..=u32::MAX,
            sr in prop::sample::select(vec![44_100_u32, 48_000, 88_200, 96_000, 176_400, 192_000]),
        ) {
            let bpm = Tempo(raw_bpm);
            prop_assert_eq!(tick_to_whole_samples(Tick(tick), bpm, sr), Some(exact_whole(Tick(tick), bpm, sr)));
        }

        #[test]
        fn whole_samples_monotone_tick(
            a in any::<u64>(),
            b in any::<u64>(),
            raw_bpm in 1_u32..=u32::MAX,
            sr in prop::sample::select(vec![44_100_u32, 48_000, 88_200, 96_000, 176_400, 192_000]),
        ) {
            let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
            let bpm = Tempo(raw_bpm);
            prop_assert!(tick_to_whole_samples(Tick(lo), bpm, sr).unwrap() <= tick_to_whole_samples(Tick(hi), bpm, sr).unwrap());
        }

        #[test]
        fn whole_samples_antitone_tempo(
            tick in any::<u64>(),
            a in 1_u32..=u32::MAX,
            b in 1_u32..=u32::MAX,
            sr in prop::sample::select(vec![44_100_u32, 48_000, 88_200, 96_000, 176_400, 192_000]),
        ) {
            let (slow, fast) = if a <= b { (a, b) } else { (b, a) };
            prop_assert!(tick_to_whole_samples(Tick(tick), Tempo(fast), sr).unwrap() <= tick_to_whole_samples(Tick(tick), Tempo(slow), sr).unwrap());
        }
    }
}
