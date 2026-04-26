//! `Grid` — full lattice of single-bar subdivisions at 960 PPQN.
//!
//! Product of [`TBase`] (binary axis, 9 levels) and two square-free
//! flags `t` and `q` (factor of 3 absent, factor of 5 absent). 36
//! elements total.
//!
//! The lattice is a bounded distributive Heyting algebra under
//! divisibility of tick counts. Top = `Grid::T1` (= bar = 3840 ticks);
//! bottom = `Grid::T512P` (= 1 tick). Meet/join factor component-wise
//! on `(n, t, q)` because the lattice is the product of three
//! sub-lattices: the 9-element chain on `n`, and 2-element Boolean
//! lattices on `t` and `q`.
//!
//! `Display` and `FromStr` give the canonical name (`T16`, `T16T`,
//! `T2P`, etc.) and serve as the DSL atom morphism — see
//! `doc/designs/dsl.md`.
//!
//! # Coordinate convention
//!
//! `tick_count = BAR / (2^n.exp() · 3^t · 5^q)`, with `t` / `q` as
//! 0/1 according to the flag. So `t = true` means "factor of 3 is
//! *absent* from the tick count" (the triplet / p-track) and
//! `t = false` means "factor of 3 is present" (binary / quintuplet).
//! Same for `q` and factor 5.
//!
//! # Plan-name index
//!
//! The conventional musical name `T<n><suffix>` uses
//! `n = 2^(coord_n.exp() + (t || q ? 1 : 0))`. Binary uses the raw
//! `coord_n.exp()`; non-binary tracks shift by one because
//! `Tnt = (2/3) · Tn` shifts the denominator by a factor of 2.
//! Examples:
//! - `Grid::T16` is binary at `coord_n = T16, t = false, q = false`,
//!   plan-name "T16" (= 16 = 2^4).
//! - `Grid::T16T` is triplet of T16 at `coord_n = T8, t = true,
//!   q = false`, plan-name "T16T" (= 2 · 2^3 = 16).
//! - `Grid::T512P` is the p-track floor at `coord_n = T256, t = true,
//!   q = true`, plan-name "T512P" (= 2 · 2^8 = 512).

use connections::lattice::Ple;

use crate::time::tbase::{BAR, TBase};

/// Lattice element at 960 PPQN. `(n, t, q)` coordinates with
/// `t, q: bool` enforcing the square-free constraint at the type
/// level.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct Grid {
    /// Binary axis position. `n.exp()` is the power of 2 in the
    /// `BAR / tick_count` denominator.
    pub n: TBase,
    /// Factor of 3 absent from tick count (triplet or 15-tuplet track).
    pub t: bool,
    /// Factor of 5 absent from tick count (quintuplet or 15-tuplet track).
    pub q: bool,
}

impl Grid {
    pub const fn new(n: TBase, t: bool, q: bool) -> Self {
        Grid { n, t, q }
    }

    /// Tick count: `BAR / (2^n.exp() · 3^t · 5^q)`.
    pub const fn tick_count(self) -> u32 {
        let two_pow = 1u32 << self.n.exp();
        let three_factor = if self.t { 3 } else { 1 };
        let five_factor = if self.q { 5 } else { 1 };
        BAR / (two_pow * three_factor * five_factor)
    }

    /// Embed a `TBase` (binary) as a `Grid`. `binary.t = false,
    /// binary.q = false` — both 3 and 5 factors present.
    pub const fn from_tbase(b: TBase) -> Self {
        Grid {
            n: b,
            t: false,
            q: false,
        }
    }

    /// True if this `Grid` value lies on the binary track
    /// (no triplet / quintuplet flags).
    pub const fn is_binary(self) -> bool {
        !self.t && !self.q
    }

    // ── Binary track (9 consts) — t=false, q=false ───────────────

    pub const T1: Grid = Grid::new(TBase::T1, false, false);
    pub const T2: Grid = Grid::new(TBase::T2, false, false);
    pub const T4: Grid = Grid::new(TBase::T4, false, false);
    pub const T8: Grid = Grid::new(TBase::T8, false, false);
    pub const T16: Grid = Grid::new(TBase::T16, false, false);
    pub const T32: Grid = Grid::new(TBase::T32, false, false);
    pub const T64: Grid = Grid::new(TBase::T64, false, false);
    pub const T128: Grid = Grid::new(TBase::T128, false, false);
    pub const T256: Grid = Grid::new(TBase::T256, false, false);

    // ── Triplet track (9 consts) — t=true, q=false ───────────────
    // Plan name "T(2k)T" stores n = T(k) (because Tnt = 2/3 · Tn
    // shifts the binary parent).

