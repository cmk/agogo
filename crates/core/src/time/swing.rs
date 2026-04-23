//! Integer-valued swing and alignment.
//!
//! Port of `SwingConfig`, `isSwungStep`, `effectiveTick`, and
//! `isAligned` from the Haskell Cirklon source. Integer-valued
//! (not float) to avoid FP drift in property tests.
//!
//! **Plan deviation: `is_swung_step` takes only a `Tick`.** The plan
//! specified `fn is_swung_step(cfg: &SwingConfig, t: Tick) -> bool`,
//! but the Haskell function doesn't consume the config — off-beat
//! status is determined purely by the T16 step-index parity. We match
//! Haskell. Recorded in the plan's Review section.
//!
//! **Haskell swing is one-sided (always subtracts).** Off-beat ticks
//! are shifted by `-amount * multiplier`; on-beat ticks pass through.
//! The plan's `swing_zero_mean_over_beat` property assumed bidirectional
//! swing (sum-to-zero across one beat) and does *not* hold for this
//! semantics. The property is `#[ignore]`d below with a Review note.

use crate::time::tbase::TBase;
use crate::time::tick::Tick;

/// Swing configuration.
///
/// `amount` is typically in `0..=16`; `multiplier` is the number of
/// master ticks per swing unit. Total displacement in ticks is
/// `amount * multiplier`. Negative values flip the sign of the
/// displacement.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct SwingConfig {
    pub amount: i32,
    pub multiplier: i32,
}

impl SwingConfig {
    /// Total displacement in ticks. `i64` so `amount * multiplier`
    /// cannot overflow for any `i32` inputs.
    pub const fn displacement(&self) -> i64 {
        self.amount as i64 * self.multiplier as i64
    }
}

/// Is the tick in an off-beat 16th-note subdivision? Off-beats are
/// the odd T16 step-index positions (1, 3, 5, …).
///
/// Operates on the floor T16 step index: for any tick `t`,
/// `is_swung_step(t) = (t / 48) % 2 == 1`, so unaligned ticks pick up
/// the parity of the T16 region they fall in.
pub fn is_swung_step(t: Tick) -> bool {
    (t.0 / TBase::T16.tick_count()) % 2 == 1
}

/// Tick after applying the swing offset. Off-beats shift by
/// `-amount * multiplier` ticks; other ticks pass through.
///
/// Saturates at 0 if the shift would underflow, and at `u32::MAX` if
/// it would overflow — both are out-of-range for any musical context,
/// so property tests that bound inputs never exercise the saturation.
pub fn effective_tick(cfg: &SwingConfig, t: Tick) -> Tick {
    if !is_swung_step(t) {
        return t;
    }
    let d = cfg.displacement();
    let shifted = i64::from(t.0) - d;
    Tick(shifted.clamp(0, i64::from(u32::MAX)) as u32)
}

