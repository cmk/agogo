//! `Channel` configuration + the pure transform pipeline.
//!
//! Pipeline stages, applied in order (agogo.md §6):
//! 1. **Divide** — keep only master ticks divisible by
//!    `channel.divider.tick_count()`.
//! 2. **Shuffle** — apply [`swing::effective_tick`] (off-beats shift
//!    earlier by `amount × multiplier`; on-beats pass through).
//! 3. **Tick → Sample** via [`SampleTickConn::inner`].
//! 4. **Shift** — add `clamp(shift, 0, MAX_SHIFT)` → Pico → Sample
//!    via `F12F06 ∘ pico_to_samples`. Plan 03 does not implement
//!    negative shift (needs a forward-look ring buffer, deferred to
//!    v0.2).
//! 5. **Offset** — same composition chain for the signed calibration
//!    offset.

use crate::channel::mode::ChannelMode;
use crate::fxp::pico_to_samples;
use crate::time::conn::SampleTickConn;
use crate::time::swing::{self, SwingConfig};
use crate::time::tbase::TBase;
use crate::time::tick::Tick;
use connections::conn::fixed::{F12F06, Micro};

/// Maximum positive shift before saturation: 300 ms = 300 000 µs.
pub const MAX_SHIFT: Micro = Micro(300_000);

/// Per-channel configuration.
#[derive(Copy, Clone, Debug)]
pub struct Channel {
    pub mode: ChannelMode,
    /// Divider expressed as the `TBase` whose tick count is the
    /// channel's step (agogo.md §6 mapping). E.g. `TBase::T16` fires
    /// 16th notes, `TBase::T4` fires quarter notes.
    pub divider: TBase,
    pub shuffle: SwingConfig,
    /// Positive-only shift, clamped to `[Micro::ZERO, MAX_SHIFT]` on
    /// use. v0.1 does not implement negative shift.
    pub shift: Micro,
    /// Signed calibration offset. Not clamped here — CLI / UI should
    /// pick a musical range (agogo.md §6 cites ±5 ms = ±5 000 µs).
    pub offset: Micro,
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

/// Convert a `Micro` offset into a whole-sample count at `sr` via
/// the adjoint-law composition `F12F06 ∘ pico_to_samples`. Shared
/// by `transform` and `scheduler`.
///
/// Panics if `sr` isn't one of the six supported rates (same set as
/// `pico_to_samples`); `SampleTickConn::new` already enforces a
/// matching invariant upstream of every caller, so this panic is
/// unreachable in practice.
pub(crate) fn micro_to_samples(m: Micro, sr: u32) -> i64 {
    let pico = F12F06.inner(m);
    pico_to_samples(pico, sr).unwrap_or_else(|| {
        panic!("channel: unsupported sample rate {sr} (expected 44_100 / 48_000 / 88_200 / 96_000 / 176_400 / 192_000)")
    })
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
    let shift_clamped = Micro(channel.shift.0.clamp(0, MAX_SHIFT.0));
    let shift_samples = micro_to_samples(shift_clamped, stc.sr()).max(0) as u64;
    let offset_samples = micro_to_samples(channel.offset, stc.sr());

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
        SampleTickConn::new(48_000, crate::fxp::Tempo::from_bpm_integer(120), 192)
    }

    fn zero_channel(divider: TBase) -> Channel {
        Channel {
            mode: ChannelMode::MidiClock,
            divider,
            shuffle: SwingConfig {
                amount: 0,
                multiplier: 1,
            },
            shift: Micro::ZERO,
            offset: Micro::ZERO,
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
        // Plan spot check: shift = 10 ms at 48 kHz → +480 samples.
        let mut ch = zero_channel(TBase::T4);
        ch.shift = Micro(10_000); // 10 ms
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
        ch.shift = Micro(1_000_000); // 1 s
        let ev = transform([Tick(0)], &ch, &stc_120_48k());
        // Clamped to 300 ms → 14 400 samples at 48 kHz.
        assert_eq!(ev[0].sample_index, 14_400);
    }

    #[test]
    fn negative_shift_clamped_to_zero() {
        let mut ch = zero_channel(TBase::T4);
        ch.shift = Micro(-100_000); // -100 ms
        let ev = transform([Tick(0), Tick(192)], &ch, &stc_120_48k());
        assert_eq!(ev[0].sample_index, 0);
        assert_eq!(ev[1].sample_index, 24_000);
    }

    #[test]
    fn offset_negative_shifts_earlier() {
        let mut ch = zero_channel(TBase::T4);
        ch.offset = Micro(-1_000); // -1 ms = -48 samples at 48 kHz.
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
            shift_us in 0_i64..=MAX_SHIFT.0,
            offset_us in -5_000_i64..=5_000,
            max_tick in 192u32..=5_000,
        ) {
            let ch = Channel {
                mode: ChannelMode::MidiClock,
                divider,
                shuffle,
                shift: Micro(shift_us),
                offset: Micro(offset_us),
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

        /// Plan property `shift_clamping`, upper bound: shift >
        /// MAX_SHIFT saturates at MAX_SHIFT; the event sample is
        /// exactly `stc.inner(tick) + micro_to_samples(MAX_SHIFT, sr)`.
        #[test]
        fn shift_upper_clamp(shift_us in MAX_SHIFT.0..=10_000_000_i64) {
            let mut ch = zero_channel(TBase::T4);
            ch.shift = Micro(shift_us);
            let stc = stc_120_48k();
            let ev = transform([Tick(192)], &ch, &stc);
            let cap_samples = super::micro_to_samples(MAX_SHIFT, stc.sr()) as u64;
            prop_assert_eq!(ev[0].sample_index, 24_000 + cap_samples);
        }

        /// Plan property `shift_clamping`, lower bound: shift < 0
        /// saturates at 0; the event sample matches a zero-shift run.
        #[test]
        fn shift_lower_clamp(shift_us in -10_000_000_i64..0) {
            let mut ch = zero_channel(TBase::T4);
            ch.shift = Micro(shift_us);
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
