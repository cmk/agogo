//! `Channel` configuration + the pure transform pipeline.
//!
//! Pipeline stages, applied in order (agogo.md §6):
//! 1. **Divide** — keep only master ticks divisible by
//!    `channel.divider.tick_count()`.
//! 2. **Shuffle** — apply [`swing::effective_tick`] (off-beats shift
//!    earlier by `amount × multiplier`; on-beats pass through).
//! 3. **Tick → Sample** via [`SampleTickConn::inner`].
//! 4. **Delay** — add `clamp(delay, 0, MAX_DELAY)` → Pico → Sample
//!    via `F12F06 ∘ pico_to_samples`. Plan 03 does not implement
//!    negative delay (needs a forward-look ring buffer, deferred to
//!    v0.2).
//! 5. **Offset** — same composition chain for the signed calibration
//!    offset.

use core::num::NonZeroU16;

use crate::channel::mode::ChannelMode;
use crate::fxp::pico_to_samples;
use crate::time::conn::SampleTickConn;
use crate::time::grid::Grid;
use crate::time::swing::{self, SwingConfig};
use crate::time::tick::Tick;
use connections::conn::fixed::{F12F06, Micro};

/// Maximum positive delay before saturation: 300 ms = 300 000 µs.
pub const MAX_DELAY: Micro = Micro(300_000);

