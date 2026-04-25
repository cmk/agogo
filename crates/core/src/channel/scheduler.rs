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

use crate::channel::transform::{Channel, MAX_SHIFT, ScheduledEvent, micro_to_samples};
use crate::time::conn::SampleTickConn;
use crate::time::swing;
use crate::time::tick::Tick;
use connections::conn::fixed::Micro;

/// Conservative upper bound on the number of [`ScheduledEvent`]s
/// that can land in one buffer of `frames` samples.
///
/// Every event corresponds to at most one sample, so `frames` is
/// the absolute ceiling regardless of `(bpm, divider, sr)`. The
/// `+16` slack absorbs swing-boundary overrun where the scheduler
/// expands its tick window by `swing_d` ticks.
///
/// Used by Plan 14's [`Machine`](crate::machine::Machine) and
/// Plan 13's `host-cpal` callback to size their pre-allocated
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
    channel: &Channel,
    stc: &SampleTickConn,
    buffer_start_sample: u64,
    frames: usize,
) -> Vec<ScheduledEvent> {
    let mut buf = Vec::new();
    tick_stream_into(&mut buf, channel, stc, buffer_start_sample, frames);
    buf
}

/// Allocation-free variant of [`tick_stream`]: pushes every accepted
/// `ScheduledEvent` into `buf` rather than returning a fresh `Vec`.
/// When `buf.capacity()` is sized to the worst-case event count for
/// the buffer window, this call allocates zero bytes on the heap —
/// the contract Plan 13's audio callback relies on. Use
/// [`max_events_for_buffer`] to compute that upper bound.
///
/// `buf` is not cleared on entry; callers who want a fresh window
/// should `buf.clear()` before the call.
pub fn tick_stream_into(
    buf: &mut Vec<ScheduledEvent>,
    channel: &Channel,
    stc: &SampleTickConn,
    buffer_start_sample: u64,
    frames: usize,
) {
    if frames == 0 {
        return;
    }
    let buffer_end = buffer_start_sample.saturating_add(frames as u64);

    // Inverse of the transform's sample offset: event.sample_index =
    // stc.inner(swung_tick) + shift_samples + offset_samples. For an
    // event to land in [start, end), the swung_tick's natural sample
    // must land in [start - delta, end - delta). Same
    // `F12F06 ∘ pico_to_samples` composition as `transform`,
    // routed through `micro_to_samples` so the two stages are
    // impossible to drift.
    let shift_clamped = Micro(channel.shift.0.clamp(0, MAX_SHIFT.0));
    let shift_samples: i64 = micro_to_samples(shift_clamped, stc.sr());
    let offset_samples: i64 = micro_to_samples(channel.offset, stc.sr());
    // Promote to i128 so `buffer_start_sample - delta` can't wrap —
    // `buffer_start_sample as i64` would lose the high bit for streams
    // past ~6×10¹² seconds and produce spurious bounds.
    let delta: i128 = i128::from(shift_samples) + i128::from(offset_samples);
    let swung_lo_signed = i128::from(buffer_start_sample) - delta;
    let swung_hi_signed = i128::from(buffer_end) - delta;
    let swung_lo = swung_lo_signed.clamp(0, i128::from(u64::MAX)) as u64;
    let swung_hi = swung_hi_signed.clamp(0, i128::from(u64::MAX)) as u64;

    // Convert swung-tick sample bounds to tick bounds, then expand by
    // swing displacement so off-beats (which are shifted by -d in tick
    // space) are included.
    let swing_d = channel.shuffle.displacement();
    let lo_from_sample = stc.floor(swung_lo).0 as i64;
    let hi_from_sample = stc.ceil(swung_hi).0 as i64;
    let lo_tick = lo_from_sample.saturating_add(swing_d.min(0)).max(0) as u32;
    let hi_tick_i = hi_from_sample.saturating_add(swing_d.max(0));
    let hi_tick = hi_tick_i.clamp(0, u32::MAX as i64) as u32;

    if lo_tick > hi_tick {
        return;
    }

    // Inlined `transform` pipeline: divider → shuffle → Tick→Sample
    // → shift → offset, with the window filter applied before
    // push. Duplicated from `transform` so each accepted event goes
    // straight into `buf` — no intermediate Vec, no heap allocation
    // when `buf` is pre-sized. Change either path's arithmetic and
    // the `scheduler_block_equivalence` /
    // `tick_stream_into_matches_transform_filtered` proptests both
    // trip.
    let divisor = channel.divider.tick_count();
    let shift_fwd = shift_samples.max(0) as u64;
    for t in (lo_tick..=hi_tick).map(Tick) {
        if t.0 % divisor != 0 {
            continue;
        }
        let swung = swing::effective_tick(&channel.shuffle, t);
        let base = stc.inner(swung);
        let with_shift = base.saturating_add(shift_fwd);
        let final_sample = if offset_samples >= 0 {
            with_shift.saturating_add(offset_samples as u64)
        } else {
            with_shift.saturating_sub(offset_samples.unsigned_abs())
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
    use crate::arb::arb_tbase;
    use crate::channel::mode::ChannelMode;
    use crate::time::swing::SwingConfig;
    use crate::time::tbase::TBase;
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
            snap_to_quantum: None,
        }
    }

    // ── Spot checks ──────────────────────────────────────────────

    #[test]
    fn t4_120bpm_48k_fires_at_buffer_6_offset_24000() {
        // One quarter note at 120 BPM = 24 000 samples. A 4 096-frame
        // buffer starting at 20 480 covers samples 20 480..24 576 —
        // exactly containing 24 000. The scheduler should emit one
        // event.
        let ch = zero_channel(TBase::T4);
        let ev = tick_stream(&ch, &stc_120_48k(), 20_480, 4_096);
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].sample_index, 24_000);
        assert_eq!(ev[0].tick.0, 192);
    }

    #[test]
    fn empty_buffer_returns_empty() {
        let ch = zero_channel(TBase::T4);
        assert!(tick_stream(&ch, &stc_120_48k(), 0, 0).is_empty());
    }

    #[test]
    fn buffer_with_no_events_returns_empty() {
        // Between two quarter notes: buffer [1000, 5000) contains no
        // multiple of 24 000.
        let ch = zero_channel(TBase::T4);
        assert!(tick_stream(&ch, &stc_120_48k(), 1_000, 4_000).is_empty());
    }

    #[test]
    fn t16_buffer_covers_multiple_events() {
        // 16th notes at 120 BPM 48 kHz: 6 000 samples apart. A buffer
        // 24 000 samples wide at sample 0 covers 4 events at
        // 0, 6 000, 12 000, 18 000.
        let ch = zero_channel(TBase::T16);
        let ev = tick_stream(&ch, &stc_120_48k(), 0, 24_000);
        let samples: Vec<u64> = ev.iter().map(|e| e.sample_index).collect();
        assert_eq!(samples, vec![0, 6_000, 12_000, 18_000]);
    }

    // ── Property tests ───────────────────────────────────────────

    /// Same `(divider, bounded-swing)` generator as `transform`'s
    /// monotonicity test: swings whose displacement is smaller than the
    /// divider's step are well-behaved.
    fn arb_divider_with_bounded_swing() -> impl Strategy<Value = (TBase, SwingConfig)> {
        arb_tbase().prop_flat_map(|d| {
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
        /// Plan property `scheduler_events_in_window`: every emitted
        /// event's `sample_index` lies inside `[buffer_start,
        /// buffer_start + frames)`.
        #[test]
        fn scheduler_events_in_window(
            (divider, shuffle) in arb_divider_with_bounded_swing(),
            shift_us in 0_i64..=MAX_SHIFT.0,
            offset_us in -5_000_i64..=5_000,
            buffer_start in 0u64..=1_000_000,
            frames in 1usize..=8_192,
        ) {
            let ch = Channel {
                mode: ChannelMode::MidiClock,
                divider,
                shuffle,
                shift: Micro(shift_us),
                offset: Micro(offset_us),
                snap_to_quantum: None,
            };
            let end = buffer_start + frames as u64;
            let ev = tick_stream(&ch, &stc_120_48k(), buffer_start, frames);
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

        /// Plan 13 property `tick_stream_into_matches_transform_filtered`:
        /// `tick_stream_into`'s inlined per-tick pipeline stays
        /// bit-identical to `transform`'s forward path (filtered to
        /// the buffer window). Comparing `tick_stream_into` against
        /// `tick_stream` would be circular — `tick_stream` delegates
        /// to `tick_stream_into` post-Plan-13-T0b — so the reference
        /// here is `transform` directly. Drift between the two
        /// pipelines trips this test immediately.
        ///
        /// The reference applies `transform` over a generous fixed
        /// tick range (`0..=65_536`); at PPQN 192 / 48 kHz / 120
        /// BPM that covers samples up to ~13 s, well past the
        /// `buffer_start ≤ 1_000_000` + `frames ≤ 8_192` bound the
        /// proptest itself uses (~21 s of stream time max).
        #[test]
        fn tick_stream_into_matches_transform_filtered(
            (divider, shuffle) in arb_divider_with_bounded_swing(),
            shift_us in 0_i64..=MAX_SHIFT.0,
            offset_us in -5_000_i64..=5_000,
            buffer_start in 0u64..=1_000_000,
            frames in 1usize..=8_192,
        ) {
            use crate::channel::transform::transform;

            let ch = Channel {
                mode: ChannelMode::MidiClock,
                divider,
                shuffle,
                shift: Micro(shift_us),
                offset: Micro(offset_us),
                snap_to_quantum: None,
            };
            let stc = stc_120_48k();
            let buffer_end = buffer_start + frames as u64;

            // Reference: `transform` over a generous tick range,
            // then window-filtered. Calls `transform` directly so
            // the inlined pipeline in `tick_stream_into` cannot
            // shadow drift behind a delegation chain.
            let reference: Vec<ScheduledEvent> = transform(
                (0..=65_536u32).map(Tick),
                &ch,
                &stc,
            )
            .into_iter()
            .filter(|e| e.sample_index >= buffer_start && e.sample_index < buffer_end)
            .collect();

            let mut pushed = Vec::new();
            tick_stream_into(&mut pushed, &ch, &stc, buffer_start, frames);
            prop_assert_eq!(pushed, reference);
        }

        /// Plan 13 property `tick_stream_into_no_realloc`: when the
        /// caller pre-sizes `buf` with enough capacity, the call
        /// leaves `buf.capacity()` unchanged. Pins the allocation-free
        /// contract — the RT callback relies on reusing one
        /// pre-allocated scratch buffer per channel across buffers.
        #[test]
        fn tick_stream_into_no_realloc(
            (divider, shuffle) in arb_divider_with_bounded_swing(),
            shift_us in 0_i64..=MAX_SHIFT.0,
            offset_us in -5_000_i64..=5_000,
            buffer_start in 0u64..=1_000_000,
            frames in 1usize..=8_192,
        ) {
            let ch = Channel {
                mode: ChannelMode::MidiClock,
                divider,
                shuffle,
                shift: Micro(shift_us),
                offset: Micro(offset_us),
                snap_to_quantum: None,
            };
            let stc = stc_120_48k();
            // Upper bound: every master tick in the window could
            // produce an event. `frames` is the sample count; at PPQN
            // 192 / 48 kHz / 120 BPM the densest divider (T128t = 1
            // tick / step) is ~125 master ticks per 1000 samples.
            // 4× `frames` is a generous ceiling for the shrink domain.
            let cap = frames * 4 + 32;
            let mut buf = Vec::with_capacity(cap);
            let cap_before = buf.capacity();
            tick_stream_into(&mut buf, &ch, &stc, buffer_start, frames);
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
            shift_us in 0_i64..=MAX_SHIFT.0,
            offset_us in -5_000_i64..=5_000,
            buf_size in 64usize..=2_048,
            n_buffers in 1usize..=16,
        ) {
            let ch = Channel {
                mode: ChannelMode::MidiClock,
                divider,
                shuffle,
                shift: Micro(shift_us),
                offset: Micro(offset_us),
                snap_to_quantum: None,
            };
            let stc = stc_120_48k();
            let total = buf_size * n_buffers;

            let one_big = tick_stream(&ch, &stc, 0, total);
            let mut pieces = Vec::new();
            for b in 0..n_buffers {
                let start = (b * buf_size) as u64;
                pieces.extend(tick_stream(&ch, &stc, start, buf_size));
            }
            prop_assert_eq!(one_big, pieces);
        }
    }
}
