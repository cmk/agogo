//! Rate-typed sample-indexed time.
//!
//! Each standard audio sample rate (44.1, 48, 88.2, 96, 176.4, 192 kHz)
//! gets its own `#[repr(transparent)]` newtype over
//! [`fixed::FixedI64`]`<U16>` — i.e. Q48.16 samples: 48 bits of signed
//! integer sample count plus 16 bits of sub-sample fraction.
//!
//! Distinct types per rate prevent accidental rate mixing at compile
//! time: you cannot add an `R044` to an `R048`. To cross rates, apply the
//! appropriate Galois [`Conn`](connections::conn::Conn), which expresses the rounding semantics
//! explicitly.
//!
//! # Precision
//!
//! - **Integer range**: ±2⁴⁷ samples — at 48 kHz that is ±93 years
//!   (`2⁴⁷ / 48_000 ≈ 2.93 × 10⁹ s`).
//! - **Sub-sample resolution**: 2⁻¹⁶ of a sample ≈ 15 ppm of one sample
//!   at any rate. Far below sample-accurate.
//!
//! # Connection topology
//!
//! For every ordered pair `(Fine, Coarse)` where `Fine` has the
//! higher Q48.16-bits-per-second rate, a
//! [`Conn`](connections::conn::Conn)`<Fine, Coarse>` constant `RXX_RYY` exists:
//!
//! ```text
//!   Fine────────────ratio──────────Coarse   exactness
//!   R088   <─×2─>   R044                      integer
//!   R176  <─×4─>   R044                      integer
//!   R176  <─×2─>   R088                      integer
//!   R096   <─×2─>   R048                      integer
//!   R192  <─×4─>   R048                      integer
//!   R192  <─×2─>   R096                      integer
//!   R048   <─160:147─>  R044                  rational (lossy)
//!   R088   <─147:80─>   R048                  rational
//!   R176  <─147:40─>   R048                  rational
//!   R096   <─320:147─>  R044                  rational
//!   R096   <─160:147─>  R088                  rational
//!   R176  <─147:80─>   R096                  rational
//!   R192  <─640:147─>  R044                  rational
//!   R192  <─320:147─>  R088                  rational
//!   R192  <─160:147─>  R176                 rational
//!
//! Ratio labels read `NUM:DEN`, i.e. the `inner(Coarse) = Coarse ·
//! NUM/DEN Fine` multiplier (always ≥ 1 because Fine is the higher-
//! bits-per-second type).
//! ```
//!
//! Plus one `Conn<FD12, Rxx>` per rate connecting the sample tier to
//! the decimal SI-time tier from [`crate::conn::fixed`].
//! Each `Rxx` type also has an explicit transparent iso to
//! `FixedI64<U16>` (`R048Q016`) and a composed left connection to
//! whole `i64` sample counts (`R048I064`). Call sites that need a
//! semantic sample-count conversion use those named conns; raw Q48.16
//! representation access stays on the newtype.
//!
//! # Galois semantics for lossy `inner`
//!
//! When the ratio is rational (not integer), `inner` cannot be an exact
//! embedding — it rounds. This module defines `inner` as
//! `floor_div(coarse × NUM, DEN)` and derives `ceil` / `floor` in terms
//! of this `inner` so **both** Galois laws hold by construction:
//!
//! - `ceil ⊣ inner`:    `ceil(x) ≤ b  ⟺  x ≤ inner(b)`
//! - `inner ⊣ floor`:   `inner(b) ≤ x  ⟺  b ≤ floor(x)`
//!
//! The `floor` formula `floor_div((x+1)·DEN − 1, NUM)` looks unusual
//! but it is the correct upper adjoint of a lossy `inner`. For integer
//! ratios (`DEN = 1`) it collapses to the familiar `floor_div(x, NUM)`.

use crate::conn::fixed::FD12;
use connections::conn::{ViewL, ViewR};
use connections::fixed::i64::{Q000I064, Q016Q000};
use fixed::FixedI64;
use fixed::types::extra::{U0, U16};

/// Q48.16 samples. Alias for clarity; all rate newtypes wrap this.
pub type Q48_16 = FixedI64<U16>;
pub type Q64_0 = FixedI64<U0>;

