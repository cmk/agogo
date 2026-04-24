//! `TBase` — musical time base at 192 PPQN.
//!
//! Port of `Control.Cirklon.Type.Time.TBase`. T1 of plan-2026-04-22-01
//! delivers the enum, per-variant tick count, and the divisibility
//! preorder. T2 layers lattice ops (join/meet/heyting/coheyting) on
//! top.

use connections::lattice::Ple;

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

impl std::fmt::Display for TBase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            TBase::T1 => "t1",
            TBase::T2 => "t2",
            TBase::T4 => "t4",
            TBase::T8 => "t8",
            TBase::T16 => "t16",
            TBase::T32 => "t32",
            TBase::T64 => "t64",
            TBase::T2t => "t2t",
            TBase::T4t => "t4t",
            TBase::T8t => "t8t",
            TBase::T16t => "t16t",
            TBase::T32t => "t32t",
            TBase::T64t => "t64t",
            TBase::T128t => "t128t",
        };
        f.write_str(s)
    }
}

impl std::str::FromStr for TBase {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "t1" => Ok(TBase::T1),
            "t2" => Ok(TBase::T2),
            "t4" => Ok(TBase::T4),
            "t8" => Ok(TBase::T8),
            "t16" => Ok(TBase::T16),
            "t32" => Ok(TBase::T32),
            "t64" => Ok(TBase::T64),
            "t2t" => Ok(TBase::T2t),
            "t4t" => Ok(TBase::T4t),
            "t8t" => Ok(TBase::T8t),
            "t16t" => Ok(TBase::T16t),
            "t32t" => Ok(TBase::T32t),
            "t64t" => Ok(TBase::T64t),
            "t128t" => Ok(TBase::T128t),
            _ => Err(format!("unknown TBase: {s}")),
        }
    }
}

// ── Lattice operations ────────────────────────────────────────────
//
// The 14 tick counts are all of the form `2^i * 3^j` with `j ∈ {0, 1}`
// and bounded `i`, so LCM and GCD on tick counts stay within the set.
// The lattice is bounded, distributive, and Heyting (but not Boolean:
// some elements lack a complement).

fn gcd_u32(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a
}

fn lcm_u32(a: u32, b: u32) -> u32 {
    // Dividing before multiplying avoids intermediate overflow; for
    // values in the 14 tick counts (max 768), plain multiplication
    // wouldn't overflow `u32` either, but the style generalises if
    // this helper is reused later.
    a / gcd_u32(a, b) * b
}

/// Look up the unique `TBase` with the given tick count, if one
/// exists. Returns `None` for values outside the 14-element set.
fn from_tick_count(n: u32) -> Option<TBase> {
    TBase::ALL.iter().copied().find(|tb| tb.tick_count() == n)
}

/// Lattice join (coarsening): LCM of tick counts. The closed-form
/// `.expect` is valid because the 14-element lattice is closed under
/// LCM; `tbase_lattice_closure` in the tests verifies this exhaustively.
pub fn join(a: TBase, b: TBase) -> TBase {
    from_tick_count(lcm_u32(a.tick_count(), b.tick_count()))
        .expect("TBase lattice is closed under LCM")
}

/// Lattice meet (refinement): GCD of tick counts.
pub fn meet(a: TBase, b: TBase) -> TBase {
    from_tick_count(gcd_u32(a.tick_count(), b.tick_count()))
        .expect("TBase lattice is closed under GCD")
}

/// Heyting implication `a // b`: the coarsest `c` such that
/// `meet(a, c) ≤ b`. Computed directly as a join over the witness set.
///
/// The witness set is always non-empty — `c = T128t` makes
/// `meet(a, c) = T128t`, which is the lattice bottom and thus ≤ any
/// `b`.
pub fn heyting(a: TBase, b: TBase) -> TBase {
    TBase::ALL
        .iter()
        .copied()
        .filter(|&c| meet(a, c).ple(&b))
        .reduce(join)
        .expect("heyting witness set is non-empty (T128t always qualifies)")
}