    pub const T2T: Grid = Grid::new(TBase::T1, true, false); // 1280
    pub const T4T: Grid = Grid::new(TBase::T2, true, false); // 640
    pub const T8T: Grid = Grid::new(TBase::T4, true, false); // 320
    pub const T16T: Grid = Grid::new(TBase::T8, true, false); // 160
    pub const T32T: Grid = Grid::new(TBase::T16, true, false); // 80
    pub const T64T: Grid = Grid::new(TBase::T32, true, false); // 40
    pub const T128T: Grid = Grid::new(TBase::T64, true, false); // 20
    pub const T256T: Grid = Grid::new(TBase::T128, true, false); // 10
    pub const T512T: Grid = Grid::new(TBase::T256, true, false); // 5

    // ── Quintuplet track (9 consts) — t=false, q=true ────────────

    pub const T2Q: Grid = Grid::new(TBase::T1, false, true); // 768
    pub const T4Q: Grid = Grid::new(TBase::T2, false, true); // 384
    pub const T8Q: Grid = Grid::new(TBase::T4, false, true); // 192
    pub const T16Q: Grid = Grid::new(TBase::T8, false, true); // 96
    pub const T32Q: Grid = Grid::new(TBase::T16, false, true); // 48
    pub const T64Q: Grid = Grid::new(TBase::T32, false, true); // 24
    pub const T128Q: Grid = Grid::new(TBase::T64, false, true); // 12
    pub const T256Q: Grid = Grid::new(TBase::T128, false, true); // 6
    pub const T512Q: Grid = Grid::new(TBase::T256, false, true); // 3

    // ── 15-tuplet (p) track (9 consts) — t=true, q=true ──────────

    pub const T2P: Grid = Grid::new(TBase::T1, true, true); // 256
    pub const T4P: Grid = Grid::new(TBase::T2, true, true); // 128
    pub const T8P: Grid = Grid::new(TBase::T4, true, true); // 64
    pub const T16P: Grid = Grid::new(TBase::T8, true, true); // 32
    pub const T32P: Grid = Grid::new(TBase::T16, true, true); // 16
    pub const T64P: Grid = Grid::new(TBase::T32, true, true); // 8
    pub const T128P: Grid = Grid::new(TBase::T64, true, true); // 4
    pub const T256P: Grid = Grid::new(TBase::T128, true, true); // 2
    pub const T512P: Grid = Grid::new(TBase::T256, true, true); // 1

    /// All 36 elements, coarsest-first (largest tick count first
    /// within each track; binary → triplet → quintuplet → p).
    pub const ALL: [Grid; 36] = [
        // Binary (3840 → 15)
        Grid::T1, Grid::T2, Grid::T4, Grid::T8, Grid::T16,
        Grid::T32, Grid::T64, Grid::T128, Grid::T256,
        // Triplet (1280 → 5)
        Grid::T2T, Grid::T4T, Grid::T8T, Grid::T16T, Grid::T32T,
        Grid::T64T, Grid::T128T, Grid::T256T, Grid::T512T,
        // Quintuplet (768 → 3)
        Grid::T2Q, Grid::T4Q, Grid::T8Q, Grid::T16Q, Grid::T32Q,
        Grid::T64Q, Grid::T128Q, Grid::T256Q, Grid::T512Q,
        // 15-tuplet (256 → 1)
        Grid::T2P, Grid::T4P, Grid::T8P, Grid::T16P, Grid::T32P,
        Grid::T64P, Grid::T128P, Grid::T256P, Grid::T512P,
    ];
}

/// Divisibility preorder: `a.ple(&b)` iff `a`'s tick count divides
/// `b`'s. Factors component-wise — `a.ple(&b) ⟺ a.n.ple(&b.n) ∧
/// (a.t ≥ b.t) ∧ (a.q ≥ b.q)` (treating `true < false`, since
/// `t = true` / `q = true` mean the corresponding factor is absent,
/// making the tick count finer / lower on those axes).
impl Ple for Grid {
    fn ple(&self, other: &Self) -> bool {
        self.n.ple(&other.n)
            && (self.t as u8) >= (other.t as u8)
            && (self.q as u8) >= (other.q as u8)
    }
}

// ── Lattice operations (component-wise on the product) ──────────

