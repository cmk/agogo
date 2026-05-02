//! Per-audio-buffer scheduler.
//!
//! Given a `Channel` and a `SampleTickConn`, [`tick_stream`] returns
//! the `ScheduledEvent`s whose `sample_index` falls inside the half-
//! open window `[buffer_start_sample, buffer_start_sample + frames)`.
//!
//! The plan's `tick_stream` signature names a `&mut PhaseSource` as
//! its first argument. v0.1 does not read phase here — the tick range
//! is derived algebraically from `SampleTickConn`, and the PLL is
//! driven by the audio callback outside this function (Plan 05). The
//! parameter is omitted rather than kept unused; documented in the
//! sprint's Review section.

use crate::channel::role::ChannelCommon;
use crate::channel::time::{MAX_DELAY, ScheduledEvent, micro_to_samples};
use crate::conn::fixed::Micro;
use crate::time::conn::SampleTickConn;
use crate::time::swing;
use crate::time::tick::Tick;
use connections::fixed::u64::{I064U064, I128U064};

/// Conservative upper bound on the number of [`ScheduledEvent`]s
/// that can land in one buffer of `frames` samples.
///
/// Every event corresponds to at most one sample, so `frames` is
/// the absolute ceiling regardless of `(bpm, divider, sr)`. The
/// `+16` slack absorbs swing-boundary overrun where the scheduler
/// expands its tick window by `swing_d` ticks.
///
/// Used by [`Playhead`](crate::control::Playhead) and `host-cpal`'s
/// callback to size their pre-allocated
/// `Vec<ScheduledEvent>` so [`tick_stream_into`] never reallocates
/// inside the audio callback.
pub fn max_events_for_buffer(frames: usize) -> usize {
    frames + 16
}

/// Compute all `ScheduledEvent`s whose `sample_index` falls in
/// `[buffer_start_sample, buffer_start_sample + frames)`.
///
/// Pure: no I/O, deterministic in its inputs, allocates only the
/// returned `Vec`. Consecutive calls covering a contiguous sample
/// range together yield each tick exactly once — no dupes, no gaps
/// (see the `scheduler_block_equivalence` property in the test
/// module).
///
/// Thin wrapper over [`tick_stream_into`]: allocates a fresh `Vec`
/// and delegates. Use `tick_stream_into` directly from RT code to
/// reuse a pre-sized buffer and stay allocation-free.
pub fn tick_stream(
    common: &ChannelCommon,
    stc: &SampleTickConn,
    buffer_start_sample: u64,
    frames: usize,
) -> Vec<ScheduledEvent> {
    let mut buf = Vec::new();
    tick_stream_into(&mut buf, common, stc, buffer_start_sample, frames);
    buf
}

