//! `Channel` configuration + the pure transform pipeline.
//!
//! Pipeline stages, applied in order (agogo.md §6):
//! 1. **Divide** — keep only master ticks divisible by
//!    `channel.divider.tick_count()`.
//! 2. **Shuffle** — apply [`swing::effective_tick`] (off-beats shift
//!    earlier by `amount × multiplier`; on-beats pass through).
//! 3. **Tick → Sample** via [`SampleTickConn::inner`].
//! 4. **Shift** — add `clamp(shift_ms, 0, 300) × sr / 1000` samples.
//!    Plan 03 does not implement negative shift (needs a forward-look
//!    ring buffer, deferred to v0.2).
//! 5. **Offset** — add `offset_ms × sr / 1000` samples (signed; for
//!    per-channel calibration against downstream latency).

use crate::channel::mode::ChannelMode;
use crate::time::conn::SampleTickConn;
use crate::time::swing::{self, SwingConfig};
use crate::time::tbase::TBase;
use crate::time::tick::Tick;

/// Maximum positive shift, in milliseconds, before saturation.
pub const MAX_SHIFT_MS: f32 = 300.0;

/// Per-channel configuration.
#[derive(Copy, Clone, Debug)]
pub struct Channel {
    pub mode: ChannelMode,
    /// Divider expressed as the `TBase` whose tick count is the
    /// channel's step (agogo.md §6 mapping). E.g. `TBase::T16` fires
    /// 16th notes, `TBase::T4` fires quarter notes.
    pub divider: TBase,
    pub shuffle: SwingConfig,
    /// Positive-only shift in ms, clamped to `[0, MAX_SHIFT_MS]` on
    /// use. v0.1 does not implement negative shift.
    pub shift_ms: f32,
    /// Signed calibration offset in ms. Not clamped here — CLI / UI
    /// should pick a musical range (agogo.md §6 cites ±5 ms).
    pub offset_ms: f32,
}

/// A master-tick-driven event scheduled at a specific sample index.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct ScheduledEvent {
    pub sample_index: u64,
    /// The post-shuffle tick that produced this event. Useful for
    /// downstream bookkeeping (e.g. pairing with a MIDI clock byte
    /// counter).
    pub tick: Tick,
}