/// Per-channel configuration.
#[derive(Copy, Clone, Debug)]
pub struct Channel {
    pub mode: ChannelMode,
    /// Divider expressed as the `Grid` whose tick count is the
    /// channel's step (agogo.md §6 mapping). E.g. `Grid::T16` fires
    /// 16th notes, `Grid::T4` fires quarter notes, `Grid::T8Q` fires
    /// quintuplet 8ths (5 per quarter at 192 ticks each).
    pub divider: Grid,
    pub shuffle: SwingConfig,
    /// Positive-only delay compensation, clamped to
    /// `[Micro::ZERO, MAX_DELAY]` on use.
    pub delay: Micro,
    /// Signed calibration offset. Not clamped here — CLI / UI should
    /// pick a musical range (agogo.md §6 cites ±5 ms = ±5 000 µs).
    /// Audit P2 (Plan 20) folded the previous `snap_to_quantum`
    /// arming intent into this single offset field at orchestrator
    /// startup time; the snap intent now lives only on `ChannelSpec`
    /// (`spec.snap_intent()`) and is applied via
    /// `LinkSession::snap_offset_for(intent)` by the caller.
    pub offset: Micro,
    /// Period multiplier on the channel's grid output. When
    /// `Some(N)`, the channel emits every `N`-th `tick_stream` event
    /// — applied as a pre-renderer filter in `Machine::on_buffer`
    /// against the per-channel `bar_counters` slot. The name reflects
    /// the most idiomatic case (`grid=t1,bars=N` = `N` literal bars
    /// in 4/4); the mechanism is divider-agnostic, so `grid=t8,bars=3`
    /// expresses a dotted-quarter period that isn't in `Grid::ALL`.
    /// `NonZeroU16` caps `N` at 65,535 — worst-case multiplied period
    /// `65,535 × Grid::T1.tick_count() (3840) ≈ 251M` ticks fits in
    /// `Tick(u32)` (`u32::MAX ≈ 4.29B`) with no overflow-check
    /// arithmetic.
    pub bar_multiplier: Option<NonZeroU16>,
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
/// # Panics
///
/// Panics if `sr` isn't one of the six supported rates (same set as
/// `pico_to_samples`). Every production caller goes through the CLI
/// or a test helper that validates `sr` before constructing the
/// `SampleTickConn`, so this panic surfaces programmer error (an
/// un-validated `sr` reached the transform) rather than user input.
/// `SampleTickConn::new` itself only asserts `sr > 0` — the audio-
/// rate allowlist is a separate invariant enforced at the CLI /
/// config boundary.
pub(crate) fn micro_to_samples(m: Micro, sr: u32) -> i64 {
    let pico = F12F06.inner(m);
    pico_to_samples(pico, sr).unwrap_or_else(|| {
        panic!("channel: unsupported sample rate {sr} (expected 44_100 / 48_000 / 88_200 / 96_000 / 176_400 / 192_000); validate `sr` at the CLI / config boundary before constructing the channel pipeline")
    })
}

/// Run the divider → shuffle → sample → delay → offset pipeline over
/// a master tick stream. Pure: output order matches input order and
/// no I/O is performed.
pub fn transform(
    master_ticks: impl IntoIterator<Item = Tick>,
    channel: &Channel,
    stc: &SampleTickConn,
) -> Vec<ScheduledEvent> {
    let divisor = channel.divider.tick_count();
    let delay_clamped = Micro(channel.delay.0.clamp(0, MAX_DELAY.0));
    let delay_samples = micro_to_samples(delay_clamped, stc.sr()).max(0) as u64;
    let offset_samples = micro_to_samples(channel.offset, stc.sr());

    master_ticks
        .into_iter()
        .filter(|t| t.0 % divisor == 0)
        .map(|t| {
            let swung = swing::effective_tick(&channel.shuffle, t);
            let base = stc.inner(swung);
            let with_delay = base.saturating_add(delay_samples);
            let final_sample = if offset_samples >= 0 {
                with_delay.saturating_add(offset_samples as u64)
            } else {
                with_delay.saturating_sub(offset_samples.unsigned_abs())
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
    use crate::arb::arb_grid;
    use crate::time::tbase::TBase;
    use proptest::prelude::*;

    fn stc_120_48k() -> SampleTickConn {
        SampleTickConn::new(48_000, crate::fxp::Tempo::from_bpm_integer(120), 960)
    }

    fn zero_channel(divider: Grid) -> Channel {
        Channel {
            mode: ChannelMode::MidiClock,
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
    fn t4_at_120bpm_48k_emits_at_half_second_multiples() {
        // Divider T4 (quarter, 960 ticks at 960 PPQN), shuffle 0,
        // delay 0, offset 0 → samples 0, 24 000, 48 000, …
        let ch = zero_channel(Grid::T4);
        let master: Vec<Tick> = (0..=3840).map(Tick).collect();
        let ev = transform(master, &ch, &stc_120_48k());
        let samples: Vec<u64> = ev.iter().map(|e| e.sample_index).collect();
        assert_eq!(samples, vec![0, 24_000, 48_000, 72_000, 96_000]);
    }

    #[test]
    fn delay_10ms_adds_exactly_480_samples() {
        // delay = 10 ms at 48 kHz → +480 samples.
        let mut ch = zero_channel(Grid::T4);
        ch.delay = Micro(10_000); // 10 ms
        let ev = transform([Tick(0), Tick(960)], &ch, &stc_120_48k());
        let samples: Vec<u64> = ev.iter().map(|e| e.sample_index).collect();
        assert_eq!(samples, vec![480, 24_480]);
    }

    #[test]
    fn divider_filters_unaligned_ticks() {
        // Divider T4 (960 ticks). Master stream contains every tick in
        // [0, 1000]; only 0 and 960 survive the filter.
        let ch = zero_channel(Grid::T4);
        let master: Vec<Tick> = (0..=1000).map(Tick).collect();
        let ev = transform(master, &ch, &stc_120_48k());
        let ticks: Vec<u32> = ev.iter().map(|e| e.tick.0).collect();
        assert_eq!(ticks, vec![0, 960]);
    }

    #[test]
    fn t8q_quintuplet_divider_fires_at_192_ticks() {
        // T8Q = 192 ticks (5 per quarter). Master stream covers
        // [0, 1000] → ticks 0, 192, 384, 576, 768, 960.
        let ch = zero_channel(Grid::T8Q);
        let master: Vec<Tick> = (0..=1000).map(Tick).collect();
        let ev = transform(master, &ch, &stc_120_48k());
        let ticks: Vec<u32> = ev.iter().map(|e| e.tick.0).collect();
        assert_eq!(ticks, vec![0, 192, 384, 576, 768, 960]);
    }

    #[test]
    fn delay_over_300ms_saturates() {
        let mut ch = zero_channel(Grid::T4);
        ch.delay = Micro(1_000_000); // 1 s
        let ev = transform([Tick(0)], &ch, &stc_120_48k());
        // Clamped to 300 ms → 14 400 samples at 48 kHz.
        assert_eq!(ev[0].sample_index, 14_400);
    }

    #[test]
    fn negative_delay_clamped_to_zero() {
        let mut ch = zero_channel(Grid::T4);
        ch.delay = Micro(-100_000); // -100 ms
        let ev = transform([Tick(0), Tick(960)], &ch, &stc_120_48k());
        assert_eq!(ev[0].sample_index, 0);
        assert_eq!(ev[1].sample_index, 24_000);
    }

    #[test]
    fn offset_negative_shifts_earlier() {
        let mut ch = zero_channel(Grid::T4);
        ch.offset = Micro(-1_000); // -1 ms = -48 samples at 48 kHz.
        let ev = transform([Tick(960)], &ch, &stc_120_48k());
        assert_eq!(ev[0].sample_index, 24_000 - 48);
    }

    // ── Property tests ───────────────────────────────────────────

    /// `(divider, swing)` pairs whose swing displacement is smaller
    /// than the divider's step so swung off-beats never leap past an
    /// adjacent on-beat — required for the monotonicity property.
    /// Resolution is pinned to the divider's binary axis (`divider.n`)
    /// so swing detection aligns with the divider grid wherever
    /// possible.
    fn arb_divider_with_bounded_swing() -> impl Strategy<Value = (Grid, SwingConfig)> {
        arb_grid().prop_flat_map(|d| {
            // |amount| < divider tick count keeps swung off-beats
            // strictly inside their step window. Clamp to i8.
            let cap_i32 = ((d.tick_count() as i32 - 1).max(0)).min(i8::MAX as i32);
            let cap = cap_i32 as i8;
            let resolution = d.n;
            (
                Just(d),
                (-(cap as i32)..=(cap as i32))
                    .prop_map(move |amount| SwingConfig {
                        resolution,
                        amount: amount as i8,
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
            delay_us in 0_i64..=MAX_DELAY.0,
            offset_us in -5_000_i64..=5_000,
            max_tick in 960u32..=10_000,
        ) {
            let ch = Channel {
                mode: ChannelMode::MidiClock,
                divider,
                shuffle,
                delay: Micro(delay_us),
                offset: Micro(offset_us),
                bar_multiplier: None,
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
        /// multiples that fall inside `[0, beats × 960)` at 960 PPQN.
        #[test]
        fn divider_rate_preservation(
            divider in arb_grid(),
            beats in 1u32..=16,
        ) {
            let ch = zero_channel(divider);
            let span = beats * 960;
            let master: Vec<Tick> = (0..span).map(Tick).collect();
            let ev = transform(master, &ch, &stc_120_48k());
            let expected = span.div_ceil(divider.tick_count());
            prop_assert_eq!(ev.len() as u32, expected);
        }

        /// Plan property `shift_clamping`, upper bound.
        #[test]
        fn delay_upper_clamp(delay_us in MAX_DELAY.0..=10_000_000_i64) {
            let mut ch = zero_channel(Grid::T4);
            ch.delay = Micro(delay_us);
            let stc = stc_120_48k();
            let ev = transform([Tick(960)], &ch, &stc);
            let cap_samples = super::micro_to_samples(MAX_DELAY, stc.sr()) as u64;
            prop_assert_eq!(ev[0].sample_index, 24_000 + cap_samples);
        }

        /// Plan property `shift_clamping`, lower bound.
        #[test]
        fn delay_lower_clamp(delay_us in -10_000_000_i64..0) {
            let mut ch = zero_channel(Grid::T4);
            ch.delay = Micro(delay_us);
            let ev = transform([Tick(960)], &ch, &stc_120_48k());
            prop_assert_eq!(ev[0].sample_index, 24_000);
        }

        /// Swing is identity on T16 even-parity steps regardless of
        /// `amount`. At 960 PPQN, even T16 steps land at ticks
        /// 0, 480, 960, 1440 (two T16 steps per T8 boundary).
        #[test]
        fn shuffle_identity_on_even_steps(amount in any::<i8>()) {
            let mut ch = zero_channel(Grid::T16);
            ch.shuffle = SwingConfig {
                resolution: TBase::T16,
                amount,
            };
            let stc = stc_120_48k();
            for step in [0u32, 480, 960, 1440] {
                let ev = transform([Tick(step)], &ch, &stc);
                prop_assert_eq!(ev[0].tick, Tick(step));
            }
        }
    }
}