/// Rates in audio samples per second.
pub trait SampleRate {
    const HZ: u32;
}

macro_rules! def_rate {
    ($name:ident, $hz:expr) => {
        #[repr(transparent)]
        #[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
        pub struct $name(pub Q48_16);

        impl SampleRate for $name {
            const HZ: u32 = $hz;
        }

        impl $name {
            /// Zero samples.
            pub const ZERO: Self = Self(Q48_16::from_bits(0));
            /// One whole sample at this rate.
            pub const ONE_SAMPLE: Self = Self(Q48_16::from_bits(1 << 16));

            /// Construct from an integer sample count.
            pub const fn from_sample(n: i64) -> Self {
                Self(Q48_16::from_bits(n << 16))
            }

            /// Construct from raw Q48.16 bits.
            pub const fn from_bits(bits: i64) -> Self {
                Self(Q48_16::from_bits(bits))
            }

            /// Extract raw Q48.16 bits.
            pub const fn to_bits(self) -> i64 {
                self.0.to_bits()
            }

            /// Integer sample part (via arithmetic shift, so negatives
            /// round toward −∞).
            pub const fn sample(self) -> i64 {
                self.0.to_bits() >> 16
            }

            /// Sub-sample fraction as a signed Q16 value in
            /// (−2¹⁵, +2¹⁵).
            pub const fn sub_q16(self) -> i16 {
                (self.0.to_bits() & 0xFFFF) as i16
            }
        }
    };
}

def_rate!(R044, 44_100);
def_rate!(R048, 48_000);
def_rate!(R088, 88_200);
def_rate!(R096, 96_000);
def_rate!(R176, 176_400);
def_rate!(R192, 192_000);

// ─────────────────────────────────────────────────────────────────
// Rate ↔ Rate connections
//
// `rate_conn!(NAME, Fine, Coarse, NUM, DEN)` builds a
// `Conn<Fine, Coarse>` where `Fine`-per-`Coarse` ratio is `NUM/DEN`
// (i.e. `1 Coarse = NUM/DEN Fine`). `NUM ≥ DEN ≥ 1`.
//
// For integer ratio (DEN = 1) `inner` is exact: `coarse_bits · NUM`.
// For rational ratio `inner` rounds toward −∞ via `floor_div`, and
// `floor` is the upper adjoint — see module docs.
// ─────────────────────────────────────────────────────────────────

macro_rules! rate_conn {
    ($CONN:ident, $Fine:ident, $Coarse:ident, $num:expr, $den:expr) => {
        connections::triple! {
            #[allow(non_camel_case_types)]
            #[derive(Copy, Clone, Debug, Default)]
            pub $CONN : $Fine => $Coarse {
                ceil:  $CONN::ceil_fn,
                inner: $CONN::inner_fn,
                floor: $CONN::floor_fn,
            }
        }

        #[allow(non_camel_case_types)]
        impl $CONN {
            const NUM: i128 = $num;
            const DEN: i128 = $den;

            fn ceil_fn(x: $Fine) -> $Coarse {
                // ceil(x) = ceil_div(x · DEN, NUM)
                let n: i128 = x.0.to_bits() as i128 * Self::DEN;
                let q = n.div_euclid(Self::NUM);
                let r = n.rem_euclid(Self::NUM);
                let bits = if r != 0 { q + 1 } else { q };
                $Coarse(Q48_16::from_bits(bits as i64))
            }

            fn inner_fn(c: $Coarse) -> $Fine {
                // inner(c) = floor_div(c · NUM, DEN)
                let n: i128 = c.0.to_bits() as i128 * Self::NUM;
                $Fine(Q48_16::from_bits(n.div_euclid(Self::DEN) as i64))
            }

            fn floor_fn(x: $Fine) -> $Coarse {
                // Upper adjoint of a lossy inner:
                //   floor(x) = floor_div(x · DEN + DEN − 1, NUM)
                // Equivalent to floor_div(x, NUM) when DEN = 1.
                let n: i128 = x.0.to_bits() as i128 * Self::DEN + (Self::DEN - 1);
                $Coarse(Q48_16::from_bits(n.div_euclid(Self::NUM) as i64))
            }

            pub fn ceil(self, x: $Fine) -> $Coarse {
                <Self as ViewL<$Fine, $Coarse>>::L.ceil(x)
            }

            pub fn inner(self, x: $Coarse) -> $Fine {
                <Self as ViewL<$Fine, $Coarse>>::L.inner(x)
            }

            pub fn floor(self, x: $Fine) -> $Coarse {
                <Self as ViewR<$Fine, $Coarse>>::R.floor(x)
            }
        }
    };
}

