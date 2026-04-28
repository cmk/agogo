//! `TBase` — binary subdivision axis at 960 PPQN.
//!
//! 9 variants, each a power-of-2 fraction of the bar. Used wherever
//! a binary subdivision is required specifically (swing resolution,
//! the `n` field of [`crate::time::grid::Grid`]). The full 36-element
//! lattice lives in [`crate::time::grid`].
//!
//! `TBase` is totally ordered by divisibility of tick counts (a chain):
//! `T256 < … < T4 < T2 < T1` (finer grids — smaller tick counts — are
//! lower in the lattice), with `T1` (= bar) at the top and `T256`
//! (= bar/256) at the bottom. The custom `PartialOrd` / `Ord`
//! implementation below encodes this — `derive(PartialOrd)` would
//! produce declaration order, the inverse direction.

use crate::time::tick::PPQN;

/// Ticks per bar = `4 · PPQN`. The lattice's top element measured
/// in ticks. At 960 PPQN, `BAR = 3840`.
pub const BAR: u32 = 4 * PPQN;

/// Binary subdivision axis. `T<n>` denotes the binary subdivision
/// whose tick count is `BAR / n`, so `T1 = BAR` (whole note),
/// `T4 = BAR/4 = PPQN` (quarter), and `T256 = BAR/256` (smallest
/// useful 16th-of-16th-of-16th-of-… subdivision at 960 PPQN).
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum TBase {
    T1,
    T2,
    T4,
    T8,
    T16,
    T32,
    T64,
    T128,
    T256,
}

impl TBase {
    /// All 9 variants, coarsest-first (= largest tick count first).
    pub const ALL: [TBase; 9] = [
        TBase::T1,
        TBase::T2,
        TBase::T4,
        TBase::T8,
        TBase::T16,
        TBase::T32,
        TBase::T64,
        TBase::T128,
        TBase::T256,
    ];

    /// Power of 2 in the `BAR / tick_count` denominator.
    /// `T1.exp() = 0`, `T16.exp() = 4`, `T256.exp() = 8`.
    pub const fn exp(self) -> u32 {
        match self {
            TBase::T1 => 0,
            TBase::T2 => 1,
            TBase::T4 => 2,
            TBase::T8 => 3,
            TBase::T16 => 4,
            TBase::T32 => 5,
            TBase::T64 => 6,
            TBase::T128 => 7,
            TBase::T256 => 8,
        }
    }

    /// Inverse of [`exp`](Self::exp). Returns `None` for `e > 8`.
    pub const fn from_exp(e: u32) -> Option<TBase> {
        match e {
            0 => Some(TBase::T1),
            1 => Some(TBase::T2),
            2 => Some(TBase::T4),
            3 => Some(TBase::T8),
            4 => Some(TBase::T16),
            5 => Some(TBase::T32),
            6 => Some(TBase::T64),
            7 => Some(TBase::T128),
            8 => Some(TBase::T256),
            _ => None,
        }
    }

    /// Master ticks per step: `BAR / 2^exp`. Always integer (PPQN is
    /// chosen so `BAR = 3840 = 2⁸·3·5` is divisible by all 9 powers
    /// of 2 from 1 to 256).
    pub const fn tick_count(self) -> u32 {
        BAR >> self.exp()
    }
}

/// Divisibility ordering: `a ≤ b` iff `a`'s tick count divides
/// `b`'s. On the binary chain this is just `a.exp() >= b.exp()`
/// (more `exp` = smaller tick count = "finer" grid = lower in the
/// lattice). Total since the chain is a single linear order.
impl PartialOrd for TBase {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TBase {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // `a ≤ b ⟺ a.exp() ≥ b.exp()`, so `cmp` flips the direction
        // of `exp().cmp`.
        other.exp().cmp(&self.exp())
    }
}

impl std::fmt::Display for TBase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let n: u32 = 1u32 << self.exp();
        write!(f, "t{n}")
    }
}

