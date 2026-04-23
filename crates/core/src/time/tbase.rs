//! `TBase` — musical time base at 192 PPQN.
//!
//! Port of `Control.Cirklon.Type.Time.TBase`. T1 of plan-2026-04-22-01
//! delivers the enum, per-variant tick count, and the divisibility
//! preorder. T2 layers lattice ops (join/meet/heyting/coheyting) on
//! top.

use connections::order::Ple;

/// Musical time base (grid resolution) at 192 PPQN.
///
/// Hardware supports `T1`–`T64` (straight) and `T2t`–`T64t` (triplet).
/// `T128t` is DSL-only (4-tick scheduling resolution via delays).
///
/// The 14 constructors form a finite distributive Heyting lattice
/// under divisibility of tick counts: `a ≤ b ⟺ tick_count(a)` divides
/// `tick_count(b)`. `T128t` (4 ticks) is the bottom; `T1` (768 ticks,
/// one whole note) is the top.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum TBase {
    T1,
    T2,
    T4,
    T8,
    T16,
    T32,
    T64,
    T2t,
    T4t,
    T8t,
    T16t,
    T32t,
    T64t,
    T128t,
}

impl TBase {
    /// Every `TBase` variant, coarse-to-fine within straight then triplet.
    /// Used as a source for exhaustive tests and as the sample set for
    /// the `arb_tbase` proptest strategy.
    pub const ALL: [TBase; 14] = [
        TBase::T1,
        TBase::T2,
        TBase::T4,
        TBase::T8,
        TBase::T16,
        TBase::T32,
        TBase::T64,
        TBase::T2t,
        TBase::T4t,
        TBase::T8t,
        TBase::T16t,
        TBase::T32t,
        TBase::T64t,
        TBase::T128t,
    ];

    /// Ticks per step at 192 PPQN. Quarter note = 192, whole note = 768.
    pub const fn tick_count(self) -> u32 {
        match self {
            TBase::T1 => 768,
            TBase::T2 => 384,
            TBase::T4 => 192,
            TBase::T8 => 96,
            TBase::T16 => 48,
            TBase::T32 => 24,
            TBase::T64 => 12,
            TBase::T2t => 256,
            TBase::T4t => 128,
            TBase::T8t => 64,
            TBase::T16t => 32,
            TBase::T32t => 16,
            TBase::T64t => 8,
            TBase::T128t => 4,
        }
    }
}

/// Divisibility preorder: `a.ple(&b)` iff `tick_count(a)` divides
/// `tick_count(b)`. Because the 14 tick counts are distinct, this is
/// in fact a partial order on `TBase` — equivalence classes are
/// singletons.
impl Ple for TBase {
    fn ple(&self, other: &Self) -> bool {
        other.tick_count() % self.tick_count() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arb::arb_tbase;
    use proptest::prelude::*;

    // ── Spot checks on tick_count ──────────────────────────────────

    #[test]
    fn tick_count_t4_is_ppqn() {
        assert_eq!(TBase::T4.tick_count(), 192);
    }

    #[test]
    fn tick_count_t8t() {
        assert_eq!(TBase::T8t.tick_count(), 64);
    }

    #[test]
    fn tick_count_t16() {
        assert_eq!(TBase::T16.tick_count(), 48);
    }

    #[test]
    fn tick_count_t1_is_top() {
        assert_eq!(TBase::T1.tick_count(), 768);
    }

    #[test]
    fn tick_count_t128t_is_bottom() {
        assert_eq!(TBase::T128t.tick_count(), 4);
    }

    #[test]
    fn all_tick_counts_are_distinct() {
        let mut counts: Vec<u32> = TBase::ALL.iter().map(|tb| tb.tick_count()).collect();
        counts.sort_unstable();
        let n = counts.len();
        counts.dedup();
        assert_eq!(counts.len(), n, "tick counts collide: {counts:?}");
    }

    // ── Spot checks on divisibility preorder ──────────────────────

    #[test]
    fn ple_t16_below_t4() {
        // 48 divides 192.
        assert!(TBase::T16.ple(&TBase::T4));
    }

    #[test]
    fn ple_t4_not_below_t16() {
        // 192 does not divide 48.
        assert!(!TBase::T4.ple(&TBase::T16));
    }

    #[test]
    fn ple_t8_and_t8t_incomparable() {
        // 96 ∤ 64 and 64 ∤ 96 — straight and triplet scales cross at T1/T128t only.
        assert!(!TBase::T8.ple(&TBase::T8t));
        assert!(!TBase::T8t.ple(&TBase::T8));
    }

    #[test]
    fn t128t_is_bottom_of_lattice() {
        // 4 divides every other tick count.
        for tb in TBase::ALL {
            assert!(TBase::T128t.ple(&tb), "T128t should be ≤ {tb:?}");
        }
    }

    #[test]
    fn t1_is_top_of_lattice() {
        // Every tick count divides 768.
        for tb in TBase::ALL {
            assert!(tb.ple(&TBase::T1), "{tb:?} should be ≤ T1");
        }
    }

    // ── Preorder property tests ───────────────────────────────────

    proptest! {
        /// Reflexivity: `a ≤ a` for every `TBase`.
        #[test]
        fn tbase_ple_reflexive(a in arb_tbase()) {
            prop_assert!(a.ple(&a));
        }

        /// Antisymmetry: `a ≤ b ∧ b ≤ a ⟹ a = b`. Holds strictly (not
        /// just up to equivalence) because tick counts are distinct.
        #[test]
        fn tbase_ple_antisymmetric(a in arb_tbase(), b in arb_tbase()) {
            if a.ple(&b) && b.ple(&a) {
                prop_assert_eq!(a, b);
            }
        }

        /// Transitivity: `a ≤ b ∧ b ≤ c ⟹ a ≤ c`.
        #[test]
        fn tbase_ple_transitive(a in arb_tbase(), b in arb_tbase(), c in arb_tbase()) {
            if a.ple(&b) && b.ple(&c) {
                prop_assert!(a.ple(&c));
            }
        }
    }
}