/// Lattice meet (gcd of tick counts). Component-wise: `max` on the
/// `n.exp()` axis and `||` on each bool flag.
pub fn meet(a: Grid, b: Grid) -> Grid {
    let n_exp = a.n.exp().max(b.n.exp());
    Grid {
        n: TBase::from_exp(n_exp).expect("n.exp() ∈ [0, 8] is closed under max"),
        t: a.t || b.t,
        q: a.q || b.q,
    }
}

/// Lattice join (lcm of tick counts). Component-wise: `min` on the
/// `n.exp()` axis and `&&` on each bool flag.
pub fn join(a: Grid, b: Grid) -> Grid {
    let n_exp = a.n.exp().min(b.n.exp());
    Grid {
        n: TBase::from_exp(n_exp).expect("n.exp() ∈ [0, 8] is closed under min"),
        t: a.t && b.t,
        q: a.q && b.q,
    }
}

// ── Heyting algebra ─────────────────────────────────────────────

/// Heyting implication on the product lattice.
///
/// `imply(a, b)` = max{c ∈ L : meet(a, c) ⊑ b}
///
/// Upper adjoint of conjunction with a:
///   meet(a, x) ⊑ b  ⟺  x ⊑ imply(a, b)
///
/// Factors component-wise on the product; on the bool axes Heyting
/// implication coincides with classical implication (`a → b = !a || b`).
pub fn imply(a: Grid, b: Grid) -> Grid {
    Grid::ALL
        .iter()
        .copied()
        .filter(|&c| meet(a, c).ple(&b))
        .reduce(join)
        .expect("Grid::T512P always satisfies meet(a, T512P) = T512P ⊑ b")
}

/// Heyting pseudo-complement: `neg(x) = imply(x, ⊥)`.
///
/// The largest element orthogonal to `x` (i.e. whose meet with `x`
/// is bottom). Laws:
///   neg(⊥) = ⊤, neg(⊤) = ⊥
///   x ⊑ neg(neg(x))
///   meet(x, neg(x)) = ⊥
///   neg(neg(neg(x))) = neg(x)
pub fn neg(x: Grid) -> Grid {
    imply(x, Grid::T512P)
}

/// Heyting middle: `mid(x) = join(x, neg(x))`.
///
/// Not necessarily ⊤ — equals ⊤ only in a Boolean algebra. On this
/// lattice the excluded middle fails for most elements.
pub fn mid(x: Grid) -> Grid {
    join(x, neg(x))
}

// ── Co-Heyting algebra ──────────────────────────────────────────

/// Co-Heyting coimplication (subtraction) on the product lattice.
///
/// `coimp(a, b)` = min{c ∈ L : a ⊑ join(b, c)}
///
/// Lower adjoint of disjunction with b:
///   coimp(a, b) ⊑ c  ⟺  a ⊑ join(b, c)
///
/// Dual of [`imply`]: where `imply` finds the largest `c` satisfying
/// a conjunction constraint, `coimp` finds the smallest `c` satisfying
/// a disjunction constraint.
pub fn coimp(a: Grid, b: Grid) -> Grid {
    Grid::ALL
        .iter()
        .copied()
        .filter(|&c| a.ple(&join(b, c)))
        .reduce(meet)
        .expect("Grid::T1 always satisfies a ⊑ join(b, T1)")
}

/// Co-Heyting co-negation: `coneg(x) = coimp(⊤, x)`.
///
/// The smallest element whose join with `x` is ⊤. Laws:
///   coneg(⊥) = ⊤, coneg(⊤) = ⊥
///   coneg(coneg(x)) ⊑ x
///   join(x, coneg(x)) = ⊤
///   coneg(coneg(coneg(x))) = coneg(x)
pub fn coneg(x: Grid) -> Grid {
    coimp(Grid::T1, x)
}

/// Co-Heyting co-middle (boundary): `comid(x) = meet(x, coneg(x))`.
///
/// Not necessarily ⊥ — equals ⊥ only in a Boolean algebra. Satisfies
/// the Leibniz rule: `comid(meet(a, b)) = join(meet(comid(a), b),
/// meet(a, comid(b)))`.
pub fn comid(x: Grid) -> Grid {
    meet(x, coneg(x))
}

// ── Display / FromStr — the DSL atom morphism ───────────────────

impl std::fmt::Display for Grid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Plan-name index: binary uses n.exp(); non-binary tracks
        // shift by one because Tnt = (2/3)·Tn moves the denominator.
        let plan_exp = self.n.exp() + if self.t || self.q { 1 } else { 0 };
        let plan_idx: u32 = 1u32 << plan_exp;
        let suffix = match (self.t, self.q) {
            (false, false) => "",
            (true, false) => "t",
            (false, true) => "q",
            (true, true) => "p",
        };
        write!(f, "t{plan_idx}{suffix}")
    }
}

