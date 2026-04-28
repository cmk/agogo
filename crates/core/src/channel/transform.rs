//! `Channel` (sum-typed by routing target) + the pure transform
//! pipeline.
//!
//! Pipeline stages, applied in order (agogo.md §6) — operate on
//! [`ChannelCommon`] alone (the role-specific payload is consulted
//! only at the renderer layer):
//! 1. **Divide** — keep only master ticks divisible by
//!    `common.divider.tick_count()`.
//! 2. **Shuffle** — apply [`swing::effective_tick`] (off-beats shift
//!    earlier by `amount × multiplier`; on-beats pass through).
//! 3. **Tick → Sample** via [`SampleTickConn::inner`].
//! 4. **Delay** — add `clamp(delay, 0, MAX_DELAY)` → Pico → Sample
//!    via `FD12FD06 ∘ pico_to_samples`. Plan 03 does not implement
//!    negative delay (needs a forward-look ring buffer, deferred to
//!    v0.2).
//! 5. **Offset** — same composition chain for the signed calibration
//!    offset.

use crate::channel::role::{ChannelCommon, CvRole, DinRole, MidiRole};
use crate::fxp::pico_to_samples;
use crate::sync::sample_tick::SampleTickConn;
use crate::time::swing;
use crate::time::tick::Tick;
use crate::fxp::{FD12FD06, Micro};

/// Maximum positive delay before saturation: 300 ms = 300 000 µs.
pub const MAX_DELAY: Micro = Micro(300_000);

/// Per-channel configuration, sum-typed by routing target.
///
/// Each variant carries a [`ChannelCommon`] (the field set the
/// scheduler / transform pipeline operates on) and a target-specific
/// `*Role` payload that the renderer consumes. Renderers narrow on
/// the outer variant: a non-MIDI `Channel` doesn't type-check as
/// input to `render_midi_channel`. Plan 21 (audit P3).
///
/// `Channel::Audio` is **not** a variant in v0.1 — an empty
/// `AudioRole` would make the variant uninhabited; v0.4's
/// audio-click renderer is the natural slot to introduce both.
#[derive(Copy, Clone, Debug)]
pub enum Channel {
    Midi {
        common: ChannelCommon,
        role: MidiRole,
    },
    Din {
        common: ChannelCommon,
        role: DinRole,
    },
    Cv {
        common: ChannelCommon,
        role: CvRole,
    },
}

impl Channel {
    /// Borrow the shared [`ChannelCommon`] regardless of variant.
    /// Single match arm per variant; `cargo` inlines.
    pub fn common(&self) -> &ChannelCommon {
        match self {
            Channel::Midi { common, .. } => common,
            Channel::Din { common, .. } => common,
            Channel::Cv { common, .. } => common,
        }
    }

    /// Mutably borrow the shared [`ChannelCommon`].
    pub fn common_mut(&mut self) -> &mut ChannelCommon {
        match self {
            Channel::Midi { common, .. } => common,
            Channel::Din { common, .. } => common,
            Channel::Cv { common, .. } => common,
        }
    }
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
/// the adjoint-law composition `FD12FD06 ∘ pico_to_samples`. Shared
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
    let pico = FD12FD06.inner(m);
    pico_to_samples(pico, sr).unwrap_or_else(|| {
        panic!("channel: unsupported sample rate {sr} (expected 44_100 / 48_000 / 88_200 / 96_000 / 176_400 / 192_000); validate `sr` at the CLI / config boundary before constructing the channel pipeline")
    })
}