/// Allocation-free variant of [`tick_stream`]: pushes every accepted
/// `ScheduledEvent` into `buf` rather than returning a fresh `Vec`.
/// When `buf.capacity()` is sized to the worst-case event count for
/// the buffer window, this call allocates zero bytes on the heap —
/// the contract the audio callback relies on. Use
/// [`max_events_for_buffer`] to compute that upper bound.
///
/// `buf` is not cleared on entry; callers who want a fresh window
/// should `buf.clear()` before the call.
pub fn tick_stream_into(
    buf: &mut Vec<ScheduledEvent>,
    common: &ChannelCommon,
    stc: &SampleTickConn,
    buffer_start_sample: u64,
    frames: usize,
) {
    if frames == 0 {
        return;
    }
    let buffer_end = buffer_start_sample.saturating_add(frames as u64);

    // Inverse of the transform's sample offset: event.sample_index =
    // stc.inner(swung_tick) + delay_samples + offset_samples. For an
    // event to land in [start, end), the swung_tick's natural sample
    // must land in [start - delta, end - delta). Same
    // `FD12FD06 ∘ pico_to_samples` composition as `transform`,
    // routed through `micro_to_samples` so the two stages are
    // impossible to drift.
    let delay_clamped = Micro(common.delay.0.clamp(0, MAX_DELAY.0));
    let delay_samples: i64 = micro_to_samples(delay_clamped, stc.sr());
    let offset_samples: i64 = micro_to_samples(common.offset, stc.sr());
    // Promote to i128 so `buffer_start_sample - delta` can't wrap —
    // `buffer_start_sample as i64` would lose the high bit for streams
    // past ~6×10¹² seconds and produce spurious bounds.
    let delta: i128 = i128::from(delay_samples) + i128::from(offset_samples);
    let swung_lo_signed = i128::from(buffer_start_sample) - delta;
    let swung_hi_signed = i128::from(buffer_end) - delta;
    let swung_lo = I128U064.ceil(swung_lo_signed);
    let swung_hi = I128U064.ceil(swung_hi_signed);

    // Convert swung-tick sample bounds to tick bounds, then expand by
    // swing displacement so off-beats (which are shifted by +amount in
    // tick space; we store the negation here so the existing
    // `min(0)/max(0)` window-expansion math stays untouched) are
    // included.
    //
    // Tick is u64; widen to i128 throughout this stretch so the
    // signed swing window can't overflow at either edge of the u64
    // range.
    let swing_d: i128 = -i128::from(common.shuffle.amount);
    let lo_from_sample = i128::from(stc.floor(swung_lo).0);
    let hi_from_sample = i128::from(stc.ceil(swung_hi).0);
    let lo_tick = I128U064.ceil(lo_from_sample.saturating_add(swing_d.min(0)));
    let hi_tick_i = hi_from_sample.saturating_add(swing_d.max(0));
    let hi_tick = I128U064.ceil(hi_tick_i);

    if lo_tick > hi_tick {
        return;
    }

    // Inlined `transform` pipeline: divider → shuffle → Tick→Sample
    // → delay → offset, with the window filter applied before
    // push. Duplicated from `transform` so each accepted event goes
    // straight into `buf` — no intermediate Vec, no heap allocation
    // when `buf` is pre-sized. Change either path's arithmetic and
    // the `scheduler_block_equivalence` /
    // `tick_stream_into_matches_transform_filtered` proptests both
    // trip.
    let divisor = u64::from(common.divider.tick_count());
    let delay_fwd = I064U064.ceil(delay_samples);
    for t in (lo_tick..=hi_tick).map(Tick) {
        if t.0 % divisor != 0 {
            continue;
        }
        let swung = swing::effective_tick(&common.shuffle, t);
        let base = stc.inner(swung);
        let with_delay = base.saturating_add(delay_fwd);
        let final_sample = if offset_samples >= 0 {
            with_delay.saturating_add(I064U064.ceil(offset_samples))
        } else {
            with_delay.saturating_sub(offset_samples.unsigned_abs())
        };
        if final_sample >= buffer_start_sample && final_sample < buffer_end {
            buf.push(ScheduledEvent {
                sample_index: final_sample,
                tick: swung,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::arb::arb_grid;
    use crate::time::grid::Grid;
    use crate::time::swing::SwingConfig;
    use crate::time::tbase::TBase;
    use proptest::prelude::*;

    fn stc_120_48k() -> SampleTickConn {
        SampleTickConn::new(
            48_000,
            crate::conn::tempo::Tempo::from_bpm_integer(120),
            960,
        )
    }

    /// Bare `ChannelCommon` for scheduler tests — `tick_stream`
    /// operates on this directly post-Plan-21 (audit P3).
    fn zero_common(divider: Grid) -> ChannelCommon {
        ChannelCommon {
            divider,
            shuffle: SwingConfig {
                resolution: TBase::T16,
                amount: 0,
            },
            delay: Micro::ZERO,
            offset: Micro::ZERO,
            bar_multiplier: None,
        }
    }

    // ── Spot checks ──────────────────────────────────────────────

    #[test]
    fn t4_120bpm_48k_fires_at_buffer_6_offset_24000() {
        // One quarter note at 120 BPM = 24 000 samples. A 4 096-frame
        // buffer starting at 20 480 covers samples 20 480..24 576 —
        // exactly containing 24 000. At 960 PPQN the corresponding
        // tick is 960. The scheduler should emit one event.
        let common = zero_common(Grid::T4);
        let ev = tick_stream(&common, &stc_120_48k(), 20_480, 4_096);
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].sample_index, 24_000);
        assert_eq!(ev[0].tick.0, 960);
    }

    #[test]
    fn empty_buffer_returns_empty() {
        let common = zero_common(Grid::T4);
        assert!(tick_stream(&common, &stc_120_48k(), 0, 0).is_empty());
    }

    #[test]
    fn buffer_with_no_events_returns_empty() {
        // Between two quarter notes: buffer [1000, 5000) contains no
        // multiple of 24 000.
        let common = zero_common(Grid::T4);
        assert!(tick_stream(&common, &stc_120_48k(), 1_000, 4_000).is_empty());
    }

    #[test]
    fn t16_buffer_covers_multiple_events() {
        // 16th notes at 120 BPM 48 kHz: 6 000 samples apart. A buffer
        // 24 000 samples wide at sample 0 covers 4 events at
        // 0, 6 000, 12 000, 18 000.
        let common = zero_common(Grid::T16);
        let ev = tick_stream(&common, &stc_120_48k(), 0, 24_000);
        let samples: Vec<u64> = ev.iter().map(|e| e.sample_index).collect();
        assert_eq!(samples, vec![0, 6_000, 12_000, 18_000]);
    }

    // ── Property tests ───────────────────────────────────────────

    /// Same `(divider, bounded-swing)` generator as `transform`'s
    /// monotonicity test: swings whose displacement is smaller than
    /// the divider's step are well-behaved. Resolution pinned to the
    /// divider's binary axis.
    fn arb_divider_with_bounded_swing() -> impl Strategy<Value = (Grid, SwingConfig)> {
        arb_grid().prop_flat_map(|d| {
            let cap_i32 = ((d.tick_count() as i32 - 1).max(0)).min(i8::MAX as i32);
            let cap = cap_i32 as i8;
            let resolution = d.n;
            (
                Just(d),
                (-(cap as i32)..=(cap as i32)).prop_map(move |amount| SwingConfig {
                    resolution,
                    amount: amount as i8,
                }),
            )
        })
    }

    proptest! {
        /// Plan property `scheduler_events_in_window`: every emitted
        /// event's `sample_index` lies inside `[buffer_start,
        /// buffer_start + frames)`.
        #[test]
        fn scheduler_events_in_window(
            (divider, shuffle) in arb_divider_with_bounded_swing(),
            delay_us in 0_i64..=MAX_DELAY.0,
            offset_us in -5_000_i64..=5_000,
            buffer_start in 0u64..=1_000_000,
            frames in 1usize..=8_192,
        ) {
            let common = ChannelCommon {
                divider,
                shuffle,
                delay: Micro(delay_us),
                offset: Micro(offset_us),
                bar_multiplier: None,
            };
            let end = buffer_start + frames as u64;
            let ev = tick_stream(&common, &stc_120_48k(), buffer_start, frames);
            for e in &ev {
                prop_assert!(
                    e.sample_index >= buffer_start,
                    "event sample {} < start {}", e.sample_index, buffer_start
                );
                prop_assert!(
                    e.sample_index < end,
                    "event sample {} >= end {}", e.sample_index, end
                );
            }
        }

        /// `tick_stream_into`'s inlined per-tick pipeline stays
        /// bit-identical to `transform`'s forward path (filtered to
        /// the buffer window). Comparing `tick_stream_into` against
        /// `tick_stream` would be circular — `tick_stream` delegates
        /// to `tick_stream_into` — so the reference here is
        /// `transform` directly. Drift between the two
        /// pipelines trips this test immediately.
        ///
        /// The reference applies `transform` over a generous fixed
        /// tick range (`0..=65_536`); at PPQN 960 / 48 kHz / 120 BPM
        /// (= 25 samples/tick) that covers samples up to ~1.6 M, past
        /// the `buffer_start ≤ 1_000_000` + `frames ≤ 8_192` bound the
        /// proptest itself uses (~1.01 M samples max).
        #[test]
        fn tick_stream_into_matches_transform_filtered(
            (divider, shuffle) in arb_divider_with_bounded_swing(),
            delay_us in 0_i64..=MAX_DELAY.0,
            offset_us in -5_000_i64..=5_000,
            buffer_start in 0u64..=1_000_000,
            frames in 1usize..=8_192,
        ) {
            use crate::channel::time::transform;

            let common = ChannelCommon {
                divider,
                shuffle,
                delay: Micro(delay_us),
                offset: Micro(offset_us),
                bar_multiplier: None,
            };
            let stc = stc_120_48k();
            let buffer_end = buffer_start + frames as u64;

            // Reference: `transform` over a generous tick range,
            // then window-filtered. Calls `transform` directly so
            // the inlined pipeline in `tick_stream_into` cannot
            // shadow drift behind a delegation chain.
            let reference: Vec<ScheduledEvent> = transform(
                (0u64..=65_536).map(Tick),
                &common,
                &stc,
            )
            .into_iter()
            .filter(|e| e.sample_index >= buffer_start && e.sample_index < buffer_end)
            .collect();

            let mut pushed = Vec::new();
            tick_stream_into(&mut pushed, &common, &stc, buffer_start, frames);
            prop_assert_eq!(pushed, reference);
        }

        /// When the caller pre-sizes `buf` with enough capacity, the call
        /// leaves `buf.capacity()` unchanged. Pins the allocation-free
        /// contract — the RT callback relies on reusing one
        /// pre-allocated scratch buffer per channel across buffers.
        #[test]
        fn tick_stream_into_no_realloc(
            (divider, shuffle) in arb_divider_with_bounded_swing(),
            delay_us in 0_i64..=MAX_DELAY.0,
            offset_us in -5_000_i64..=5_000,
            buffer_start in 0u64..=1_000_000,
            frames in 1usize..=8_192,
        ) {
            let common = ChannelCommon {
                divider,
                shuffle,
                delay: Micro(delay_us),
                offset: Micro(offset_us),
                bar_multiplier: None,
            };
            let stc = stc_120_48k();
            // Upper bound: every master tick in the window could
            // produce an event. `frames` is the sample count; at PPQN
            // 960 / 48 kHz / 120 BPM the densest divider (Grid::T512P
            // = 1 tick / step) is ~40 master ticks per 1000 samples.
            // 4× `frames` is a generous ceiling for the shrink domain.
            let cap = frames * 4 + 32;
            let mut buf = Vec::with_capacity(cap);
            let cap_before = buf.capacity();
            tick_stream_into(&mut buf, &common, &stc, buffer_start, frames);
            prop_assert_eq!(
                buf.capacity(), cap_before,
                "tick_stream_into grew buf capacity — pre-alloc too small?"
            );
        }

        /// Plan properties `scheduler_no_dupes` + `scheduler_no_gaps`
        /// combined: consecutive buffer calls covering a contiguous
        /// sample range yield the same event multiset (and order) as
        /// a single call covering the whole range.
        #[test]
        fn scheduler_block_equivalence(
            (divider, shuffle) in arb_divider_with_bounded_swing(),
            delay_us in 0_i64..=MAX_DELAY.0,
            offset_us in -5_000_i64..=5_000,
            buf_size in 64usize..=2_048,
            n_buffers in 1usize..=16,
        ) {
            let common = ChannelCommon {
                divider,
                shuffle,
                delay: Micro(delay_us),
                offset: Micro(offset_us),
                bar_multiplier: None,
            };
            let stc = stc_120_48k();
            let total = buf_size * n_buffers;

            let one_big = tick_stream(&common, &stc, 0, total);
            let mut pieces = Vec::new();
            for b in 0..n_buffers {
                let start = (b * buf_size) as u64;
                pieces.extend(tick_stream(&common, &stc, start, buf_size));
            }
            prop_assert_eq!(one_big, pieces);
        }
    }
}