impl std::str::FromStr for Grid {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let lower = s.to_ascii_lowercase();
        let body = lower
            .strip_prefix('t')
            .ok_or_else(|| format!("Grid must start with 't': {s}"))?;
        // Strip optional suffix.
        let (digits, t, q) = if let Some(rest) = body.strip_suffix('t') {
            (rest, true, false)
        } else if let Some(rest) = body.strip_suffix('q') {
            (rest, false, true)
        } else if let Some(rest) = body.strip_suffix('p') {
            (rest, true, true)
        } else {
            (body, false, false)
        };
        let plan_idx: u32 = digits
            .parse()
            .map_err(|_| format!("Grid index must be a positive integer: {s}"))?;
        if plan_idx == 0 || !plan_idx.is_power_of_two() {
            return Err(format!("Grid index must be a power of 2: {s}"));
        }
        let plan_exp = plan_idx.trailing_zeros();
        // Reverse the Display shift: binary → coord_n.exp() = plan_exp;
        // non-binary → coord_n.exp() = plan_exp - 1.
        let coord_exp = if t || q {
            plan_exp.checked_sub(1).ok_or_else(|| {
                format!(
                    "non-binary Grid name needs plan_exp ≥ 1 (e.g. T2T, not T1T): {s}"
                )
            })?
        } else {
            plan_exp
        };
        let n = TBase::from_exp(coord_exp)
            .ok_or_else(|| format!("Grid index out of range: {s}"))?;
        Ok(Grid { n, t, q })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use std::collections::HashSet;

    fn arb_grid() -> impl Strategy<Value = Grid> {
        prop::sample::select(Grid::ALL.as_slice())
    }

    // ── Cardinality / closure ─────────────────────────────────────

    #[test]
    fn grid_all_has_36_elements() {
        assert_eq!(Grid::ALL.len(), 36);
        let s: HashSet<_> = Grid::ALL.iter().copied().collect();
        assert_eq!(s.len(), 36, "duplicates in Grid::ALL");
    }

    #[test]
    fn grid_factors_as_product() {
        // Grid::ALL = TBase::ALL × {false, true} × {false, true}.
        let all: HashSet<_> = Grid::ALL.iter().copied().collect();
        for n in TBase::ALL {
            for t in [false, true] {
                for q in [false, true] {
                    assert!(all.contains(&Grid { n, t, q }));
                }
            }
        }
    }

    #[test]
    fn grid_all_tick_counts_are_divisors_of_bar() {
        for g in Grid::ALL {
            let tc = g.tick_count();
            assert!(tc > 0, "tick_count is zero for {g:?}");
            assert_eq!(BAR % tc, 0, "{tc} does not divide BAR for {g:?}");
        }
    }

    #[test]
    fn grid_all_36_divisors_of_3840_present() {
        // The 36 divisors of 3840 = 2^8·3·5 with exponents
        // (a ∈ [0,8], b ∈ {0,1}, c ∈ {0,1}) are exactly the elements
        // of Grid::ALL by tick count.
        let mut expected: HashSet<u32> = HashSet::new();
        for a in 0..=8 {
            for b in 0..=1 {
                for c in 0..=1 {
                    let div: u32 = (1u32 << a) * 3u32.pow(b) * 5u32.pow(c);
                    expected.insert(BAR / div);
                }
            }
        }
        let got: HashSet<u32> = Grid::ALL.iter().map(|g| g.tick_count()).collect();
        assert_eq!(got, expected);
    }

    #[test]
    fn grid_track_factor_structure() {
        // Binary (t=false, q=false): tick count has factors 3 AND 5.
        // Triplet (t=true, q=false):  tick count has factor 5, no 3.
        // Quintuplet (t=false, q=true): factor 3, no 5.
        // p-track (t=true, q=true): neither 3 nor 5.
        for g in Grid::ALL {
            let tc = g.tick_count();
            let has_3 = tc % 3 == 0;
            let has_5 = tc % 5 == 0;
            assert_eq!(
                has_3, !g.t,
                "factor-3 mismatch for {g:?} (tc={tc})"
            );
            assert_eq!(
                has_5, !g.q,
                "factor-5 mismatch for {g:?} (tc={tc})"
            );
        }
    }

    // ── Spot checks on tick_count ─────────────────────────────────