/// Co-Heyting co-implication `a \\ b`: the finest `c` such that
/// `a ≤ join(b, c)`. Computed as a meet over the witness set.
///
/// The witness set is always non-empty — `c = T1` makes
/// `join(b, c) = T1`, the top, which every `a` refines.
pub fn coheyting(a: TBase, b: TBase) -> TBase {
    TBase::ALL
        .iter()
        .copied()
        .filter(|&c| a.ple(&join(b, c)))
        .reduce(meet)
        .expect("coheyting witness set is non-empty (T1 always qualifies)")
}

/// Heyting negation: `neg(x) = heyting(x, bottom)`. The coarsest grid
/// whose meet with `x` collapses to the lattice bottom.
pub fn neg(x: TBase) -> TBase {
    heyting(x, TBase::T128t)
}

/// Co-Heyting co-negation: `non(x) = coheyting(top, x)`. The finest
/// grid whose join with `x` reaches the lattice top. In general
/// `neg(x) ≤ non(x)` with strict inequality for some `x`, confirming
/// the lattice is Heyting but not Boolean.
pub fn non(x: TBase) -> TBase {
    coheyting(TBase::T1, x)
}

/// Co-Heyting boundary: `boundary(x) = meet(x, non(x))`.
pub fn boundary(x: TBase) -> TBase {
    meet(x, non(x))
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

    // ── FromStr / Display ─────────────────────────────────────────

    #[test]
    fn tbase_fromstr_display_round_trip() {
        for tb in TBase::ALL {
            let s = tb.to_string();
            let parsed: TBase = s.parse().expect("parse own Display");
            assert_eq!(parsed, tb);
        }
    }

    #[test]
    fn tbase_fromstr_accepts_case_insensitive() {
        assert_eq!("T16".parse::<TBase>().unwrap(), TBase::T16);
        assert_eq!("t16".parse::<TBase>().unwrap(), TBase::T16);
        assert_eq!("T128t".parse::<TBase>().unwrap(), TBase::T128t);
        assert_eq!("T8T".parse::<TBase>().unwrap(), TBase::T8t);
    }

    #[test]
    fn tbase_fromstr_rejects_garbage() {
        assert!("whatever".parse::<TBase>().is_err());
        assert!("t3".parse::<TBase>().is_err()); // not in the 14
        assert!("".parse::<TBase>().is_err());
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

    // ── Spot checks on lattice ops ────────────────────────────────

    #[test]
    fn join_t4_t8t_is_t4() {
        // lcm(192, 64) = 192
        assert_eq!(join(TBase::T4, TBase::T8t), TBase::T4);
    }

    #[test]
    fn meet_t4_t8t_is_t8t() {
        // gcd(192, 64) = 64
        assert_eq!(meet(TBase::T4, TBase::T8t), TBase::T8t);
    }

    #[test]
    fn join_t4_t2t_is_top() {
        // lcm(192, 256) = 768 = T1 — straight and triplet reach top.
        assert_eq!(join(TBase::T4, TBase::T2t), TBase::T1);
    }

    #[test]
    fn meet_t4_t2t_is_bottom_region() {
        // gcd(192, 256) = 64 = T8t
        assert_eq!(meet(TBase::T4, TBase::T2t), TBase::T8t);
    }

    #[test]
    fn neg_t4_is_bottom() {
        // Coarsest c with gcd(192, tc(c)) = 4 — only T128t qualifies.
        assert_eq!(neg(TBase::T4), TBase::T128t);
    }

    #[test]
    fn non_t4_is_t2t() {
        // Finest c with lcm(192, tc(c)) = 768 — c must contribute 2^8,
        // so c ∈ {T2t, T1}; the finer one is T2t.
        assert_eq!(non(TBase::T4), TBase::T2t);
    }

    #[test]
    fn neg_leq_non_witness() {
        // Confirms TBase is Heyting but not Boolean: neg(T4) = T128t,
        // non(T4) = T2t, and T128t ≤ T2t strictly.
        assert!(neg(TBase::T4).ple(&non(TBase::T4)));
        assert_ne!(neg(TBase::T4), non(TBase::T4));
    }

    #[test]
    fn boundary_t4_is_t8t() {
        // meet(T4, non(T4)) = meet(T4, T2t) = gcd(192, 256) = 64 = T8t.
        assert_eq!(boundary(TBase::T4), TBase::T8t);
    }

    #[test]
    fn tbase_lattice_closure_under_join_and_meet() {
        // Exhaustive 14×14: lcm and gcd of any two tick counts resolve
        // to a TBase variant. The private `from_tick_count` would
        // return None otherwise; `join`/`meet` would then panic via
        // .expect, which would surface here as a test failure.
        for a in TBase::ALL {
            for b in TBase::ALL {
                let _ = join(a, b);
                let _ = meet(a, b);
            }
        }
    }

    #[test]
    fn join_bottom_is_identity() {
        for a in TBase::ALL {
            assert_eq!(join(a, TBase::T128t), a);
            assert_eq!(join(TBase::T128t, a), a);
        }
    }

    #[test]
    fn meet_top_is_identity() {
        for a in TBase::ALL {
            assert_eq!(meet(a, TBase::T1), a);
            assert_eq!(meet(TBase::T1, a), a);
        }
    }

    // ── Lattice property tests ────────────────────────────────────

    proptest! {
        /// Join equals LCM on tick counts.
        #[test]
        fn tbase_join_is_lcm(a in arb_tbase(), b in arb_tbase()) {
            prop_assert_eq!(
                join(a, b).tick_count(),
                lcm_u32(a.tick_count(), b.tick_count())
            );
        }

        /// Meet equals GCD on tick counts.
        #[test]
        fn tbase_meet_is_gcd(a in arb_tbase(), b in arb_tbase()) {
            prop_assert_eq!(
                meet(a, b).tick_count(),
                gcd_u32(a.tick_count(), b.tick_count())
            );
        }

        /// Join commutativity.
        #[test]
        fn tbase_join_commutative(a in arb_tbase(), b in arb_tbase()) {
            prop_assert_eq!(join(a, b), join(b, a));
        }

        /// Meet commutativity.
        #[test]
        fn tbase_meet_commutative(a in arb_tbase(), b in arb_tbase()) {
            prop_assert_eq!(meet(a, b), meet(b, a));
        }

        /// Join associativity.
        #[test]
        fn tbase_join_associative(
            a in arb_tbase(),
            b in arb_tbase(),
            c in arb_tbase(),
        ) {
            prop_assert_eq!(join(join(a, b), c), join(a, join(b, c)));
        }

        /// Meet associativity.
        #[test]
        fn tbase_meet_associative(
            a in arb_tbase(),
            b in arb_tbase(),
            c in arb_tbase(),
        ) {
            prop_assert_eq!(meet(meet(a, b), c), meet(a, meet(b, c)));
        }

        /// Absorption: `a ∧ (a ∨ b) = a` and `a ∨ (a ∧ b) = a`.
        #[test]
        fn tbase_lattice_absorption(a in arb_tbase(), b in arb_tbase()) {
            prop_assert_eq!(meet(a, join(a, b)), a);
            prop_assert_eq!(join(a, meet(a, b)), a);
        }

        /// Distributivity: `a ∧ (b ∨ c) = (a ∧ b) ∨ (a ∧ c)`.
        #[test]
        fn tbase_lattice_distributivity(
            a in arb_tbase(),
            b in arb_tbase(),
            c in arb_tbase(),
        ) {
            prop_assert_eq!(
                meet(a, join(b, c)),
                join(meet(a, b), meet(a, c))
            );
        }

        /// Heyting adjunction: `a ∧ c ≤ b ⟺ c ≤ a // b`.
        #[test]
        fn tbase_heyting_adjunction(
            a in arb_tbase(),
            b in arb_tbase(),
            c in arb_tbase(),
        ) {
            let lhs = meet(a, c).ple(&b);
            let rhs = c.ple(&heyting(a, b));
            prop_assert_eq!(lhs, rhs);
        }

        /// Co-Heyting adjunction (dual): `c \\ a ≤ b ⟺ c ≤ a ∨ b`.
        ///
        /// In our `coheyting(a, b)` convention this reads:
        /// `a ≤ join(b, c) ⟺ coheyting(a, b) ≤ c`.
        #[test]
        fn tbase_coheyting_adjunction(
            a in arb_tbase(),
            b in arb_tbase(),
            c in arb_tbase(),
        ) {
            let lhs = a.ple(&join(b, c));
            let rhs = coheyting(a, b).ple(&c);
            prop_assert_eq!(lhs, rhs);
        }
    }
}