/// Run the divider → shuffle → sample → shift → offset pipeline over
/// a master tick stream. Pure: output order matches input order and
/// no I/O is performed.
pub fn transform(
    master_ticks: impl IntoIterator<Item = Tick>,
    channel: &Channel,
    stc: &SampleTickConn,
) -> Vec<ScheduledEvent> {
    let divisor = channel.divider.tick_count();
    let shift_ms = channel.shift_ms.clamp(0.0, MAX_SHIFT_MS);
    let sr_f = stc.sr() as f32;
    let shift_samples = (shift_ms * sr_f / 1000.0).round() as u64;
    let offset_samples = (channel.offset_ms * sr_f / 1000.0).round() as i64;

    master_ticks
        .into_iter()
        .filter(|t| t.0 % divisor == 0)
        .map(|t| {
            let swung = swing::effective_tick(&channel.shuffle, t);
            let base = stc.inner(swung);
            let with_shift = base.saturating_add(shift_samples);
            let final_sample = if offset_samples >= 0 {
                with_shift.saturating_add(offset_samples as u64)
            } else {
                with_shift.saturating_sub(offset_samples.unsigned_abs())
            };
            ScheduledEvent {
                sample_index: final_sample,
                tick: swung,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arb::arb_tbase;
    use proptest::prelude::*;

    fn stc_120_48k() -> SampleTickConn {
        SampleTickConn::new(48_000, 120.0, 192)
    }

    fn zero_channel(divider: TBase) -> Channel {
        Channel {
            mode: ChannelMode::MidiClock,
            divider,
            shuffle: SwingConfig {
                amount: 0,
                multiplier: 1,
            },
            shift_ms: 0.0,
            offset_ms: 0.0,
        }
    }

    // ── Spot checks ──────────────────────────────────────────────

    #[test]
    fn t4_at_120bpm_48k_emits_at_half_second_multiples() {
        // Plan spot check: divider T4 (quarter), shuffle 0, shift 0,
        // offset 0 → samples 0, 24 000, 48 000, …
        let ch = zero_channel(TBase::T4);
        let master: Vec<Tick> = (0..=768).map(Tick).collect();
        let ev = transform(master, &ch, &stc_120_48k());
        let samples: Vec<u64> = ev.iter().map(|e| e.sample_index).collect();
        assert_eq!(samples, vec![0, 24_000, 48_000, 72_000, 96_000]);
    }

    #[test]
    fn shift_10ms_adds_exactly_480_samples() {
        // Plan spot check: shift_ms = 10.0 at 48 kHz → +480 samples.
        let mut ch = zero_channel(TBase::T4);
        ch.shift_ms = 10.0;
        let ev = transform([Tick(0), Tick(192)], &ch, &stc_120_48k());
        let samples: Vec<u64> = ev.iter().map(|e| e.sample_index).collect();
        assert_eq!(samples, vec![480, 24_480]);
    }

    #[test]
    fn divider_filters_unaligned_ticks() {
        // Divider T4 (192 ticks). Master stream contains every tick in
        // [0, 200]; only 0 and 192 survive the filter.
        let ch = zero_channel(TBase::T4);
        let master: Vec<Tick> = (0..=200).map(Tick).collect();
        let ev = transform(master, &ch, &stc_120_48k());
        let ticks: Vec<u32> = ev.iter().map(|e| e.tick.0).collect();
        assert_eq!(ticks, vec![0, 192]);
    }

    #[test]
    fn shift_over_300ms_saturates() {
        let mut ch = zero_channel(TBase::T4);
        ch.shift_ms = 1_000.0;
        let ev = transform([Tick(0)], &ch, &stc_120_48k());
        // Clamped to 300 ms → 14 400 samples at 48 kHz.
        assert_eq!(ev[0].sample_index, 14_400);
    }

    #[test]
    fn negative_shift_clamped_to_zero() {
        let mut ch = zero_channel(TBase::T4);
        ch.shift_ms = -100.0;
        let ev = transform([Tick(0), Tick(192)], &ch, &stc_120_48k());
        assert_eq!(ev[0].sample_index, 0);
        assert_eq!(ev[1].sample_index, 24_000);
    }

    #[test]
    fn offset_negative_shifts_earlier() {
        let mut ch = zero_channel(TBase::T4);
        ch.offset_ms = -1.0; // -48 samples at 48 kHz.
        let ev = transform([Tick(192)], &ch, &stc_120_48k());
        assert_eq!(ev[0].sample_index, 24_000 - 48);
    }

    // ── Property tests ───────────────────────────────────────────

    /// Dividers coarser than T16 never land on swung off-beats
    /// (`is_swung_step` is keyed to T16 step parity), so monotonicity
    /// is trivial for them. For finer dividers, swing displacement
    /// must stay smaller than the divider's step so swung off-beats
    /// never leap past an adjacent on-beat. This strategy yields a
    /// `(divider, swing)` pair that honours that bound.
    fn arb_divider_with_bounded_swing() -> impl Strategy<Value = (TBase, SwingConfig)> {
        arb_tbase().prop_flat_map(|d| {
            // `multiplier = 1` so `amount` directly bounds displacement.
            let cap = (d.tick_count() as i32 - 1).max(0);
            (
                Just(d),
                (-cap..=cap).prop_map(|amount| SwingConfig {
                    amount,
                    multiplier: 1,
                }),
            )
        })
    }

    proptest! {
        /// Plan property `tick_monotonicity`: per-channel tick stream
        /// is non-decreasing under swings whose displacement is smaller
        /// than the divider's step.
        #[test]
        fn tick_monotonicity(
            (divider, shuffle) in arb_divider_with_bounded_swing(),
            shift_ms in 0.0f32..=MAX_SHIFT_MS,
            offset_ms in -5.0f32..=5.0f32,
            max_tick in 192u32..=5_000,
        ) {
            let ch = Channel {
                mode: ChannelMode::MidiClock,
                divider,
                shuffle,
                shift_ms,
                offset_ms,
            };
            let master: Vec<Tick> = (0..=max_tick).map(Tick).collect();
            let ev = transform(master, &ch, &stc_120_48k());
            for w in ev.windows(2) {
                prop_assert!(
                    w[0].tick <= w[1].tick,
                    "ticks {:?} → {:?} non-monotonic under divider={:?} swing={:?}",
                    w[0].tick, w[1].tick, divider, shuffle,
                );
                prop_assert!(
                    w[0].sample_index <= w[1].sample_index,
                    "events {:?} → {:?} non-monotonic under divider={:?} swing={:?}",
                    w[0], w[1], divider, shuffle,
                );
            }
        }

        /// Plan property `divider_rate_preservation`: a channel with
        /// divider `D` emits exactly the number of `D.tick_count()`
        /// multiples that fall inside `[0, beats × 192)`. For dividers
        /// finer than a quarter this is `beats × (192 / tick_count)`;
        /// for dividers coarser (T2, T1) a beat spans a fraction of a
        /// step and `div_ceil` handles the rounding.
        #[test]
        fn divider_rate_preservation(
            divider in arb_tbase(),
            beats in 1u32..=16,
        ) {
            let ch = zero_channel(divider);
            let span = beats * 192;
            // 0..span (half-open: span itself belongs to the next beat).
            let master: Vec<Tick> = (0..span).map(Tick).collect();
            let ev = transform(master, &ch, &stc_120_48k());
            let expected = span.div_ceil(divider.tick_count());
            prop_assert_eq!(ev.len() as u32, expected);
        }

        /// Plan property `shift_clamping`, upper bound: shift_ms >
        /// MAX_SHIFT_MS saturates at MAX_SHIFT_MS; the event sample is
        /// exactly `stc.inner(tick) + round(MAX_SHIFT_MS * sr / 1000)`.
        #[test]
        fn shift_upper_clamp(shift_ms in MAX_SHIFT_MS..=10_000.0f32) {
            let mut ch = zero_channel(TBase::T4);
            ch.shift_ms = shift_ms;
            let stc = stc_120_48k();
            let ev = transform([Tick(192)], &ch, &stc);
            let cap_samples = (MAX_SHIFT_MS * stc.sr() as f32 / 1000.0).round() as u64;
            prop_assert_eq!(ev[0].sample_index, 24_000 + cap_samples);
        }

        /// Plan property `shift_clamping`, lower bound: shift_ms < 0
        /// saturates at 0; the event sample matches a zero-shift run.
        #[test]
        fn shift_lower_clamp(shift_ms in -10_000.0f32..0.0) {
            let mut ch = zero_channel(TBase::T4);
            ch.shift_ms = shift_ms;
            let ev = transform([Tick(192)], &ch, &stc_120_48k());
            prop_assert_eq!(ev[0].sample_index, 24_000);
        }

        /// Replacement for the plan's `shuffle_zero_mean_per_beat`:
        /// that property cannot hold under Haskell's one-sided swing
        /// semantics (see `time/swing.rs` module doc and the existing
        /// `swing_zero_mean_over_beat` ignored test). The weaker
        /// invariant that does hold: swing is identity on even-parity
        /// T16 steps. The plan's spirit is preserved — off-beat shifts
        /// are the only effect of the shuffle stage.
        #[test]
        fn shuffle_identity_on_even_steps(
            swing_amount in -16i32..=16,
            swing_mult in 1i32..=8,
        ) {
            let mut ch = zero_channel(TBase::T16);
            ch.shuffle = SwingConfig {
                amount: swing_amount,
                multiplier: swing_mult,
            };
            let stc = stc_120_48k();
            // Even T16 steps (0, 96, 192, 288) — all `is_swung_step ==
            // false`. For each, transform's tick == original tick.
            for step in [0u32, 96, 192, 288] {
                let ev = transform([Tick(step)], &ch, &stc);
                prop_assert_eq!(ev[0].tick, Tick(step));
            }
        }
    }
}