    #[test]
    fn tick_count_spot_checks() {
        assert_eq!(Grid::T1.tick_count(), 3840);
        assert_eq!(Grid::T16.tick_count(), 240);
        assert_eq!(Grid::T256.tick_count(), 15);
        assert_eq!(Grid::T2T.tick_count(), 1280);
        assert_eq!(Grid::T16T.tick_count(), 160);
        assert_eq!(Grid::T512T.tick_count(), 5);
        assert_eq!(Grid::T2Q.tick_count(), 768);
        assert_eq!(Grid::T16Q.tick_count(), 96);
        assert_eq!(Grid::T8Q.tick_count(), 192);
        assert_eq!(Grid::T512Q.tick_count(), 3);
        assert_eq!(Grid::T2P.tick_count(), 256);
        assert_eq!(Grid::T16P.tick_count(), 32);
        assert_eq!(Grid::T512P.tick_count(), 1);
    }

    // ── Lattice spot checks (top, bottom) ─────────────────────────

    #[test]
    fn t1_is_top() {
        for g in Grid::ALL {
            assert!(g.ple(&Grid::T1), "{g:?} should be ≤ T1");
        }
    }

    #[test]
    fn t512p_is_bottom() {
        for g in Grid::ALL {
            assert!(Grid::T512P.ple(&g), "T512P should be ≤ {g:?}");
        }
    }

    // ── Lattice ops spot checks ───────────────────────────────────

    #[test]
    fn meet_t16t_t16q_is_t16p() {
        // gcd(160, 96) = 32 = T16P.
        assert_eq!(meet(Grid::T16T, Grid::T16Q), Grid::T16P);
    }

    #[test]
    fn join_t16t_t16q_is_t8() {
        // lcm(160, 96) = 480 = T8.
        assert_eq!(join(Grid::T16T, Grid::T16Q), Grid::T8);
    }

    #[test]
    fn meet_t256t_t256q_is_t256p() {
        // gcd(10, 6) = 2 = T256P.
        assert_eq!(meet(Grid::T256T, Grid::T256Q), Grid::T256P);
    }

    #[test]
    fn join_t1_t2_is_t1() {
        assert_eq!(join(Grid::T1, Grid::T2), Grid::T1);
    }

    // ── Heyting / co-Heyting spot checks ────────────────────────────

    /// h8: neg boundary values
    #[test]
    fn h8_neg_boundary() {
        assert_eq!(neg(Grid::T512P), Grid::T1);
        assert_eq!(neg(Grid::T1), Grid::T512P);
    }

    /// c8: coneg boundary values
    #[test]
    fn c8_coneg_boundary() {
        assert_eq!(coneg(Grid::T512P), Grid::T1);
        assert_eq!(coneg(Grid::T1), Grid::T512P);
    }

    #[test]
    fn coneg_t16_is_t2p() {
        assert_eq!(coneg(Grid::T16), Grid::T2P);
    }

    #[test]
    fn coneg_t16t_is_t2q() {
        assert_eq!(coneg(Grid::T16T), Grid::T2Q);
    }

    /// Non-Boolean witness: neg ≠ coneg for T16.
    #[test]
    fn not_boolean_neg_ne_coneg() {
        assert_ne!(neg(Grid::T16), coneg(Grid::T16));
    }

    /// Non-Boolean witness: double neg ≠ identity for T16.
    #[test]
    fn not_boolean_double_neg_ne_id() {
        assert_ne!(neg(neg(Grid::T16)), Grid::T16);
    }

    /// Non-Boolean witness: excluded middle fails for neg.
    #[test]
    fn not_boolean_mid_ne_top() {
        assert_ne!(mid(Grid::T16), Grid::T1);
    }

    // ── Display / FromStr ─────────────────────────────────────────

    #[test]
    fn display_round_trip_all() {
        for g in Grid::ALL {
            let s = g.to_string();
            let parsed: Grid = s.parse().unwrap_or_else(|e| {
                panic!("failed to parse {s:?} (from {g:?}): {e}");
            });
            assert_eq!(parsed, g, "round trip failed for {g:?} ↔ {s:?}");
        }
    }

    #[test]
    fn display_spot_checks() {
        assert_eq!(Grid::T1.to_string(), "t1");
        assert_eq!(Grid::T16.to_string(), "t16");
        assert_eq!(Grid::T16T.to_string(), "t16t");
        assert_eq!(Grid::T8Q.to_string(), "t8q");
        assert_eq!(Grid::T2P.to_string(), "t2p");
        assert_eq!(Grid::T512P.to_string(), "t512p");
    }

    #[test]
    fn fromstr_accepts_uppercase() {
        assert_eq!("T16Q".parse::<Grid>().unwrap(), Grid::T16Q);
        assert_eq!("T2P".parse::<Grid>().unwrap(), Grid::T2P);
    }