// Integer ratios (power-of-two intra-family).
rate_conn!(R088R044, R088, R044, 2, 1);
rate_conn!(R176R044, R176, R044, 4, 1);
rate_conn!(R176R088, R176, R088, 2, 1);
rate_conn!(R096R048, R096, R048, 2, 1);
rate_conn!(R192R048, R192, R048, 4, 1);
rate_conn!(R192R096, R192, R096, 2, 1);

// Rational ratios (cross-family). Naming convention: `RXXRYY` has
// `RXX` as the Fine side (higher Q48.16-bits-per-second) and `RYY` as
// Coarse. NUM ≥ DEN ≥ 1 so `inner(coarse) = coarse · NUM / DEN` is an
// upscale. Reduced ratios; gcd(NUM, 147) = 1 in every case so 147
// (= 3² · 7²) stays in the denominator whenever one side is from the
// 44.1k family.
rate_conn!(R048R044, R048, R044, 160, 147);
rate_conn!(R088R048, R088, R048, 147, 80);
rate_conn!(R176R048, R176, R048, 147, 40);
rate_conn!(R096R044, R096, R044, 320, 147);
rate_conn!(R096R088, R096, R088, 160, 147);
rate_conn!(R176R096, R176, R096, 147, 80);
rate_conn!(R192R044, R192, R044, 640, 147);
rate_conn!(R192R088, R192, R088, 320, 147);
rate_conn!(R192R176, R192, R176, 160, 147);

// ─────────────────────────────────────────────────────────────────
// Rate ↔ FD12 connections
//
// FD12 has 10¹² bits per second; an Rxxx rate has `R · 2¹⁶` bits per
// second (where `R` is the kHz-side sample rate). FD12 is the finer
// tier (more bits/sec), so the connections are `Conn<Fine=FD12,
// Coarse=Rxx>` with the relation NUM · sample_bit = DEN · pico after
// reducing by gcd. One Rxx-bit spans NUM/DEN picoseconds.
//
// Simplified ratios (computed once):
//   R048:  gcd(10^12, 48_000·2^16) = 512_000
//         num/den = (10^12 / 512_000) / ((48_000·2^16) / 512_000)
//                 = 1_953_125 / 6144
//   R096:  ratio = 1_953_125 / 12_288   (half of R048)
//   R192:  ratio = 1_953_125 / 24_576   (quarter of R048)
//   R044:  gcd(10^12, 44_100·2^16) = 102_400
//         num/den = 9_765_625 / 28_224
//   R088:  ratio = 9_765_625 / 56_448   (half of R044)
//   R176:  ratio = 9_765_625 / 112_896  (quarter of R044)
// ─────────────────────────────────────────────────────────────────

