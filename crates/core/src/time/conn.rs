//! Galois connections for `Tick`, `Time`, `Rational`, and `TBase`.
//!
//! Five `Conn<A, B>` values port the Haskell Cirklon connections:
//!
//! | Rust              | Haskell      | Shape                        |
//! |-------------------|--------------|------------------------------|
//! | [`ticks`]         | `ticks`      | `Conn<Tick, Time>`           |
//! | [`rat_tick`]      | `ratTick`    | `Conn<Whole, Tick>`          |
//! | [`quantize_at`]   | `quantizeAt` | `Conn<Tick, Time>` per TBase |
//! | [`time`]          | `time`       | `Conn<(Time, Time), Time>`   |
//! | [`tbase`]         | `tbase`      | `Conn<(TBase, TBase), TBase>`|
//!
//! All use bare `fn` pointers from [`connections::conn::Conn`] — no
//! closure capture, tempo-independent. `Conn::new` isn't `const fn`
//! upstream, so each accessor returns a freshly-built `Conn` (still
//! cheap: three `fn` pointers).
//!
//! **Orientation of `time` and `tbase`.** These are lattice
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

use crate::time::tbase::{self, TBase};
use crate::time::tick::{Tick, Time, from_ticks, from_ticks_floor, time_to_tick};

/// A rational whole-note duration. `Whole::new(1, 4)` = quarter note.
pub type Whole = Rational64;

/// Ticks per whole note at 192 PPQN. `4 * PPQN`.
const TPW: i64 = 768;

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

/// Master `Tick ↔ Time` connection. Ceiling rounds up to the T128t
/// grid then canonicalises; floor rounds down; embed is exact.
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
/// `rt_inner(Tick(n)) = n / 768`.
pub fn rat_tick() -> Conn<Whole, Tick> {
    Conn::new(rt_ceil, rt_inner, rt_floor)
}

// ── quantize_at: Conn<Tick, Time> per TBase ──────────────────────

fn qa_inner(t: Time) -> Tick {
    time_to_tick(t)
}

macro_rules! qa_variant {
    ($variant:ident, $ceil:ident, $floor:ident) => {
        fn $ceil(n: Tick) -> Time {
            Time {
                beats: n.0.div_ceil(TBase::$variant.tick_count()),
                base: TBase::$variant,
            }
        }
        fn $floor(n: Tick) -> Time {
            Time {
                beats: n.0 / TBase::$variant.tick_count(),
                base: TBase::$variant,
            }
        }
    };
}

qa_variant!(T1, qa_t1_ceil, qa_t1_floor);
qa_variant!(T2, qa_t2_ceil, qa_t2_floor);
qa_variant!(T4, qa_t4_ceil, qa_t4_floor);
qa_variant!(T8, qa_t8_ceil, qa_t8_floor);
qa_variant!(T16, qa_t16_ceil, qa_t16_floor);
qa_variant!(T32, qa_t32_ceil, qa_t32_floor);
qa_variant!(T64, qa_t64_ceil, qa_t64_floor);
qa_variant!(T2t, qa_t2t_ceil, qa_t2t_floor);
qa_variant!(T4t, qa_t4t_ceil, qa_t4t_floor);
qa_variant!(T8t, qa_t8t_ceil, qa_t8t_floor);
qa_variant!(T16t, qa_t16t_ceil, qa_t16t_floor);
qa_variant!(T32t, qa_t32t_ceil, qa_t32t_floor);
qa_variant!(T64t, qa_t64t_ceil, qa_t64t_floor);
qa_variant!(T128t, qa_t128t_ceil, qa_t128t_floor);

