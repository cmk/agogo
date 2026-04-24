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

/// Pico ↔ Sample bridge parameterised by sample rate.
///
/// Mirrors upstream `connections::Conn<Pico, Sxx>`'s adjoint
/// triple but with a *runtime* rate. The sample side uses
/// `connections::sample::Q48_16` (= `FixedI64<U16>`) exactly like
/// `F12S48` / `F12S44` / etc. — this is what makes the bidirectional
/// Galois laws exact for every IEEE-reasonable rate (44.1 kHz
/// included), because the fractional sample type carries sub-sample
/// Pico precision that plain `i64` samples cannot.
///
/// - `ceil(p) ≤ s  ⟺  p ≤ inner(s)`
/// - `inner(s) ≤ p  ⟺  s ≤ floor(p)`
///
/// Not a real `Conn` because the conversion depends on runtime
/// `sr` — needs a closure-capturing `Conn` variant in the upstream
/// `connections` crate (tracked there as deferred work). Until that
/// lands, this stays as a Conn-lookalike in agogo-core.
///
/// `i128` intermediate arithmetic; no floating-point.
///
/// Integer sample callers extract via `.ceil().to_num::<i64>()`
/// (round up to the next whole sample) or `.floor().to_num::<i64>()`.
#[derive(Copy, Clone, Debug)]
pub struct PicoSampleConn {
    /// pico-per-bit ratio = 10¹² / (sr × 2¹⁶), reduced by gcd.
    num: i128,
    den: i128,
    sr: u32,
}

impl PicoSampleConn {
    /// # Panics
    ///
    /// Panics if `sr == 0` — it's always validated at the CLI
    /// / config boundary, and a zero rate is a programming error.
    pub fn new(sr: u32) -> Self {
        assert!(sr > 0, "sample rate must be positive");
        // 1 Q48.16 bit = 10¹² / (sr × 2¹⁶) pico.
        let num: i128 = 1_000_000_000_000;
        let den: i128 = i128::from(sr) * (1_i128 << 16);
        let g = gcd_i128(num, den);
        Self { num: num / g, den: den / g, sr }
    }

    pub fn sr(&self) -> u32 {
        self.sr
    }

    /// Sample (Q48.16) → Pico, flooring the exact product to integer
    /// picoseconds. Matches upstream `F12SXX`'s `inner` direction.
    ///
    /// Saturates to `Pico(i64::MIN)` / `Pico(i64::MAX)` for Q48.16
    /// values whose pico representation exceeds `i64` range. At a
    /// realistic 44.1 kHz rate this is 2⁶³ × 10¹² / (44 100 × 2¹⁶) ≈
    /// 2⁴⁷ Q48.16 bits ≈ 2³¹ whole samples ≈ 13.6 hours of audio, so
    /// no realistic caller should saturate — but silently wrapping
    /// on out-of-range Q48.16 inputs would turn a contract violation
    /// into a quiet data-corruption bug, so we clamp explicitly.
    pub fn inner(&self, s: connections::sample::Q48_16) -> connections::fixed::Pico {
        let n: i128 = i128::from(s.to_bits()) * self.num;
        let clamped = n
            .div_euclid(self.den)
            .clamp(i128::from(i64::MIN), i128::from(i64::MAX));
        connections::fixed::Pico(clamped as i64)
    }

    /// Pico → Sample (Q48.16), rounding up: smallest `s` with
    /// `inner(s) ≥ p`.
    pub fn ceil(&self, p: connections::fixed::Pico) -> connections::sample::Q48_16 {
        let n: i128 = i128::from(p.0) * self.den;
        let q = n.div_euclid(self.num);
        let r = n.rem_euclid(self.num);
        let bits = if r != 0 { q + 1 } else { q };
        connections::sample::Q48_16::from_bits(bits as i64)
    }

    /// Pico → Sample (Q48.16): the Galois right-adjoint of `inner`
    /// — largest `s` with `inner(s) ≤ p`. For positive `p` this
    /// agrees with the mathematical floor of `p × DEN / NUM`; for
    /// negative `p` the Galois formula
    /// `floor_div(p × DEN + DEN − 1, NUM)` differs from the naïve
    /// floor by at most one ULP, and IS what the adjoint laws
    /// require. Mirrors upstream `F12SXX::floor`.
    pub fn floor(&self, p: connections::fixed::Pico) -> connections::sample::Q48_16 {
        let n: i128 = i128::from(p.0) * self.den + (self.den - 1);
        connections::sample::Q48_16::from_bits(n.div_euclid(self.num) as i64)
    }
}