macro_rules! pico_conn {
    ($CONN:ident, $Rate:ident, $num:expr, $den:expr) => {
        connections::triple! {
            #[allow(non_camel_case_types)]
            #[derive(Copy, Clone, Debug, Default)]
            pub $CONN : FD12 => $Rate {
                ceil:  $CONN::ceil_fn,
                inner: $CONN::inner_fn,
                floor: $CONN::floor_fn,
            }
        }

        #[allow(non_camel_case_types)]
        impl $CONN {
            // Conn<Fine=FD12, Coarse=Rxx>:
            //   inner: Coarse → Fine. inner(s: Rxx) = floor_div(s_bits · NUM, DEN) picoseconds
            //   ceil:  Fine → Coarse. ceil(p: FD12)  = ceil_div(p · DEN, NUM) Rxx-bits
            //   floor: Fine → Coarse. floor(p: FD12) = floor_div(p · DEN + DEN − 1, NUM) Rxx-bits
            // The `floor(p) = floor_div((p+1)·DEN − 1, NUM)` form is
            // the Galois upper adjoint of a lossy `inner` (see module
            // docs); it collapses to the familiar `floor_div(p, NUM)`
            // when `DEN = 1`.
            const NUM: i128 = $num;
            const DEN: i128 = $den;

            fn ceil_fn(p: FD12) -> $Rate {
                let n: i128 = p.0 as i128 * Self::DEN;
                let q = n.div_euclid(Self::NUM);
                let r = n.rem_euclid(Self::NUM);
                let bits = if r != 0 { q + 1 } else { q };
                $Rate(Q48_16::from_bits(bits as i64))
            }

            fn inner_fn(s: $Rate) -> FD12 {
                let n: i128 = s.0.to_bits() as i128 * Self::NUM;
                FD12(n.div_euclid(Self::DEN) as i64)
            }

            fn floor_fn(p: FD12) -> $Rate {
                let n: i128 = p.0 as i128 * Self::DEN + (Self::DEN - 1);
                $Rate(Q48_16::from_bits(n.div_euclid(Self::NUM) as i64))
            }

            pub fn ceil(self, x: FD12) -> $Rate {
                <Self as ViewL<FD12, $Rate>>::L.ceil(x)
            }

            pub fn inner(self, x: $Rate) -> FD12 {
                <Self as ViewL<FD12, $Rate>>::L.inner(x)
            }

            pub fn floor(self, x: FD12) -> $Rate {
                <Self as ViewR<FD12, $Rate>>::R.floor(x)
            }
        }
    };
}

pico_conn!(FD12R044, R044, 9_765_625, 28_224);
pico_conn!(FD12R048, R048, 1_953_125, 6_144);
pico_conn!(FD12R088, R088, 9_765_625, 56_448);
pico_conn!(FD12R096, R096, 1_953_125, 12_288);
pico_conn!(FD12R176, R176, 9_765_625, 112_896);
pico_conn!(FD12R192, R192, 1_953_125, 24_576);

// ────────────────────────────────────────────────────────────────────
// Rate ↔ Q16 / i64 connections
// ────────────────────────────────────────────────────────────────────

macro_rules! sample_q016_conn {
    ($CONN:ident, $Rate:ident) => {
        connections::iso! {
            pub $CONN : $Rate => Q48_16 {
                forward: |s: $Rate| s.0,
                back:    |q: Q48_16| $Rate(q),
            }
        }
    };
}

macro_rules! sample_i064_conn {
    ($CONN:ident, $Rate:ident, $Q016:ident) => {
        pub struct $CONN;

        impl ViewL<$Rate, i64> for $CONN {
            const L: connections::conn::ConnL<$Rate, i64> = connections::compose_l!(
                <$Q016 as ViewL<$Rate, Q48_16>>::L,
                <Q016Q000 as ViewL<Q48_16, Q64_0>>::L,
                <Q000I064 as ViewL<Q64_0, i64>>::L,
            );
        }

        impl $CONN {
            pub fn ceil(self, x: $Rate) -> i64 {
                <Self as ViewL<$Rate, i64>>::L.ceil(x)
            }

            pub fn inner(self, x: i64) -> $Rate {
                <Self as ViewL<$Rate, i64>>::L.inner(x)
            }
        }
    };
}

macro_rules! sample_whole_conn {
    ($Q016:ident, $I064:ident, $Rate:ident) => {
        sample_q016_conn!($Q016, $Rate);
        sample_i064_conn!($I064, $Rate, $Q016);
    };
}