/// Is the tick aligned to the `base` grid?
pub fn is_aligned(t: Tick, base: TBase) -> bool {
    t.0 % base.tick_count() == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arb::{arb_swing, arb_tbase, arb_tick};
    use proptest::prelude::*;

    // ── Spot checks on is_swung_step ──────────────────────────────

    #[test]
    fn is_swung_step_zero_is_even() {
        assert!(!is_swung_step(Tick(0)));
    }

    #[test]
    fn is_swung_step_t16_boundaries() {
        // Steps 0..4 across one beat (192 ticks): even → not swung,
        // odd → swung.
        assert!(!is_swung_step(Tick(0))); // step 0
        assert!(is_swung_step(Tick(48))); // step 1
        assert!(!is_swung_step(Tick(96))); // step 2
        assert!(is_swung_step(Tick(144))); // step 3
    }

    #[test]
    fn is_swung_step_picks_up_floor_region() {
        // 50 ticks falls in step 1 (50/48 = 1) → swung.
        assert!(is_swung_step(Tick(50)));
        // 47 ticks falls in step 0 → not swung.
        assert!(!is_swung_step(Tick(47)));
    }

    // ── Spot checks on is_aligned ─────────────────────────────────

    #[test]
    fn is_aligned_t16_48_true() {
        assert!(is_aligned(Tick(48), TBase::T16));
    }

    #[test]
    fn is_aligned_t16_50_false() {
        assert!(!is_aligned(Tick(50), TBase::T16));
    }

    #[test]
    fn is_aligned_t1_only_multiples_of_768() {
        assert!(is_aligned(Tick(0), TBase::T1));
        assert!(is_aligned(Tick(768), TBase::T1));
        assert!(!is_aligned(Tick(384), TBase::T1));
    }

    // ── Spot checks on effective_tick ────────────────────────────

    #[test]
    fn effective_tick_on_beat_is_identity() {
        let cfg = SwingConfig {
            amount: 8,
            multiplier: 2,
        };
        assert_eq!(effective_tick(&cfg, Tick(0)), Tick(0));
        assert_eq!(effective_tick(&cfg, Tick(96)), Tick(96));
    }

    #[test]
    fn effective_tick_off_beat_shifts() {
        let cfg = SwingConfig {
            amount: 3,
            multiplier: 4,
        };
        // displacement = 12 ticks. Off-beat at 48 → 48 - 12 = 36.
        assert_eq!(effective_tick(&cfg, Tick(48)), Tick(36));
    }

    #[test]
    fn effective_tick_saturates_on_underflow() {
        let cfg = SwingConfig {
            amount: 100,
            multiplier: 100,
        };
        // displacement = 10_000 > 48, saturates at 0.
        assert_eq!(effective_tick(&cfg, Tick(48)), Tick(0));
    }

    #[test]
    fn effective_tick_negative_amount_shifts_forward() {
        let cfg = SwingConfig {
            amount: -3,
            multiplier: 4,
        };
        // displacement = -12, off-beat at 48 → 48 - (-12) = 60.
        assert_eq!(effective_tick(&cfg, Tick(48)), Tick(60));
    }

    // ── Property tests ───────────────────────────────────────────

    proptest! {
        /// `amount = 0` (any multiplier) is the identity.
        #[test]
        fn swing_identity_when_amount_zero(
            mult in -16i32..=16, t in arb_tick(),
        ) {
            let cfg = SwingConfig { amount: 0, multiplier: mult };
            prop_assert_eq!(effective_tick(&cfg, t), t);
        }

        /// `effective_tick` is the identity on non-off-beat ticks for
        /// any config.
        #[test]
        fn swing_only_affects_off_beats(cfg in arb_swing(), t in arb_tick()) {
            if !is_swung_step(t) {
                prop_assert_eq!(effective_tick(&cfg, t), t);
            }
        }

        /// On off-beats, the displacement is exactly `-amount * multiplier`
        /// (when the shift stays within `u32` bounds — the property
        /// constrains inputs so it always does).
        #[test]
        fn swing_displacement_on_off_beats(
            cfg in arb_swing(),
            t in (1u32..=1_000_000).prop_map(Tick),
        ) {
            if is_swung_step(t) {
                let d = cfg.displacement();
                // Skip edges where saturation would kick in: keep |d|
                // well inside u32 range from t.0.
                let shifted = i64::from(t.0) - d;
                if (0..=i64::from(u32::MAX)).contains(&shifted) {
                    prop_assert_eq!(effective_tick(&cfg, t), Tick(shifted as u32));
                }
            }
        }

        /// `is_aligned` matches direct tick-count divisibility.
        #[test]
        fn is_aligned_matches_tick_count_mod(
            t in arb_tick(), base in arb_tbase(),
        ) {
            prop_assert_eq!(is_aligned(t, base), t.0 % base.tick_count() == 0);
        }

        /// Guarded alignment invariant: when a tick is aligned to `base`
        /// *and* the swing displacement is a multiple of `base`'s tick
        /// count, the swung tick is still aligned to `base`. This is
        /// the restricted form of the plan's
        /// `swing_is_aligned_invariant` that holds under the Haskell
        /// one-sided swing semantics.
        #[test]
        fn swing_preserves_alignment_when_displacement_divides_base(
            cfg in arb_swing(),
            base in arb_tbase(),
            k in 0u32..=10_000,
        ) {
            let t = Tick(k * base.tick_count());
            let d = cfg.displacement();
            let tc = i64::from(base.tick_count());
            if d.rem_euclid(tc) == 0 {
                let shifted_in_range = {
                    let s = i64::from(t.0) - d;
                    (0..=i64::from(u32::MAX)).contains(&s)
                };
                if shifted_in_range {
                    prop_assert!(is_aligned(effective_tick(&cfg, t), base));
                }
            }
        }

        /// Unguarded alignment invariant (the plan's original
        /// formulation): `is_aligned(effective_tick(t), base) =
        /// is_aligned(t, base)` for grid-aligned `t`. This only holds
        /// when displacement happens to be aligned to `base`, and the
        /// test confirms the contrapositive by construction — it's a
        /// sanity test showing the guarded version above is necessary.
        #[test]
        fn swing_alignment_can_break_without_displacement_guard(
            cfg in arb_swing(),
            base in arb_tbase(),
            k in 0u32..=1_000,
        ) {
            let t = Tick(k * base.tick_count());
            let tc = i64::from(base.tick_count());
            let d = cfg.displacement();
            let s = i64::from(t.0) - d;
            if is_swung_step(t) && d.rem_euclid(tc) != 0
                && (0..=i64::from(u32::MAX)).contains(&s)
            {
                // displacement doesn't divide base → alignment breaks
                prop_assert!(!is_aligned(effective_tick(&cfg, t), base));
            }
        }
    }

    // ── #[ignore]'d plan property with re-enablement plan ────────

    /// Plan property `swing_zero_mean_over_beat`. **Deferred.**
    ///
    /// The plan specifies: `sum of (effective_tick(t) - t) across one
    /// beat = 0 for every SwingConfig`. Under Haskell's one-sided
    /// swing (always subtract on off-beats), the sum across one beat
    /// at T16 grid = `-2 * amount * multiplier`, which is non-zero in
    /// general.
    ///
    /// To re-enable, swing would need to become bidirectional
    /// (alternating ± on consecutive off-beats, so they cancel
    /// pairwise). That is an API/semantics change and is out of scope
    /// for this sprint.
    #[test]
    #[ignore = "plan property assumes bidirectional swing; Haskell swing is one-sided"]
    fn swing_zero_mean_over_beat() {
        let cfg = SwingConfig {
            amount: 5,
            multiplier: 3,
        };
        let beat_steps = [Tick(0), Tick(48), Tick(96), Tick(144)];
        let total: i64 = beat_steps
            .iter()
            .map(|&t| i64::from(effective_tick(&cfg, t).0) - i64::from(t.0))
            .sum();
        assert_eq!(total, 0);
    }
}