impl std::str::FromStr for TBase {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let lower = s.to_ascii_lowercase();
        let rest = lower
            .strip_prefix('t')
            .ok_or_else(|| format!("TBase must start with 't': {s}"))?;
        let n: u32 = rest
            .parse()
            .map_err(|_| format!("TBase index must be a positive integer: {s}"))?;
        // `n` must be a power of 2 in [1, 256].
        if n == 0 || !n.is_power_of_two() || n > 256 {
            return Err(format!("TBase index must be a power of 2 in [1, 256]: {s}"));
        }
        let e = n.trailing_zeros();
        TBase::from_exp(e).ok_or_else(|| format!("TBase index out of range: {s}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arb::arb_tbase;
    use proptest::prelude::*;

    // ── Spot checks on tick_count ──────────────────────────────────

    #[test]
    fn tick_count_t1_is_bar() {
        assert_eq!(TBase::T1.tick_count(), BAR);
        assert_eq!(BAR, 3840);
    }

    #[test]
    fn tick_count_t4_is_ppqn() {
        assert_eq!(TBase::T4.tick_count(), PPQN);
        assert_eq!(PPQN, 960);
    }

    #[test]
    fn tick_count_t16_is_240() {
        assert_eq!(TBase::T16.tick_count(), 240);
    }

    #[test]
    fn tick_count_t256_is_15() {
        assert_eq!(TBase::T256.tick_count(), 15);
    }

    #[test]
    fn all_tick_counts_distinct() {
        let mut counts: Vec<u32> = TBase::ALL.iter().map(|tb| tb.tick_count()).collect();
        counts.sort_unstable();
        let n = counts.len();
        counts.dedup();
        assert_eq!(counts.len(), n, "tick counts collide: {counts:?}");
    }

    // ── exp / from_exp round-trip ─────────────────────────────────

    #[test]
    fn exp_round_trips() {
        for tb in TBase::ALL {
            assert_eq!(TBase::from_exp(tb.exp()), Some(tb));
        }
    }

    #[test]
    fn from_exp_out_of_range() {
        assert_eq!(TBase::from_exp(9), None);
        assert_eq!(TBase::from_exp(100), None);
    }

    // ── FromStr / Display ─────────────────────────────────────────

    #[test]
    fn fromstr_display_round_trip() {
        for tb in TBase::ALL {
            let s = tb.to_string();
            let parsed: TBase = s.parse().unwrap();
            assert_eq!(parsed, tb);
        }
    }

    #[test]
    fn fromstr_case_insensitive() {
        assert_eq!("T16".parse::<TBase>().unwrap(), TBase::T16);
        assert_eq!("t16".parse::<TBase>().unwrap(), TBase::T16);
        assert_eq!("T256".parse::<TBase>().unwrap(), TBase::T256);
    }

    #[test]
    fn fromstr_rejects_non_binary() {
        assert!("t3".parse::<TBase>().is_err());
        assert!("t6".parse::<TBase>().is_err());
        assert!("t512".parse::<TBase>().is_err()); // > 256
    }

    #[test]
    fn fromstr_rejects_garbage() {
        assert!("whatever".parse::<TBase>().is_err());
        assert!("".parse::<TBase>().is_err());
        assert!("t".parse::<TBase>().is_err());
    }

    // ── Spot checks on divisibility ordering ──────────────────────

    #[test]
    fn t16_below_t4() {
        // T16 = 240 divides T4 = 960.
        assert!(TBase::T16 <= TBase::T4);
    }

    #[test]
    fn t4_not_below_t16() {
        assert!(TBase::T4 > TBase::T16);
    }

    #[test]
    fn t256_is_bottom() {
        for tb in TBase::ALL {
            assert!(TBase::T256 <= tb, "T256 should be ≤ {tb:?}");
        }
    }

    #[test]
    fn t1_is_top() {
        for tb in TBase::ALL {
            assert!(tb <= TBase::T1, "{tb:?} should be ≤ T1");
        }
    }

    /// Divisibility chain: `T256 < T128 < … < T1`.
    #[test]
    fn divisibility_chain_strictly_ascending() {
        let chain = [
            TBase::T256,
            TBase::T128,
            TBase::T64,
            TBase::T32,
            TBase::T16,
            TBase::T8,
            TBase::T4,
            TBase::T2,
            TBase::T1,
        ];
        for w in chain.windows(2) {
            assert!(w[0] < w[1], "{:?} should be < {:?}", w[0], w[1]);
        }
    }

    // ── Order property tests ──────────────────────────────────────

    proptest! {
        #[test]
        fn ple_reflexive(a in arb_tbase()) {
            prop_assert!(a <= a);
        }

        #[test]
        fn ple_antisymmetric(a in arb_tbase(), b in arb_tbase()) {
            if a <= b && b <= a {
                prop_assert_eq!(a, b);
            }
        }

        #[test]
        fn ple_transitive(a in arb_tbase(), b in arb_tbase(), c in arb_tbase()) {
            if a <= b && b <= c {
                prop_assert!(a <= c);
            }
        }

        /// Total order on the binary chain: every pair is comparable.
        #[test]
        fn ple_total(a in arb_tbase(), b in arb_tbase()) {
            prop_assert!(a <= b || b <= a);
        }

        /// `tick_count` matches the formula `BAR >> exp()`.
        #[test]
        fn tick_count_matches_formula(a in arb_tbase()) {
            prop_assert_eq!(a.tick_count(), BAR >> a.exp());
        }
    }
}