/// Quantise a `Tick` to the nearest `Time` on the `tb` grid, keeping
/// the result on that grid (no further nicest-coarsening, unlike
/// [`ticks`]). `fn` pointers can't close over `tb`, so dispatch is a
/// per-variant `match`.
pub fn quantize_at(tb: TBase) -> Conn<Tick, Time> {
    match tb {
        TBase::T1 => Conn::new(qa_t1_ceil, qa_inner, qa_t1_floor),
        TBase::T2 => Conn::new(qa_t2_ceil, qa_inner, qa_t2_floor),
        TBase::T4 => Conn::new(qa_t4_ceil, qa_inner, qa_t4_floor),
        TBase::T8 => Conn::new(qa_t8_ceil, qa_inner, qa_t8_floor),
        TBase::T16 => Conn::new(qa_t16_ceil, qa_inner, qa_t16_floor),
        TBase::T32 => Conn::new(qa_t32_ceil, qa_inner, qa_t32_floor),
        TBase::T64 => Conn::new(qa_t64_ceil, qa_inner, qa_t64_floor),
        TBase::T2t => Conn::new(qa_t2t_ceil, qa_inner, qa_t2t_floor),
        TBase::T4t => Conn::new(qa_t4t_ceil, qa_inner, qa_t4t_floor),
        TBase::T8t => Conn::new(qa_t8t_ceil, qa_inner, qa_t8t_floor),
        TBase::T16t => Conn::new(qa_t16t_ceil, qa_inner, qa_t16t_floor),
        TBase::T32t => Conn::new(qa_t32t_ceil, qa_inner, qa_t32t_floor),
        TBase::T64t => Conn::new(qa_t64t_ceil, qa_inner, qa_t64t_floor),
        TBase::T128t => Conn::new(qa_t128t_ceil, qa_inner, qa_t128t_floor),
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
/// unreachable; tests use `arb_small_time` (tick counts ≤ 38_400,
/// LCM well inside `u32`) to stay safely bounded.
pub fn time() -> Conn<(Time, Time), Time> {
    Conn::new(time_pair_ceil, time_pair_inner, time_pair_floor)
}

// ── tbase: Conn<(TBase, TBase), TBase> ───────────────────────────

fn tbase_pair_ceil(ab: (TBase, TBase)) -> TBase {
    let (a, b) = ab;
    tbase::meet(a, b)
}

fn tbase_pair_inner(t: TBase) -> (TBase, TBase) {
    (t, t)
}

fn tbase_pair_floor(ab: (TBase, TBase)) -> TBase {
    let (a, b) = ab;
    tbase::join(a, b)
}

/// Divisibility-lattice connection on `TBase`. `ceil = meet (GCD of
/// tick counts)`, `floor = join (LCM)`, `inner = diagonal`.
pub fn tbase() -> Conn<(TBase, TBase), TBase> {
    Conn::new(tbase_pair_ceil, tbase_pair_inner, tbase_pair_floor)
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
    /// by `bpm_µ × ppqn` (e.g. 48 kHz / 120 BPM / 192 PPQN is exact);
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
    use crate::arb::{arb_rational_nonneg, arb_small_time, arb_tbase, arb_tick, arb_time};
    use connections::order::Ple;
    use proptest::prelude::*;

    // ── Spot checks ──────────────────────────────────────────────

    #[test]
    fn ticks_inner_is_exact() {
        let c = ticks();
        let t = Time {
            beats: 3,
            base: TBase::T16,
        };
        assert_eq!(c.inner(t), Tick(144));
    }

    #[test]
    fn ticks_ceil_aligned() {
        let c = ticks();
        assert_eq!(
            c.ceil(Tick(48)),
            Time {
                beats: 1,
                base: TBase::T16
            }
        );
    }

    #[test]
    fn ticks_floor_unaligned() {
        let c = ticks();
        // 50 → round down to 48 → T16
        assert_eq!(
            c.floor(Tick(50)),
            Time {
                beats: 1,
                base: TBase::T16
            }
        );
    }

    #[test]
    fn ticks_ceil_unaligned() {
        let c = ticks();
        // 50 → round up to 52 → T128t
        assert_eq!(
            c.ceil(Tick(50)),
            Time {
                beats: 13,
                base: TBase::T128t
            }
        );
    }

    #[test]
    fn rat_tick_quarter_is_192() {
        let c = rat_tick();
        assert_eq!(c.floor(Rational64::new(1, 4)), Tick(192));
        assert_eq!(c.ceil(Rational64::new(1, 4)), Tick(192));
    }

    #[test]
    fn rat_tick_three_sixteenths_is_144() {
        let c = rat_tick();
        assert_eq!(c.floor(Rational64::new(3, 16)), Tick(144));
    }

    #[test]
    fn rat_tick_one_seventh_ceils_to_110() {
        // 768 / 7 = 109.71…, ceil = 110.
        let c = rat_tick();
        assert_eq!(c.ceil(Rational64::new(1, 7)), Tick(110));
        assert_eq!(c.floor(Rational64::new(1, 7)), Tick(109));
    }

    #[test]
    fn quantize_at_t16_aligned() {
        let c = quantize_at(TBase::T16);
        assert_eq!(
            c.floor(Tick(48)),
            Time {
                beats: 1,
                base: TBase::T16
            }
        );
        assert_eq!(
            c.ceil(Tick(48)),
            Time {
                beats: 1,
                base: TBase::T16
            }
        );
    }

    #[test]
    fn quantize_at_t16_unaligned_splits_on_grid() {
        let c = quantize_at(TBase::T16);
        // 50: floor to grid = 48/48 = 1 → Time 1 T16; ceil → Time 2 T16.
        assert_eq!(
            c.floor(Tick(50)),
            Time {
                beats: 1,
                base: TBase::T16
            }
        );
        assert_eq!(
            c.ceil(Tick(50)),
            Time {
                beats: 2,
                base: TBase::T16
            }
        );
    }

    #[test]
    fn time_ceil_gcd_of_n4_n8() {
        let c = time();
        let a = Time {
            beats: 1,
            base: TBase::T4,
        }; // 192 ticks
        let b = Time {
            beats: 1,
            base: TBase::T8,
        }; // 96 ticks
        // gcd(192, 96) = 96 → Time 1 T8
        assert_eq!(
            c.ceil((a, b)),
            Time {
                beats: 1,
                base: TBase::T8
            }
        );
    }

    #[test]
    fn time_floor_lcm_of_n16_t16() {
        let c = time();
        let a = Time {
            beats: 1,
            base: TBase::T16,
        }; // 48 ticks
        let b = Time {
            beats: 1,
            base: TBase::T16t,
        }; // 32 ticks
        // lcm(48, 32) = 96 → Time 1 T8
        assert_eq!(
            c.floor((a, b)),
            Time {
                beats: 1,
                base: TBase::T8
            }
        );
    }

    #[test]
    fn tbase_ceil_meet_of_t4_t8() {
        let c = tbase();
        assert_eq!(c.ceil((TBase::T4, TBase::T8)), TBase::T8);
    }

    #[test]
    fn tbase_floor_join_of_t4_t8t() {
        let c = tbase();
        // lcm(192, 64) = 192 = T4
        assert_eq!(c.floor((TBase::T4, TBase::T8t)), TBase::T4);
    }

    // ── Generic connections-tests laws for magnitude connections ──
    //
    // For `ticks`, `rat_tick`, `quantize_at` the input/output types
    // carry their natural magnitude preorder (tick count for Tick and
    // Time, standard order for Rational). The five laws from
    // connections/src/conn.rs apply directly.

    proptest! {
        // ── ticks ────────────────────────────────────────────────

        /// Adjointness for `ceil ⊣ inner`.
        #[test]
        fn ticks_adjoint(a in arb_tick(), b in arb_time()) {
            let c = ticks();
            let lhs = c.ceil(a).ple(&b);
            let rhs = a.ple(&c.inner(b));
            prop_assert_eq!(lhs, rhs);
        }

        /// Closure: `a ≤ inner(ceil(a))`.
        #[test]
        fn ticks_closed(a in arb_tick()) {
            let c = ticks();
            prop_assert!(a.ple(&c.inner(c.ceil(a))));
        }

        /// Kernel: `ceil(inner(b)) ≤ b`.
        #[test]
        fn ticks_kernel(b in arb_time()) {
            let c = ticks();
            prop_assert!(c.ceil(c.inner(b)).ple(&b));
        }

        /// Monotonicity of ceil and inner.
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

        /// Idempotence: `inner(ceil(inner(ceil(a)))) == inner(ceil(a))`.
        #[test]
        fn ticks_idempotent(a in arb_tick()) {
            let c = ticks();
            let once = c.inner(c.ceil(a));
            let twice = c.inner(c.ceil(once));
            prop_assert_eq!(once, twice);
        }

        /// Plan property `ticks_round_trip`: for grid-aligned ticks,
        /// `inner(floor(t)) = t`.
        #[test]
        fn ticks_round_trip_on_aligned(q in 0u32..=1_000_000) {
            let c = ticks();
            let n = Tick(q * TBase::T128t.tick_count());
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

        /// Plan property `rat_tick_monotone`: `a ≤ b` in `Rational`
        /// ⟹ `floor(a) ≤ floor(b)` in `Tick`.
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

        /// Plan property `quantize_at_galois`: `floor(x) ≤ x ≤ ceil(x)`
        /// under the `ticks` magnitude order, and `inner ∘ floor =
        /// inner ∘ ceil = id` on grid-aligned ticks.
        #[test]
        fn quantize_at_brackets_input(
            tb in arb_tbase(), n in arb_tick(),
        ) {
            let c = quantize_at(tb);
            let lo = time_to_tick(c.floor(n));
            let hi = time_to_tick(c.ceil(n));
            prop_assert!(lo.ple(&n));
            prop_assert!(n.ple(&hi));
        }

        #[test]
        fn quantize_at_aligned_inner_round_trip(
            tb in arb_tbase(), q in 0u32..=10_000,
        ) {
            let c = quantize_at(tb);
            let n = Tick(q * tb.tick_count());
            prop_assert_eq!(c.inner(c.floor(n)), n);
            prop_assert_eq!(c.inner(c.ceil(n)), n);
        }

        /// Adjointness for `quantize_at` holds only when `t` is on the
        /// `tb` grid. Off-grid `Time` values break the kernel law
        /// (`ceil(inner(t)) ≤ t`) because `inner` can yield a tick
        /// count that `ceil` then rounds up past `t`. `Time` values
        /// constructed through this Conn's `ceil`/`floor` are always
        /// on-grid, so the restricted adjoint is the operative fact.
        #[test]
        fn quantize_at_adjoint(
            tb in arb_tbase(), n in arb_tick(), k in 0u32..=10_000,
        ) {
            let c = quantize_at(tb);
            let t = Time { beats: k, base: tb };
            let lhs = c.ceil(n).ple(&t);
            let rhs = n.ple(&c.inner(t));
            prop_assert_eq!(lhs, rhs);
        }

        #[test]
        fn quantize_at_closed(tb in arb_tbase(), n in arb_tick()) {
            let c = quantize_at(tb);
            prop_assert!(n.ple(&c.inner(c.ceil(n))));
        }

        /// Kernel law restricted to grid-aligned `Time` values (see
        /// `quantize_at_adjoint` for why).
        #[test]
        fn quantize_at_kernel(tb in arb_tbase(), k in 0u32..=10_000) {
            let c = quantize_at(tb);
            let t = Time { beats: k, base: tb };
            prop_assert!(c.ceil(c.inner(t)).ple(&t));
        }

        #[test]
        fn quantize_at_monotonic(
            tb in arb_tbase(),
            a1 in arb_tick(), a2 in arb_tick(),
        ) {
            let c = quantize_at(tb);
            if a1.ple(&a2) {
                prop_assert!(c.ceil(a1).ple(&c.ceil(a2)));
                prop_assert!(c.floor(a1).ple(&c.floor(a2)));
            }
        }

        #[test]
        fn quantize_at_idempotent(tb in arb_tbase(), n in arb_tick()) {
            let c = quantize_at(tb);
            let once = c.inner(c.ceil(n));
            let twice = c.inner(c.ceil(once));
            prop_assert_eq!(once, twice);
        }
    }

    // ── Lattice-connection laws for `time` and `tbase` ──────────
    //
    // The adjoint structure `meet ⊣ diag ⊣ join` holds under the
    // "refine-to" order: `a ≤ b ⟺ tc(b) divides tc(a)` (i.e. "b is at
    // least as fine as a"). Under this ordering the coarsest element
    // (T1 / `Time{_, T1}`) is bottom and the finest (T128t) is top,
    // `ceil = meet = GCD` is the left adjoint of the diagonal, and
    // `floor = join = LCM` is the right adjoint.
    //
    // Standard divisibility order (our `Ple` impl) orients the other
    // way around and would give a non-adjoint structure here, so we
    // use ad-hoc `refine_le` helpers.

    fn tbase_refine_le(a: TBase, b: TBase) -> bool {
        a.tick_count() % b.tick_count() == 0
    }

    fn time_refine_le(a: Time, b: Time) -> bool {
        let ta = time_to_tick(a).0;
        let tb = time_to_tick(b).0;
        if tb == 0 { ta == 0 } else { ta % tb == 0 }
    }

    proptest! {
        // ── tbase connection ─────────────────────────────────────

        /// `ceil((a, b)) = meet(a, b)` (GCD on tick counts).
        #[test]
        fn tbase_ceil_is_meet(a in arb_tbase(), b in arb_tbase()) {
            let c = tbase();
            prop_assert_eq!(c.ceil((a, b)), tbase::meet(a, b));
        }

        /// `floor((a, b)) = join(a, b)` (LCM on tick counts).
        #[test]
        fn tbase_floor_is_join(a in arb_tbase(), b in arb_tbase()) {
            let c = tbase();
            prop_assert_eq!(c.floor((a, b)), tbase::join(a, b));
        }

        /// `inner(t) = (t, t)` — diagonal.
        #[test]
        fn tbase_inner_is_diagonal(t in arb_tbase()) {
            let c = tbase();
            prop_assert_eq!(c.inner(t), (t, t));
        }

        /// Adjointness `ceil ⊣ inner` under divisibility. Left adjoint
        /// here is meet (the right adjoint in magnitude order), because
        /// the lattice order has GCD below both operands.
        #[test]
        fn tbase_adjoint(
            a in arb_tbase(), b in arb_tbase(), z in arb_tbase(),
        ) {
            let c = tbase();
            // ceil((a, b)) ≤ z ⟺ (a, b) ≤ inner(z) = (z, z)
            let lhs = tbase_refine_le(c.ceil((a, b)), z);
            let rhs = tbase_refine_le(a, z) && tbase_refine_le(b, z);
            prop_assert_eq!(lhs, rhs);
        }

        /// Closure: `(a, b) ≤ inner(ceil((a, b)))` under divisibility.
        #[test]
        fn tbase_closed(a in arb_tbase(), b in arb_tbase()) {
            let c = tbase();
            let (x, y) = c.inner(c.ceil((a, b)));
            prop_assert!(tbase_refine_le(a, x));
            prop_assert!(tbase_refine_le(b, y));
        }

        /// Kernel: `ceil(inner(z)) ≤ z`.
        #[test]
        fn tbase_kernel(z in arb_tbase()) {
            let c = tbase();
            prop_assert!(tbase_refine_le(c.ceil(c.inner(z)), z));
        }

        /// Monotonicity under divisibility.
        #[test]
        fn tbase_monotonic(
            a1 in arb_tbase(), a2 in arb_tbase(),
            b1 in arb_tbase(), b2 in arb_tbase(),
            z1 in arb_tbase(), z2 in arb_tbase(),
        ) {
            let c = tbase();
            if tbase_refine_le(a1, a2) && tbase_refine_le(b1, b2) {
                prop_assert!(tbase_refine_le(c.ceil((a1, b1)), c.ceil((a2, b2))));
            }
            if tbase_refine_le(z1, z2) {
                let (x1, y1) = c.inner(z1);
                let (x2, y2) = c.inner(z2);
                prop_assert!(tbase_refine_le(x1, x2));
                prop_assert!(tbase_refine_le(y1, y2));
            }
        }

        /// Idempotence.
        #[test]
        fn tbase_idempotent(a in arb_tbase(), b in arb_tbase()) {
            let c = tbase();
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

        /// Monotonicity under refine-to order, matching the pattern
        /// used for the other four connections. `ceil = GCD` is
        /// monotone because `gcd` is monotone in each argument under
        /// divisibility; `inner` (diagonal) inherits monotonicity
        /// component-wise from the input.
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
    //
    // Runtime-parameterised Sample ↔ Tick bridge. Laws mirror
    // `connections::Conn`'s `(ceil, inner, floor)` triple but are
    // asserted in-module because `SampleTickConn` is not a genuine
    // `Conn` (cannot capture runtime `(sr, bpm)`).

    fn mbpm(b: u32) -> crate::fxp::Tempo {
        crate::fxp::Tempo::from_bpm_integer(b)
    }

    #[test]
    fn sample_tick_inner_120bpm_48k_one_beat() {
        // 120 BPM, 192 PPQN, 48 kHz: one quarter note (tick 192) is
        // 0.5 s = 24 000 samples. Plan spot check.
        let stc = SampleTickConn::new(48_000, mbpm(120), 192);
        assert_eq!(stc.inner(Tick(192)), 24_000);
    }

    #[test]
    fn sample_tick_floor_and_ceil_bracket_inner() {
        let stc = SampleTickConn::new(48_000, mbpm(120), 192);
        // Halfway between tick 192 and 193: sample ≈ 24 062.5.
        // floor → 192, ceil → 193.
        let s = 24_062;
        assert_eq!(stc.floor(s), Tick(192));
        assert_eq!(stc.ceil(s), Tick(193));
    }

    #[test]
    fn sample_tick_zero_is_zero() {
        let stc = SampleTickConn::new(48_000, mbpm(120), 192);
        assert_eq!(stc.inner(Tick(0)), 0);
        assert_eq!(stc.floor(0), Tick(0));
        assert_eq!(stc.ceil(0), Tick(0));
    }

    /// Sample-rate / BPM / PPQN combinations that keep integer
    /// samples-per-tick exact (`sr · 60 · 10⁶` divisible by
    /// `bpm_µ · ppqn`), needed for the round-trip property.
    fn arb_integer_stc() -> impl Strategy<Value = SampleTickConn> {
        prop_oneof![
            Just(SampleTickConn::new(48_000, mbpm(120), 192)),
            Just(SampleTickConn::new(48_000, mbpm(60), 192)),
            Just(SampleTickConn::new(48_000, mbpm(240), 192)),
            Just(SampleTickConn::new(96_000, mbpm(120), 192)),
            Just(SampleTickConn::new(192_000, mbpm(120), 192)),
            Just(SampleTickConn::new(48_000, mbpm(120), 24)),
        ]
    }

    proptest! {
        /// Plan property `sample_tick_round_trip`:
        /// `floor(inner(t)) == t` for any tick in range. Holds exactly
        /// when `sr * 60` is divisible by `bpm * ppqn`; `arb_integer_stc`
        /// restricts to configurations where it is.
        #[test]
        fn sample_tick_round_trip(
            stc in arb_integer_stc(),
            t in 0u32..=1_000_000,
        ) {
            let tick = Tick(t);
            let sample = stc.inner(tick);
            prop_assert_eq!(stc.floor(sample), tick);
        }

        /// Plan property `sample_tick_monotonic`:
        /// `s1 ≤ s2 ⇒ floor(s1) ≤ floor(s2)`.
        #[test]
        fn sample_tick_monotonic(
            stc in arb_integer_stc(),
            s1 in 0u64..=10_000_000,
            s2 in 0u64..=10_000_000,
        ) {
            let (lo, hi) = if s1 <= s2 { (s1, s2) } else { (s2, s1) };
            prop_assert!(stc.floor(lo).0 <= stc.floor(hi).0);
        }

        /// Ceil is at-or-after floor for every sample.
        #[test]
        fn sample_tick_ceil_ge_floor(
            stc in arb_integer_stc(),
            s in 0u64..=10_000_000,
        ) {
            prop_assert!(stc.floor(s).0 <= stc.ceil(s).0);
        }
    }
}