    #[test]
    fn fromstr_rejects_garbage() {
        assert!("whatever".parse::<Grid>().is_err());
        assert!("t3".parse::<Grid>().is_err());
        assert!("t1t".parse::<Grid>().is_err()); // would shift to plan_exp = 0 - 1
        assert!("".parse::<Grid>().is_err());
    }

    // ── Property tests ────────────────────────────────────────────

    proptest! {
        #[test]
        fn ple_reflexive(a in arb_grid()) {
            prop_assert!(a.ple(&a));
        }

        #[test]
        fn ple_antisymmetric(a in arb_grid(), b in arb_grid()) {
            if a.ple(&b) && b.ple(&a) {
                prop_assert_eq!(a, b);
            }
        }

        #[test]
        fn ple_transitive(a in arb_grid(), b in arb_grid(), c in arb_grid()) {
            if a.ple(&b) && b.ple(&c) {
                prop_assert!(a.ple(&c));
            }
        }

        /// Meet equals GCD on tick counts.
        #[test]
        fn meet_is_gcd(a in arb_grid(), b in arb_grid()) {
            prop_assert_eq!(
                meet(a, b).tick_count(),
                gcd_u32(a.tick_count(), b.tick_count())
            );
        }

        /// Join equals LCM on tick counts.
        #[test]
        fn join_is_lcm(a in arb_grid(), b in arb_grid()) {
            prop_assert_eq!(
                join(a, b).tick_count(),
                lcm_u32(a.tick_count(), b.tick_count())
            );
        }

        #[test]
        fn meet_join_closure(a in arb_grid(), b in arb_grid()) {
            // meet/join return Grid::ALL members by construction;
            // the proptest just exercises every pair to confirm the
            // .expect calls in meet/join never trip.
            let _ = meet(a, b);
            let _ = join(a, b);
        }

        #[test]
        fn meet_commutative(a in arb_grid(), b in arb_grid()) {
            prop_assert_eq!(meet(a, b), meet(b, a));
        }

        #[test]
        fn join_commutative(a in arb_grid(), b in arb_grid()) {
            prop_assert_eq!(join(a, b), join(b, a));
        }

        #[test]
        fn meet_associative(a in arb_grid(), b in arb_grid(), c in arb_grid()) {
            prop_assert_eq!(meet(meet(a, b), c), meet(a, meet(b, c)));
        }

        #[test]
        fn join_associative(a in arb_grid(), b in arb_grid(), c in arb_grid()) {
            prop_assert_eq!(join(join(a, b), c), join(a, join(b, c)));
        }

        #[test]
        fn absorption(a in arb_grid(), b in arb_grid()) {
            prop_assert_eq!(meet(a, join(a, b)), a);
            prop_assert_eq!(join(a, meet(a, b)), a);
        }

        /// Distributivity: `a ∧ (b ∨ c) = (a ∧ b) ∨ (a ∧ c)`.
        #[test]
        fn distributive_lattice(
            a in arb_grid(), b in arb_grid(), c in arb_grid(),
        ) {
            prop_assert_eq!(
                meet(a, join(b, c)),
                join(meet(a, b), meet(a, c))
            );
        }

        // ── Heyting (imply / neg / mid) ──────────────────────────

        /// h0: adjunction — meet(x, y) ⊑ z ⟺ x ⊑ imply(y, z)
        #[test]
        fn h0_imply_adjunction(
            x in arb_grid(), y in arb_grid(), z in arb_grid(),
        ) {
            prop_assert_eq!(
                meet(x, y).ple(&z),
                x.ple(&imply(y, z))
            );
        }

        /// h1: imply monotone in 2nd arg under join
        #[test]
        fn h1_imply_monotone_join_2nd(
            x in arb_grid(), y in arb_grid(), z in arb_grid(),
        ) {
            prop_assert!(imply(x, y).ple(&imply(x, join(y, z))));
        }

        /// h2: imply antitone in 1st arg under join
        #[test]
        fn h2_imply_antitone_join_1st(
            x in arb_grid(), y in arb_grid(), z in arb_grid(),
        ) {
            prop_assert!(imply(join(x, z), y).ple(&imply(x, y)));
        }

        /// h3: imply monotone in 2nd arg under ple
        #[test]
        fn h3_imply_monotone_ple_2nd(
            x in arb_grid(), y in arb_grid(), z in arb_grid(),
        ) {
            if x.ple(&y) {
                prop_assert!(imply(z, x).ple(&imply(z, y)));
            }
        }

        /// h4: currying
        #[test]
        fn h4_imply_currying(
            x in arb_grid(), y in arb_grid(), z in arb_grid(),
        ) {
            prop_assert_eq!(imply(meet(x, y), z), imply(x, imply(y, z)));
        }

        /// h5: imply distributes over meet
        #[test]
        fn h5_imply_distributes_over_meet(
            x in arb_grid(), y in arb_grid(), z in arb_grid(),
        ) {
            prop_assert_eq!(
                imply(x, meet(y, z)),
                meet(imply(x, y), imply(x, z))
            );
        }

        /// h6: weakening
        #[test]
        fn h6_weakening(x in arb_grid(), y in arb_grid()) {
            prop_assert!(y.ple(&imply(x, meet(x, y))));
        }

        /// h7: modus ponens
        #[test]
        fn h7_modus_ponens(x in arb_grid(), y in arb_grid()) {
            prop_assert_eq!(meet(x, imply(x, y)), meet(x, y));
        }

        /// h9: neg-join ≤ imply
        #[test]
        fn h9_neg_join_le_imply(x in arb_grid(), y in arb_grid()) {
            prop_assert!(join(neg(x), y).ple(&imply(x, y)));
        }

        /// h10: imply = top iff ple
        #[test]
        fn h10_imply_top_iff_ple(x in arb_grid(), y in arb_grid()) {
            prop_assert_eq!(x.ple(&y), imply(x, y) == Grid::T1);
        }

        /// h11: neg antitone under join
        #[test]
        fn h11_neg_antitone_join(x in arb_grid(), y in arb_grid()) {
            prop_assert!(neg(join(x, y)).ple(&neg(x)));
        }

        /// h12: neg-imply de Morgan
        #[test]
        fn h12_neg_imply_de_morgan(x in arb_grid(), y in arb_grid()) {
            prop_assert_eq!(neg(imply(x, y)), meet(neg(neg(x)), neg(y)));
        }

        /// h13: neg-join de Morgan
        #[test]
        fn h13_neg_join_de_morgan(x in arb_grid(), y in arb_grid()) {
            prop_assert_eq!(neg(join(x, y)), meet(neg(x), neg(y)));
        }

        /// h14: non-contradiction
        #[test]
        fn h14_non_contradiction(x in arb_grid()) {
            prop_assert_eq!(meet(x, neg(x)), Grid::T512P);
        }

        /// h15: triple neg = neg
        #[test]
        fn h15_triple_neg(x in arb_grid()) {
            prop_assert_eq!(neg(neg(neg(x))), neg(x));
        }

        /// h16: double neg excluded middle
        #[test]
        fn h16_double_neg_mid(x in arb_grid()) {
            prop_assert_eq!(neg(neg(mid(x))), Grid::T1);
        }

        /// h17: double neg monad
        #[test]
        fn h17_double_neg_monad(x in arb_grid()) {
            prop_assert!(x.ple(&neg(neg(x))));
        }

        // ── Co-Heyting (coimp / coneg / comid) ───────────────────

        /// c0: adjunction — coimp(x, y) ⊑ z ⟺ x ⊑ join(y, z)
        #[test]
        fn c0_coimp_adjunction(
            x in arb_grid(), y in arb_grid(), z in arb_grid(),
        ) {
            prop_assert_eq!(
                coimp(x, y).ple(&z),
                x.ple(&join(y, z))
            );
        }

        /// c1: coimp monotone (meet in 1st arg)
        #[test]
        fn c1_coimp_monotone_meet_1st(
            x in arb_grid(), y in arb_grid(), z in arb_grid(),
        ) {
            prop_assert!(coimp(meet(x, z), y).ple(&coimp(x, y)));
        }

        /// c2: coimp antitone (meet in 2nd arg)
        #[test]
        fn c2_coimp_antitone_meet_2nd(
            x in arb_grid(), y in arb_grid(), z in arb_grid(),
        ) {
            prop_assert!(coimp(x, y).ple(&coimp(x, meet(y, z))));
        }

        /// c3: coimp monotone (ple in 1st arg)
        #[test]
        fn c3_coimp_monotone_ple_1st(
            x in arb_grid(), y in arb_grid(), z in arb_grid(),
        ) {
            if y.ple(&x) {
                prop_assert!(coimp(y, z).ple(&coimp(x, z)));
            }
        }

        /// c4: co-currying
        #[test]
        fn c4_coimp_co_currying(
            x in arb_grid(), y in arb_grid(), z in arb_grid(),
        ) {
            prop_assert_eq!(coimp(z, join(x, y)), coimp(coimp(z, x), y));
        }

        /// c5: coimp distributes over join
        #[test]
        fn c5_coimp_distributes_over_join(
            x in arb_grid(), y in arb_grid(), z in arb_grid(),
        ) {
            prop_assert_eq!(
                coimp(join(y, z), x),
                join(coimp(y, x), coimp(z, x))
            );
        }

        /// c6: coimp ≤ self
        #[test]
        fn c6_coimp_le_self(x in arb_grid(), y in arb_grid()) {
            prop_assert!(coimp(x, y).ple(&x));
        }

        /// c7: join absorption
        #[test]
        fn c7_join_absorption(x in arb_grid(), y in arb_grid()) {
            prop_assert_eq!(join(x, coimp(y, x)), join(x, y));
        }

        /// c9: meet-coneg ≥ coimp
        #[test]
        fn c9_meet_coneg_ge_coimp(x in arb_grid(), y in arb_grid()) {
            prop_assert!(coimp(x, y).ple(&meet(x, coneg(y))));
        }

        /// c10: coimp = bottom iff ple
        #[test]
        fn c10_coimp_bot_iff_ple(x in arb_grid(), y in arb_grid()) {
            prop_assert_eq!(y.ple(&x), coimp(y, x) == Grid::T512P);
        }

        /// c11: coneg antitone under meet
        #[test]
        fn c11_coneg_antitone_meet(x in arb_grid(), y in arb_grid()) {
            prop_assert!(coneg(x).ple(&coneg(meet(x, y))));
        }

        /// c12: coneg-coimp de Morgan
        #[test]
        fn c12_coneg_coimp_de_morgan(x in arb_grid(), y in arb_grid()) {
            prop_assert_eq!(
                coneg(coimp(y, x)),
                join(coneg(coneg(x)), coneg(y))
            );
        }

        /// c13: coneg-meet de Morgan
        #[test]
        fn c13_coneg_meet_de_morgan(x in arb_grid(), y in arb_grid()) {
            prop_assert_eq!(coneg(meet(x, y)), join(coneg(x), coneg(y)));
        }

        /// c14: excluded middle
        #[test]
        fn c14_excluded_middle(x in arb_grid()) {
            prop_assert_eq!(join(x, coneg(x)), Grid::T1);
        }

        /// c15: triple coneg = coneg
        #[test]
        fn c15_triple_coneg(x in arb_grid()) {
            prop_assert_eq!(coneg(coneg(coneg(x))), coneg(x));
        }

        /// c16: double coneg comid
        #[test]
        fn c16_double_coneg_comid(x in arb_grid()) {
            prop_assert_eq!(coneg(coneg(comid(x))), Grid::T512P);
        }

        /// c17: double coneg comonad
        #[test]
        fn c17_double_coneg_comonad(x in arb_grid()) {
            prop_assert!(coneg(coneg(x)).ple(&x));
        }

        /// c18: comid decomposition
        #[test]
        fn c18_comid_decomposition(x in arb_grid()) {
            prop_assert_eq!(x, join(comid(x), coneg(coneg(x))));
        }

        /// c19: Leibniz rule
        #[test]
        fn c19_leibniz(x in arb_grid(), y in arb_grid()) {
            prop_assert_eq!(
                comid(meet(x, y)),
                join(meet(comid(x), y), meet(x, comid(y)))
            );
        }

        /// c20: comid additivity
        #[test]
        fn c20_comid_additivity(x in arb_grid(), y in arb_grid()) {
            prop_assert_eq!(
                join(comid(join(x, y)), comid(meet(x, y))),
                join(comid(x), comid(y))
            );
        }

        // ── Bi-Heyting ──────────────────────────────────────────

        /// s1: neg ≤ coneg
        #[test]
        fn s1_neg_le_coneg(x in arb_grid()) {
            prop_assert!(neg(x).ple(&coneg(x)));
        }

        #[test]
        fn lattice_top_bottom(a in arb_grid()) {
            prop_assert!(a.ple(&Grid::T1));
            prop_assert!(Grid::T512P.ple(&a));
        }
    }

    // ── helpers ──────────────────────────────────────────────────

    fn gcd_u32(mut a: u32, mut b: u32) -> u32 {
        while b != 0 {
            let t = b;
            b = a % b;
            a = t;
        }
        a
    }

    fn lcm_u32(a: u32, b: u32) -> u32 {
        if a == 0 || b == 0 {
            0
        } else {
            a / gcd_u32(a, b) * b
        }
    }
}