fn gcd_i128(mut a: i128, mut b: i128) -> i128 {
    // Inputs are `num = 10¹²` and `den = sr × 2¹⁶` with `sr ≤ 2³²`,
    // so both fit in ~50 bits — `unsigned_abs() as i128` can't wrap.
    a = a.unsigned_abs() as i128;
    b = b.unsigned_abs() as i128;
    while b != 0 {
        (a, b) = (b, a.rem_euclid(b));
    }
    a
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

    // ── PicoSampleConn ───────────────────────────────────────────
    //
    // Runtime-parameterised Pico ↔ Sample bridge (Q48.16 samples).
    // Laws mirror upstream `connections::Conn<Pico, Sxx>` exactly.

    use connections::fixed::Pico;
    use connections::sample::Q48_16;

    /// All standard audio rates. Q48.16 carries sub-sample pico
    /// precision, so the adjoint laws are exact at every rate —
    /// including the 44.1 kHz family, which is the only one where
    /// `10¹²` is not divisible by `sr` (so NUM / DEN don't reduce
    /// to trivial powers of 2). Bias the strategy toward the 44.1
    /// kHz family since that's where an off-by-one bug would
    /// surface first.
    fn arb_pico_sample_conn() -> impl Strategy<Value = PicoSampleConn> {
        prop_oneof![
            3 => Just(PicoSampleConn::new(44_100)),
            3 => Just(PicoSampleConn::new(88_200)),
            3 => Just(PicoSampleConn::new(176_400)),
            1 => Just(PicoSampleConn::new(48_000)),
            1 => Just(PicoSampleConn::new(96_000)),
            1 => Just(PicoSampleConn::new(192_000)),
        ]
    }

    /// Full `i64` Pico range. All intermediate arithmetic is `i128`
    /// and stays comfortably inside `i128::MAX` even at `Pico(i64::MAX)`
    /// times the largest post-gcd `den` (~1.26×10¹⁰), so there's no
    /// reason to bound the generator — doing so would fake coverage
    /// by hiding the exact region where saturation / wrap bugs live
    /// (per CLAUDE.md proptest convention).
    fn arb_pico() -> impl Strategy<Value = Pico> {
        prop_oneof![
            1 => Just(Pico(0)),
            1 => Just(Pico(i64::MIN)),
            1 => Just(Pico(i64::MAX)),
            6 => any::<i64>().prop_map(Pico),
        ]
    }

    /// Bounded Q48.16 for the strict round-trip identity tests —
    /// `floor(inner(s)) == s` and `ceil(inner(s)) == s` only hold
    /// inside the non-saturating region. The saturation *behaviour*
    /// is covered by `pico_sample_inner_saturates_at_i64_boundaries`;
    /// this generator keeps the round-trip proptest on its valid
    /// domain.
    fn arb_q48_16_non_saturating() -> impl Strategy<Value = Q48_16> {
        // At 44.1 kHz, num = 9_765_625 / den = 28_224; saturation
        // kicks in at roughly |bits| × (num/den) > i64::MAX, i.e.
        // |bits| > i64::MAX × 28_224 / 9_765_625 ≈ 2.66×10¹⁶. Bound
        // to ±10¹⁵ leaves a comfortable margin.
        (-1_000_000_000_000_000_i64..=1_000_000_000_000_000).prop_map(Q48_16::from_bits)
    }

    #[test]
    fn pico_sample_48k_exact_boundary() {
        // 1 second = 10¹² pico = 48 000 samples at 48 kHz.
        let psc = PicoSampleConn::new(48_000);
        let one_sec = Pico(1_000_000_000_000);
        let s48k = Q48_16::from_num(48_000);
        assert_eq!(psc.ceil(one_sec), s48k);
        assert_eq!(psc.floor(one_sec), s48k);
        assert_eq!(psc.inner(s48k), one_sec);
    }

    #[test]
    fn pico_sample_48k_half_sample_brackets() {
        // Half a sample at 48 kHz: 10¹² / 96000 ≈ 10_416_666.67 pico.
        // Below the exact half: floor = 0 whole samples (0 Q48.16 bits
        // for integer part), ceil = 1 sample (2¹⁶ bits).
        let psc = PicoSampleConn::new(48_000);
        let one_sample = Q48_16::from_num(1);
        // Pico(20_833_333) < exact 1-sample boundary → ceil is still 1
        // sample (since some positive fraction means we must round up
        // to at least 1 sample's worth of bits).
        let nearly_one_sample = Pico(20_833_332);
        assert_eq!(psc.ceil(nearly_one_sample), one_sample);
        // And floor just below the boundary gives the fractional value
        // strictly less than 1 sample, not 0 — because Q48.16 carries
        // the fraction exactly.
        let floored = psc.floor(nearly_one_sample);
        assert!(floored < one_sample);
        assert!(floored.to_bits() > 0);
    }

    #[test]
    fn pico_sample_negative_offsets() {
        let psc = PicoSampleConn::new(48_000);

        // Exactly −1 second: −48 000 samples.
        let neg_one_sec = Pico(-1_000_000_000_000);
        let neg_48k = Q48_16::from_num(-48_000);
        assert_eq!(psc.floor(neg_one_sec), neg_48k);
        assert_eq!(psc.ceil(neg_one_sec), neg_48k);
        assert_eq!(psc.inner(neg_48k), neg_one_sec);

        // −500 ms = −24 000 samples.
        let neg_half_sec = Pico(-500_000_000_000);
        let neg_24k = Q48_16::from_num(-24_000);
        assert_eq!(psc.floor(neg_half_sec), neg_24k);
        assert_eq!(psc.ceil(neg_half_sec), neg_24k);
    }

    #[test]
    fn pico_sample_44100_exact_on_bit_grid() {
        // 1 Q48.16 bit at 44.1 kHz = 10¹² / (44_100 × 2¹⁶) pico.
        // Reduce: num=10¹² gcd den = 2² × 5² ⇒ num/g = 10¹² / 100,
        // den/g = 44_100 × 2¹⁶ / 100. Bit-grid round-trips are exact.
        let psc = PicoSampleConn::new(44_100);
        let one_sec = Pico(1_000_000_000_000);
        let s44k = Q48_16::from_num(44_100);
        assert_eq!(psc.ceil(one_sec), s44k);
        assert_eq!(psc.floor(one_sec), s44k);
        assert_eq!(psc.inner(s44k), one_sec);
    }

    proptest! {
        /// Galois adjoint upper: `ceil(p) ≤ s ⟺ p ≤ inner(s)`.
        ///
        /// Bounded s to the non-saturating range because `inner`
        /// intentionally clamps at the i64 Pico boundary — many
        /// distinct s values near ±i64::MAX all map to the same
        /// `Pico(i64::MIN/MAX)`, flattening the law at that edge.
        /// That flattening is a designed behaviour (covered by
        /// `pico_sample_inner_saturates_at_i64_boundaries`), not a
        /// bug the law should catch. `p` stays at full i64 because
        /// `ceil`/`floor` don't saturate on the Pico side.
        #[test]
        fn pico_sample_adjoint_upper(
            psc in arb_pico_sample_conn(),
            p in arb_pico(),
            s in arb_q48_16_non_saturating(),
        ) {
            prop_assert_eq!(psc.ceil(p) <= s, p.0 <= psc.inner(s).0);
        }

        /// Galois adjoint lower: `inner(s) ≤ p ⟺ s ≤ floor(p)`.
        /// Same saturation caveat as `pico_sample_adjoint_upper`.
        #[test]
        fn pico_sample_adjoint_lower(
            psc in arb_pico_sample_conn(),
            p in arb_pico(),
            s in arb_q48_16_non_saturating(),
        ) {
            prop_assert_eq!(psc.inner(s).0 <= p.0, s <= psc.floor(p));
        }

        /// Monotone ceil.
        #[test]
        fn pico_sample_monotone_ceil(
            psc in arb_pico_sample_conn(),
            p1 in arb_pico(),
            p2 in arb_pico(),
        ) {
            let (lo, hi) = if p1.0 <= p2.0 { (p1, p2) } else { (p2, p1) };
            prop_assert!(psc.ceil(lo) <= psc.ceil(hi));
        }

        /// Monotone floor.
        #[test]
        fn pico_sample_monotone_floor(
            psc in arb_pico_sample_conn(),
            p1 in arb_pico(),
            p2 in arb_pico(),
        ) {
            let (lo, hi) = if p1.0 <= p2.0 { (p1, p2) } else { (p2, p1) };
            prop_assert!(psc.floor(lo) <= psc.floor(hi));
        }

        /// Floor ≤ ceil, differing by at most 1 Q48.16 bit.
        #[test]
        fn pico_sample_floor_le_ceil(
            psc in arb_pico_sample_conn(),
            p in arb_pico(),
        ) {
            let f = psc.floor(p);
            let c = psc.ceil(p);
            prop_assert!(f <= c);
            prop_assert!(c.to_bits() - f.to_bits() <= 1);
        }

        /// `floor(inner(s)) == s` — inner lands on the exact pico
        /// for integer-bit Q48.16 samples, floor is the left inverse.
        /// Bounded to the non-saturating domain; the saturation
        /// behaviour is exercised by
        /// `pico_sample_inner_saturates_at_i64_boundaries`.
        #[test]
        fn pico_sample_inner_round_trip_floor(
            psc in arb_pico_sample_conn(),
            s in arb_q48_16_non_saturating(),
        ) {
            prop_assert_eq!(psc.floor(psc.inner(s)), s);
        }

        /// `ceil(inner(s)) == s` symmetrically — inner is exact in
        /// the other direction too.
        #[test]
        fn pico_sample_inner_round_trip_ceil(
            psc in arb_pico_sample_conn(),
            s in arb_q48_16_non_saturating(),
        ) {
            prop_assert_eq!(psc.ceil(psc.inner(s)), s);
        }
    }

    // Triangle property: `tick → sample` via SampleTickConn agrees
    // with `tick → pico → sample` via PicoSampleConn, at a
    // compile-time-pinned (bpm, ppq, sr) where the arithmetic is
    // exact. Demonstrates that the two runtime Conn-lookalikes
    // describe the same sample-time geometry.
    //
    // Stays as a hand-computed spot check rather than a full
    // proptest because "tick → pico" needs `bpm` and `ppq`, which
    // `PicoSampleConn` doesn't capture — a full proptest would
    // require wrapping the two conns in a combined bridge that
    // agogo doesn't have yet and doesn't need outside this test.
    // The three arithmetic-boundary tests below exist because an
    // earlier revision of `PicoSampleConn::inner` did `as i64` on the
    // i128 intermediate, which silently wrapped for Q48.16 bits in
    // roughly ±2⁴⁸..2⁶³. The `arb_q48_16` proptest generator bounded
    // inputs to ±10¹⁵ bits — under the wrap threshold — so the main
    // Galois-law battery never saw the bug. Expanding the generator
    // here to hit the full i64 range is the test that should have
    // caught it.

    // `pico_sample_inner_monotone_full_i64`: `inner` is monotone
    // non-decreasing across the full i64 bit range. With the old
    // `as i64` wrap, very large bits would silently flip sign and
    // break monotonicity; with the saturating clamp, monotonicity
    // is restored (with equality plateaus at the ±i64 boundaries).
    proptest! {
        #[test]
        fn pico_sample_inner_monotone_full_i64(
            psc in arb_pico_sample_conn(),
            a in any::<i64>(),
            b in any::<i64>(),
        ) {
            let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
            let inner_lo = psc.inner(Q48_16::from_bits(lo));
            let inner_hi = psc.inner(Q48_16::from_bits(hi));
            prop_assert!(inner_lo.0 <= inner_hi.0);
        }

        // `pico_sample_inner_adjacent_bits_differ_by_at_most_ratio`:
        // the semantic check for no-silent-wrap — adjacent-bit inputs
        // produce Pico values differing by at most `ceil(num/den)+1`.
        // A wrap would produce a jump of ~2⁶⁴; a saturation plateau
        // produces a step of 0.
        #[test]
        fn pico_sample_inner_adjacent_bits_differ_by_at_most_ratio(
            psc in arb_pico_sample_conn(),
            bits in (i64::MIN + 1)..=(i64::MAX - 1),
        ) {
            let p0 = psc.inner(Q48_16::from_bits(bits)).0;
            let p1 = psc.inner(Q48_16::from_bits(bits + 1)).0;
            // First: adjacency must be monotone. `wrapping_sub` would
            // turn a regressed wrap (`p0 = i64::MAX`, `p1 = i64::MIN`)
            // into a tiny positive step and silently pass the bound
            // check; an explicit `p1 >= p0` guard catches that.
            prop_assert!(
                p1 >= p0,
                "bits {}→{}: output regressed from {} to {}",
                bits, bits + 1, p0, p1
            );
            // And the forward step is bounded by ceil(num/den) + 1
            // (the +1 covers div_euclid rounding and the saturation
            // plateau). Cast to i128 for non-wrapping subtraction.
            let step = i128::from(p1) - i128::from(p0);
            let max_step = (psc.num / psc.den) + 1;
            prop_assert!(
                step <= max_step,
                "bits {}→{}: step {} exceeds max {}",
                bits, bits + 1, step, max_step
            );
        }
    }

    #[test]
    fn pico_sample_inner_saturates_at_i64_boundaries() {
        // At 48 kHz, num/den reduces to 1_953_125 / 6_144 ≈ 317.87 pico
        // per bit. i64::MAX bits would map to ~2.93e21 pico, which is
        // way beyond i64::MAX (≈9.22e18), so the output must saturate.
        let psc = PicoSampleConn::new(48_000);
        assert_eq!(
            psc.inner(Q48_16::from_bits(i64::MAX)),
            Pico(i64::MAX),
            "inner(i64::MAX bits) must saturate to Pico(i64::MAX), not wrap"
        );
        assert_eq!(
            psc.inner(Q48_16::from_bits(i64::MIN)),
            Pico(i64::MIN),
            "inner(i64::MIN bits) must saturate to Pico(i64::MIN), not wrap"
        );

        // Same at 44.1 kHz (where NUM/DEN doesn't reduce to trivial
        // powers of 2).
        let psc = PicoSampleConn::new(44_100);
        assert_eq!(psc.inner(Q48_16::from_bits(i64::MAX)), Pico(i64::MAX));
        assert_eq!(psc.inner(Q48_16::from_bits(i64::MIN)), Pico(i64::MIN));
    }

    #[test]
    fn pico_sample_new_reduces_gcd_correctly() {
        // Semantic invariant: num/den ≡ 10¹² / (sr × 2¹⁶) regardless
        // of reduction. Check by cross-multiplication at each rate.
        let two_to_16: i128 = 1 << 16;
        let ten_to_12: i128 = 1_000_000_000_000;
        for sr in [44_100_u32, 48_000, 88_200, 96_000, 176_400, 192_000] {
            let psc = PicoSampleConn::new(sr);
            assert_eq!(
                psc.num * i128::from(sr) * two_to_16,
                psc.den * ten_to_12,
                "sr = {}: num·sr·2¹⁶ = {} ≠ den·10¹² = {}",
                sr,
                psc.num * i128::from(sr) * two_to_16,
                psc.den * ten_to_12,
            );
        }
    }

    #[test]
    #[should_panic(expected = "sample rate must be positive")]
    fn pico_sample_new_panics_on_zero_sr() {
        let _ = PicoSampleConn::new(0);
    }

    #[test]
    fn sample_tick_and_pico_sample_agree_at_120bpm_48k() {
        // 120 BPM / ppq=192 / 48 kHz: each quarter note = 0.5 s =
        // 24 000 samples = 5×10¹¹ pico. At tick 192 (one beat):
        let stc = SampleTickConn::new(48_000, mbpm(120), 192);
        let psc = PicoSampleConn::new(48_000);

        let via_stc: u64 = stc.inner(Tick(192));
        let pico_at_one_beat = Pico(500_000_000_000);
        let via_psc: i64 = psc.ceil(pico_at_one_beat).to_num::<i64>();
        assert_eq!(via_stc, 24_000);
        assert_eq!(via_psc, 24_000);
        assert_eq!(via_stc as i64, via_psc);

        // And at tick 384 (two beats = 1 s = 48 000 samples = 10¹² pico):
        assert_eq!(stc.inner(Tick(384)), 48_000);
        assert_eq!(
            psc.ceil(Pico(1_000_000_000_000)).to_num::<i64>(),
            48_000
        );
    }
}