/// Run the divider → shuffle → sample → delay → offset pipeline over
/// a master tick stream. Pure: output order matches input order and
/// no I/O is performed.
///
/// Operates on [`ChannelCommon`] alone — call sites that hold a
/// `Channel` pass `ch.common()`. The role payload is irrelevant to
/// the transform pipeline; it's only consulted at the renderer
/// layer.
pub fn transform(
    master_ticks: impl IntoIterator<Item = Tick>,
    common: &ChannelCommon,
    stc: &SampleTickConn,
) -> Vec<ScheduledEvent> {
    let divisor = common.divider.tick_count();
    let delay_clamped = Micro(common.delay.0.clamp(0, MAX_DELAY.0));
    let delay_samples = micro_to_samples(delay_clamped, stc.sr()).max(0) as u64;
    let offset_samples = micro_to_samples(common.offset, stc.sr());

    master_ticks
        .into_iter()
        .filter(|t| t.0 % divisor == 0)
        .map(|t| {
            let swung = swing::effective_tick(&common.shuffle, t);
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
    use crate::time::grid::Grid;
    use crate::time::swing::SwingConfig;
    use crate::time::tbase::TBase;
    use proptest::prelude::*;

    fn stc_120_48k() -> SampleTickConn {
        SampleTickConn::new(48_000, crate::fxp::Tempo::from_bpm_integer(120), 960)
    }

    /// Bare `ChannelCommon` for transform-pipeline tests — the
    /// pipeline doesn't care about role, so the test fixture
    /// doesn't either.
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

    /// Full `Channel::Midi` for sites that need to construct a
    /// channel rather than just a `ChannelCommon`.
    fn zero_midi_clock_channel(divider: Grid) -> Channel {
        Channel::Midi {
            common: zero_common(divider),
            role: MidiRole::Clock,
        }
    }

    // ── Channel structural tests ─────────────────────────────────

    #[test]
    fn channel_common_borrow_matches_inner_field() {
        let ch = zero_midi_clock_channel(Grid::T4);
        assert_eq!(ch.common().divider, Grid::T4);
        assert_eq!(ch.common().delay, Micro::ZERO);
    }

    #[test]
    fn channel_common_mut_round_trip() {
        let mut ch = zero_midi_clock_channel(Grid::T4);
        ch.common_mut().delay = Micro(10_000);
        assert_eq!(ch.common().delay, Micro(10_000));
    }

    #[test]
    fn channel_din_constructible() {
        let _ = Channel::Din {
            common: zero_common(Grid::T4),
            role: DinRole::Sync24,
        };
    }

    #[test]
    fn channel_cv_constructible() {
        let _ = Channel::Cv {
            common: zero_common(Grid::T4),
            role: CvRole::Pulse,
        };
        let _ = Channel::Cv {
            common: zero_common(Grid::T4),
            role: CvRole::Lfo,
        };
    }

    // ── Spot checks (transform pipeline; role is irrelevant) ─────

    #[test]
    fn t4_at_120bpm_48k_emits_at_half_second_multiples() {
        // Divider T4 (quarter, 960 ticks at 960 PPQN), shuffle 0,
        // delay 0, offset 0 → samples 0, 24 000, 48 000, …
        let common = zero_common(Grid::T4);
        let master: Vec<Tick> = (0..=3840).map(Tick).collect();
        let ev = transform(master, &common, &stc_120_48k());
        let samples: Vec<u64> = ev.iter().map(|e| e.sample_index).collect();
        assert_eq!(samples, vec![0, 24_000, 48_000, 72_000, 96_000]);
    }

    #[test]
    fn delay_10ms_adds_exactly_480_samples() {
        // delay = 10 ms at 48 kHz → +480 samples.
        let mut common = zero_common(Grid::T4);
        common.delay = Micro(10_000); // 10 ms
        let ev = transform([Tick(0), Tick(960)], &common, &stc_120_48k());
        let samples: Vec<u64> = ev.iter().map(|e| e.sample_index).collect();
        assert_eq!(samples, vec![480, 24_480]);
    }

    #[test]
    fn divider_filters_unaligned_ticks() {
        // Divider T4 (960 ticks). Master stream contains every tick in
        // [0, 1000]; only 0 and 960 survive the filter.
        let common = zero_common(Grid::T4);
        let master: Vec<Tick> = (0..=1000).map(Tick).collect();
        let ev = transform(master, &common, &stc_120_48k());
        let ticks: Vec<u32> = ev.iter().map(|e| e.tick.0).collect();
        assert_eq!(ticks, vec![0, 960]);
    }

    #[test]
    fn t8q_quintuplet_divider_fires_at_192_ticks() {
        // T8Q = 192 ticks (5 per quarter). Master stream covers
        // [0, 1000] → ticks 0, 192, 384, 576, 768, 960.
        let common = zero_common(Grid::T8Q);
        let master: Vec<Tick> = (0..=1000).map(Tick).collect();
        let ev = transform(master, &common, &stc_120_48k());
        let ticks: Vec<u32> = ev.iter().map(|e| e.tick.0).collect();
        assert_eq!(ticks, vec![0, 192, 384, 576, 768, 960]);
    }

    #[test]
    fn delay_over_300ms_saturates() {
        let mut common = zero_common(Grid::T4);
        common.delay = Micro(1_000_000); // 1 s
        let ev = transform([Tick(0)], &common, &stc_120_48k());
        // Clamped to 300 ms → 14 400 samples at 48 kHz.
        assert_eq!(ev[0].sample_index, 14_400);
    }

    #[test]
    fn negative_delay_clamped_to_zero() {
        let mut common = zero_common(Grid::T4);
        common.delay = Micro(-100_000); // -100 ms
        let ev = transform([Tick(0), Tick(960)], &common, &stc_120_48k());
        assert_eq!(ev[0].sample_index, 0);
        assert_eq!(ev[1].sample_index, 24_000);
    }

    #[test]
    fn offset_negative_shifts_earlier() {
        let mut common = zero_common(Grid::T4);
        common.offset = Micro(-1_000); // -1 ms = -48 samples at 48 kHz.
        let ev = transform([Tick(960)], &common, &stc_120_48k());
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
            let common = ChannelCommon {
                divider,
                shuffle,
                delay: Micro(delay_us),
                offset: Micro(offset_us),
                bar_multiplier: None,
            };
            let master: Vec<Tick> = (0..=max_tick).map(Tick).collect();
            let ev = transform(master, &common, &stc_120_48k());
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
            let common = zero_common(divider);
            let span = beats * 960;
            let master: Vec<Tick> = (0..span).map(Tick).collect();
            let ev = transform(master, &common, &stc_120_48k());
            let expected = span.div_ceil(divider.tick_count());
            prop_assert_eq!(ev.len() as u32, expected);
        }

        /// Plan property `shift_clamping`, upper bound.
        #[test]
        fn delay_upper_clamp(delay_us in MAX_DELAY.0..=10_000_000_i64) {
            let mut common = zero_common(Grid::T4);
            common.delay = Micro(delay_us);
            let stc = stc_120_48k();
            let ev = transform([Tick(960)], &common, &stc);
            let cap_samples = super::micro_to_samples(MAX_DELAY, stc.sr()) as u64;
            prop_assert_eq!(ev[0].sample_index, 24_000 + cap_samples);
        }

        /// Plan property `shift_clamping`, lower bound.
        #[test]
        fn delay_lower_clamp(delay_us in -10_000_000_i64..0) {
            let mut common = zero_common(Grid::T4);
            common.delay = Micro(delay_us);
            let ev = transform([Tick(960)], &common, &stc_120_48k());
            prop_assert_eq!(ev[0].sample_index, 24_000);
        }

        /// Swing is identity on T16 even-parity steps regardless of
        /// `amount`. At 960 PPQN, even T16 steps land at ticks
        /// 0, 480, 960, 1440 (two T16 steps per T8 boundary).
        #[test]
        fn shuffle_identity_on_even_steps(amount in any::<i8>()) {
            let mut common = zero_common(Grid::T16);
            common.shuffle = SwingConfig {
                resolution: TBase::T16,
                amount,
            };
            let stc = stc_120_48k();
            for step in [0u32, 480, 960, 1440] {
                let ev = transform([Tick(step)], &common, &stc);
                prop_assert_eq!(ev[0].tick, Tick(step));
            }
        }
    }
}