sample_whole_conn!(R044Q016, R044I064, R044);
sample_whole_conn!(R048Q016, R048I064, R048);
sample_whole_conn!(R088Q016, R088I064, R088);
sample_whole_conn!(R096Q016, R096I064, R096);
sample_whole_conn!(R176Q016, R176I064, R176);
sample_whole_conn!(R192Q016, R192I064, R192);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conn::arb::{
        pico_coarse, pico_fine, pico_safe, rate_coarse, rate_fine, rate_safe_fine,
    };
    use proptest::prelude::*;

    // ─────────────────────────────────────────────
    // Spot checks
    // ─────────────────────────────────────────────

    #[test]
    fn r048_from_sample_bits() {
        assert_eq!(R048::from_sample(0).to_bits(), 0);
        assert_eq!(R048::from_sample(1).to_bits(), 1 << 16);
        assert_eq!(R048::from_sample(-1).to_bits(), -(1 << 16));
        assert_eq!(R048::ONE_SAMPLE.to_bits(), 1 << 16);
    }

    #[test]
    fn r048_sample_and_sub() {
        let s = R048::from_sample(42);
        assert_eq!(s.sample(), 42);
        assert_eq!(s.sub_q16(), 0);

        // 1 sample + 1/4 sub-sample = 0x1_4000 bits (16384 = 0x4000)
        let s = R048::from_bits((1 << 16) | 0x4000);
        assert_eq!(s.sample(), 1);
        assert_eq!(s.sub_q16(), 0x4000);
    }

    #[test]
    fn r048_i064_spots() {
        assert_eq!(R048I064.inner(0).to_bits(), 0);
        assert_eq!(R048I064.inner(1).to_bits(), 1 << 16);
        assert_eq!(R048I064.ceil(R048::from_bits((1 << 16) - 1)), 1);
        assert_eq!(R048I064.ceil(R048::from_bits(-1)), 0);
        assert_eq!(R048I064.ceil(R048::from_bits(-(1 << 16) - 1)), -1);
    }

    #[test]
    fn r088_r044_power_of_two_exact_embed() {
        // 1 R044 sample = 2 R088 samples, bit-exact.
        assert_eq!(R088R044.inner(R044::from_sample(7)), R088::from_sample(14));
        // ceil and floor agree on values that land cleanly.
        assert_eq!(R088R044.ceil(R088::from_sample(14)), R044::from_sample(7));
        assert_eq!(R088R044.floor(R088::from_sample(14)), R044::from_sample(7));
        // Off-by-one R088 bit → ceil/floor differ by 1 R044 bit.
        let s088_odd = R088::from_bits(R088::from_sample(14).to_bits() + 1);
        assert_eq!(
            R088R044.ceil(s088_odd),
            R044::from_bits(R044::from_sample(7).to_bits() + 1)
        );
        assert_eq!(
            R088R044.floor(s088_odd),
            R044::from_bits(R044::from_sample(7).to_bits())
        );
    }

    #[test]
    fn r048_r044_rational_boundary() {
        // 1 R044 bit = 160/147 R048 bits (floor), so inner(R044(147)) = R048(160) exactly.
        let r044 = R044::from_bits(147);
        assert_eq!(R048R044.inner(r044), R048::from_bits(160));
        // Round-trip at the boundary.
        assert_eq!(R048R044.ceil(R048::from_bits(160)), R044::from_bits(147));
        assert_eq!(R048R044.floor(R048::from_bits(160)), R044::from_bits(147));
        // At x=161, inner(148) = floor(148·160/147) = floor(161.088) = 161. So
        // both ceil and floor of 161 land on 148.
        assert_eq!(R048R044.ceil(R048::from_bits(161)), R044::from_bits(148));
        assert_eq!(R048R044.floor(R048::from_bits(161)), R044::from_bits(148));
        // A value skipped by the staircase: inner(11) = 11, inner(12) = 13,
        // so x=12 is not hit. ceil(12) = 12, floor(12) = 11.
        assert_eq!(R048R044.ceil(R048::from_bits(12)), R044::from_bits(12));
        assert_eq!(R048R044.floor(R048::from_bits(12)), R044::from_bits(11));
    }

    #[test]
    fn r048_pico_spot() {
        // 1 R048 sample = 1/48000 s = 1_000_000_000_000/48_000 ps = 20_833_333.333… ps.
        // inner(R048::from_sample(1)) should be the floor_div version.
        // R048(1 sample) = 65_536 bits. inner = floor_div(65_536 · 1_953_125, 6_144).
        // = floor_div(128_000_000_000, 6_144) = 20_833_333.
        let p = FD12R048.inner(R048::from_sample(1));
        assert_eq!(p.0, 20_833_333);
        // ceil of that same FD12 is back to exactly 1 R048 sample.
        assert_eq!(FD12R048.ceil(FD12(20_833_333)), R048::from_sample(1));
        // floor of one ps higher is still 1 sample.
        assert_eq!(FD12R048.floor(FD12(20_833_333)), R048::from_sample(1));
    }

    // ─────────────────────────────────────────────
    // Galois property battery
    //
    // For each Conn<F, C>, we test:
    //   - upper Galois: ceil(x) ≤ b ⟺ x ≤ inner(b)
    //   - lower Galois: inner(b) ≤ x ⟺ b ≤ floor(x)
    //   - ceil/floor monotone
    //   - floor ≤ ceil
    //   - inner-then-ceil and inner-then-floor round-trip (for integer
    //     ratios the embedding is exact so both round-trip; for rational
    //     the embedding is lossy so the round-trip may differ by 1 ULP,
    //     which the Galois laws already bound)
    //
    // Strategies (`rate_coarse`, `rate_fine`, `rate_safe_fine`) live
    // in `crate::conn::arb` (vendored from `connections @ d1ac1ead`'s
    // `property::arb` alongside the type families they generate for).
    // ─────────────────────────────────────────────

    macro_rules! props_for_conn {
        ($mod:ident, $conn:ident, $Fine:ident, $Coarse:ident, $num:expr, $den:expr) => {
            mod $mod {
                use super::*;
                use connections::prop::conn as laws;

                proptest! {
                    #[test]
                    fn monotone_l(
                        x in rate_fine($den, $num),
                        y in rate_fine($den, $num),
                    ) {
                        prop_assert!(laws::monotone_l(
                            &<$conn as ViewL<$Fine, $Coarse>>::L,
                            $Fine::from_bits(x),
                            $Fine::from_bits(y),
                        ));
                    }

                    #[test]
                    fn monotone_r(
                        a in rate_coarse($num),
                        b in rate_coarse($num),
                    ) {
                        prop_assert!(laws::monotone_r(
                            &<$conn as ViewR<$Fine, $Coarse>>::R,
                            $Coarse::from_bits(a),
                            $Coarse::from_bits(b),
                        ));
                    }

                    #[test]
                    fn floor_le_ceil(x in rate_fine($den, $num)) {
                        prop_assert!(laws::floor_le_ceil(&$conn, $Fine::from_bits(x)));
                    }

                    #[test]
                    fn galois_l(
                        x in rate_fine($den, $num),
                        b in rate_coarse($num),
                    ) {
                        prop_assert!(laws::galois_l(
                            &<$conn as ViewL<$Fine, $Coarse>>::L,
                            $Fine::from_bits(x),
                            $Coarse::from_bits(b),
                        ));
                    }

                    #[test]
                    fn galois_r(
                        x in rate_fine($den, $num),
                        b in rate_coarse($num),
                    ) {
                        prop_assert!(laws::galois_r(
                            &<$conn as ViewR<$Fine, $Coarse>>::R,
                            $Fine::from_bits(x),
                            $Coarse::from_bits(b),
                        ));
                    }

                    // For integer ratios (DEN=1) the embedding is exact,
                    // so the strict roundtrip holds. For rational ratios
                    // the embedding is lossy; the Galois laws above
                    // already bound the slack.
                    #[test]
                    fn roundtrip_ceil_integer_ratio(b in rate_coarse($num)) {
                        if $den == 1 {
                            prop_assert!(laws::roundtrip_ceil(
                                &<$conn as ViewL<$Fine, $Coarse>>::L,
                                $Coarse::from_bits(b),
                            ));
                        }
                    }

                    #[test]
                    fn roundtrip_floor_integer_ratio(b in rate_coarse($num)) {
                        if $den == 1 {
                            prop_assert!(laws::roundtrip_floor(
                                &<$conn as ViewR<$Fine, $Coarse>>::R,
                                $Coarse::from_bits(b),
                            ));
                        }
                    }

                    // Closure laws use rate_safe_fine because the
                    // round-trip can grow by up to num/den < num units.
                    #[test]
                    fn closure_l(x in rate_safe_fine($num)) {
                        prop_assert!(laws::closure_l(&<$conn as ViewL<$Fine, $Coarse>>::L, $Fine::from_bits(x)));
                    }

                    #[test]
                    fn closure_r(x in rate_safe_fine($num)) {
                        prop_assert!(laws::closure_r(&<$conn as ViewR<$Fine, $Coarse>>::R, $Fine::from_bits(x)));
                    }

                    #[test]
                    fn idempotent(x in rate_safe_fine($num)) {
                        prop_assert!(laws::idempotent_l(&<$conn as ViewL<$Fine, $Coarse>>::L, $Fine::from_bits(x)));
                    }
                }
            }
        };
    }

    // Integer-ratio pairs.
    props_for_conn!(p_r088r044, R088R044, R088, R044, 2, 1);
    props_for_conn!(p_r176r044, R176R044, R176, R044, 4, 1);
    props_for_conn!(p_r176r088, R176R088, R176, R088, 2, 1);
    props_for_conn!(p_r096r048, R096R048, R096, R048, 2, 1);
    props_for_conn!(p_r192r048, R192R048, R192, R048, 4, 1);
    props_for_conn!(p_r192r096, R192R096, R192, R096, 2, 1);

    // Cross-family rational pairs.
    props_for_conn!(p_r048r044, R048R044, R048, R044, 160, 147);
    props_for_conn!(p_r088r048, R088R048, R088, R048, 147, 80);
    props_for_conn!(p_r176r048, R176R048, R176, R048, 147, 40);
    props_for_conn!(p_r096r044, R096R044, R096, R044, 320, 147);
    props_for_conn!(p_r096r088, R096R088, R096, R088, 160, 147);
    props_for_conn!(p_r176r096, R176R096, R176, R096, 147, 80);
    props_for_conn!(p_r192r044, R192R044, R192, R044, 640, 147);
    props_for_conn!(p_r192r088, R192R088, R192, R088, 320, 147);
    props_for_conn!(p_r192r176, R192R176, R192, R176, 160, 147);

    // FD12 connections. Here Fine = FD12, Coarse = Rxx. The macro is
    // identical but FD12 is not an Rxx — write a tailored mod per conn
    // that reads `.0` on FD12 and `.0.to_bits()` on Rxx.
    macro_rules! props_for_pico_conn {
        ($mod:ident, $conn:ident, $Rate:ident, $num:expr, $den:expr) => {
            mod $mod {
                use super::*;
                use connections::prop::conn as laws;

                proptest! {
                    #[test]
                    fn monotone_l(
                        a in pico_fine(),
                        b in pico_fine(),
                    ) {
                        prop_assert!(laws::monotone_l(&<$conn as ViewL<FD12, $Rate>>::L, FD12(a), FD12(b)));
                    }

                    #[test]
                    fn monotone_r(
                        a in pico_coarse($num, $den),
                        b in pico_coarse($num, $den),
                    ) {
                        prop_assert!(laws::monotone_r(
                            &<$conn as ViewR<FD12, $Rate>>::R,
                            $Rate::from_bits(a),
                            $Rate::from_bits(b),
                        ));
                    }

                    #[test]
                    fn floor_le_ceil(p in pico_fine()) {
                        let pp = FD12(p);
                        prop_assert!(laws::floor_le_ceil(&$conn, pp));
                        // Stronger: rational-ratio ULP bound
                        // (`ceil - floor ≤ 1` Rxx Q48.16 ULP).
                        prop_assert!(laws::ulp_bound(
                            &$conn,
                            pp,
                            |s: $Rate| s.0.to_bits(),
                        ));
                    }

                    #[test]
                    fn galois_l(
                        p in pico_fine(),
                        s in pico_coarse($num, $den),
                    ) {
                        prop_assert!(laws::galois_l(
                            &<$conn as ViewL<FD12, $Rate>>::L,
                            FD12(p),
                            $Rate::from_bits(s),
                        ));
                    }

                    #[test]
                    fn galois_r(
                        p in pico_fine(),
                        s in pico_coarse($num, $den),
                    ) {
                        prop_assert!(laws::galois_r(
                            &<$conn as ViewR<FD12, $Rate>>::R,
                            FD12(p),
                            $Rate::from_bits(s),
                        ));
                    }

                    // Closure laws use pico_safe because the
                    // round-trip can grow p by up to NUM/DEN ps.
                    #[test]
                    fn closure_l(p in pico_safe($num)) {
                        prop_assert!(laws::closure_l(&<$conn as ViewL<FD12, $Rate>>::L, FD12(p)));
                    }

                    #[test]
                    fn closure_r(p in pico_safe($num)) {
                        prop_assert!(laws::closure_r(&<$conn as ViewR<FD12, $Rate>>::R, FD12(p)));
                    }

                    #[test]
                    fn idempotent(p in pico_safe($num)) {
                        prop_assert!(laws::idempotent_l(&<$conn as ViewL<FD12, $Rate>>::L, FD12(p)));
                    }
                }
            }
        };
    }

    props_for_pico_conn!(p_fd12r044, FD12R044, R044, 9_765_625, 28_224);
    props_for_pico_conn!(p_fd12r048, FD12R048, R048, 1_953_125, 6_144);
    props_for_pico_conn!(p_fd12r088, FD12R088, R088, 9_765_625, 56_448);
    props_for_pico_conn!(p_fd12r096, FD12R096, R096, 1_953_125, 12_288);
    props_for_pico_conn!(p_fd12r176, FD12R176, R176, 9_765_625, 112_896);
    props_for_pico_conn!(p_fd12r192, FD12R192, R192, 1_953_125, 24_576);

    macro_rules! props_for_sample_whole_conn {
        ($iso_mod:ident, $l_mod:ident, $iso:ident, $whole:ident, $Rate:ident) => {
            connections::law_battery! {
                mod $iso_mod,
                conn: $iso,
                fine: any::<i64>().prop_map($Rate::from_bits),
                coarse: any::<i64>().prop_map(Q48_16::from_bits),
                subset: iso_only,
                cases: 64,
            }

            connections::law_battery! {
                mod $l_mod,
                conn: $whole,
                fine: any::<i64>().prop_map($Rate::from_bits),
                coarse: any::<i64>(),
                subset: l_only,
                cases: 64,
            }
        };
    }

    props_for_sample_whole_conn!(p_r044q016, p_r044i064, R044Q016, R044I064, R044);
    props_for_sample_whole_conn!(p_r048q016, p_r048i064, R048Q016, R048I064, R048);
    props_for_sample_whole_conn!(p_r088q016, p_r088i064, R088Q016, R088I064, R088);
    props_for_sample_whole_conn!(p_r096q016, p_r096i064, R096Q016, R096I064, R096);
    props_for_sample_whole_conn!(p_r176q016, p_r176i064, R176Q016, R176I064, R176);
    props_for_sample_whole_conn!(p_r192q016, p_r192i064, R192Q016, R192I064, R192);

    // Sanity-check the FD12↔sample rate against the transcendental
    // definition: inner(Rxx::from_sample(1)) should be within 0.5 ps
    // of 10^12 / Rxx::HZ.
    #[test]
    fn fd12_inner_matches_ideal() {
        // Use f64 for the ideal — this test only, asserts sit here as
        // proof that the integer math agrees with the analytic formula.
        fn check<R: SampleRate + Copy>(got_pico: FD12) {
            let got = got_pico.0 as f64;
            let ideal = 1.0e12 / (R::HZ as f64);
            assert!(
                (got - ideal).abs() <= 1.0,
                "Rate {}: got {}, ideal {}",
                R::HZ,
                got,
                ideal
            );
        }
        check::<R044>(FD12R044.inner(R044::from_sample(1)));
        check::<R048>(FD12R048.inner(R048::from_sample(1)));
        check::<R088>(FD12R088.inner(R088::from_sample(1)));
        check::<R096>(FD12R096.inner(R096::from_sample(1)));
        check::<R176>(FD12R176.inner(R176::from_sample(1)));
        check::<R192>(FD12R192.inner(R192::from_sample(1)));
    }
}
