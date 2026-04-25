//! Audio callback hot loop — `CallbackState::on_buffer`.
//!
//! Drives the per-buffer pipeline:
//!
//! 1. Feed input samples into the [`PhaseSource`] (PLL or Internal).
//! 2. Schedule master ticks for this buffer via
//!    [`tick_stream_into`] (alloc-free; reuses the preallocated
//!    `events` Vec).
//! 3. Render through [`render_channel_block`] into the
//!    [`RtProducer`], which enqueues onto the SPSC ring for the
//!    drain thread.
//!
//! No allocation, no locks. The audio thread spends time only in
//! integer arithmetic and one ring-buffer push per emitted event.

use crate::cpal::control::RtProducer;
use agogo_core::channel::scheduler::tick_stream_into;
use agogo_core::channel::{Channel, ScheduledEvent};
use agogo_core::fxp::SampleTime;
use agogo_core::host::AudioIo;
use agogo_core::out::midi::{MidiRtByte, render_channel_block};
use agogo_core::sync::PhaseSource;
use agogo_core::time::conn::SampleTickConn;

/// State the audio thread owns by-value across the stream's
/// lifetime. Built on the control thread, moved into the cpal
/// callback closure, never touched from the control thread again.
///
/// The `R: SampleTime` parameter binds the `PhaseSource`'s rate at
/// compile time. Plan 13 dispatches it at the CLI boundary
/// (`agogo demo --sr 48000` instantiates `CallbackState<S48>`,
/// `--sr 44100` instantiates `CallbackState<S44>`); wider rate
/// support is a matter of adding `SampleTime` impls in
/// `agogo_core::fxp`.
pub struct CallbackState<R: SampleTime> {
    pub phase_source: PhaseSource<R>,
    /// Single channel for v0.1's demo path; Plan 14 generalises
    /// to N channels via `Machine`.
    pub channel: Channel,
    pub stc: SampleTickConn,
    pub producer: RtProducer,
    /// Preallocated event buffer, reused across calls. Pre-size
    /// via [`max_events_for_buffer`] so `tick_stream_into` never
    /// reallocates during the callback.
    pub events: Vec<ScheduledEvent>,
}

impl<R: SampleTime> CallbackState<R> {
    /// Per-buffer entry point. Called by `CpalHost`'s data callback
    /// once per audio buffer. No allocation, no locks.
    ///
    /// `transport` lets a higher layer (Plan 14's `Machine`) inject
    /// a `Start` / `Continue` / `Stop` byte at the buffer boundary;
    /// Plan 13's standalone demo passes `None`.
    pub fn on_buffer(&mut self, io: &mut AudioIo, transport: Option<MidiRtByte>) {
        // 1. Feed PCM into the PhaseSource — Internal: no-op;
        //    External: drives detector + PLL.
        self.phase_source.feed_samples(io.input, io.buffer_start_sample);

        // 2. Schedule events for this buffer into the preallocated
        //    Vec. `clear()` resets length, leaves capacity intact.
        self.events.clear();
        tick_stream_into(
            &mut self.events,
            &self.channel,
            &self.stc,
            io.buffer_start_sample,
            io.frames,
        );

        // 3. Render to the SPSC. `render_channel_block` takes
        //    `&dyn MidiSink`; `RtProducer: MidiSink` (Plan 13 T3)
        //    enqueues each `send_at` call onto the ring.
        render_channel_block(
            &self.channel,
            &self.events,
            transport,
            io.buffer_start_sample,
            &self.producer,
        );
    }
}

/// Re-export of the canonical helper. The implementation moved to
/// [`agogo_core::channel::scheduler::max_events_for_buffer`] in
/// Plan 14 so [`agogo_core::machine::Machine`] can size its pool
/// without depending on `host-cpal`. Kept here so existing call
/// sites (the demo CLI handler, internal tests) compile unchanged.
pub use agogo_core::channel::scheduler::max_events_for_buffer;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpal::control::spsc;
    use agogo_core::channel::ChannelMode;
    use agogo_core::fxp::{Micro, S48, Tempo};
    use agogo_core::time::swing::SwingConfig;
    use agogo_core::time::tbase::TBase;
    use agogo_core::time::tick::PPQN;

    fn build_state() -> (CallbackState<S48>, crate::cpal::control::ControlConsumer) {
        let (producer, consumer) = spsc(1024);
        let state = CallbackState::<S48> {
            phase_source: PhaseSource::Internal {
                bpm: Tempo::from_bpm_integer(120),
            },
            channel: Channel {
                mode: ChannelMode::MidiClock,
                divider: TBase::T4,
                shuffle: SwingConfig {
                    amount: 0,
                    multiplier: 1,
                },
                shift: Micro::ZERO,
                offset: Micro::ZERO,
                snap_to_quantum: None,
            },
            stc: SampleTickConn::new(48_000, Tempo::from_bpm_integer(120), PPQN),
            producer,
            events: Vec::with_capacity(max_events_for_buffer(24_000)),
        };
        (state, consumer)
    }

    /// Plan 13 property `callback_emits_expected_clock_schedule`:
    /// driving `on_buffer` with `PhaseSource::Internal` at
    /// `(120 BPM, 48 kHz, T4 divider, 24 000 frames)` for 4
    /// contiguous buffers produces SPSC messages at samples
    /// `{0, 24_000, 48_000, 72_000}`. Pins the
    /// scheduler → render → SPSC composition.
    #[test]
    fn callback_emits_expected_clock_schedule() {
        let (mut state, mut cons) = build_state();
        let input = vec![0.0_f32; 24_000];
        let mut output: [f32; 0] = [];
        for b in 0..4u64 {
            let mut io = AudioIo::new(
                &input,
                &mut output,
                b * 24_000,
                48_000,
                24_000,
            );
            state.on_buffer(&mut io, None);
        }
        let mut samples = Vec::new();
        while let Some(m) = cons.try_pop() {
            samples.push(m.at_sample);
        }
        assert_eq!(samples, vec![0, 24_000, 48_000, 72_000]);
    }

    /// Pin `max_events_for_buffer`'s contract: at the demo's
    /// canonical sizing, the bound covers Plan 13 T0b's
    /// `tick_stream_into_no_realloc` test domain.
    #[test]
    fn max_events_for_buffer_covers_realistic_sizing() {
        // 4 096 frames is the typical cpal default; +16 slack.
        assert_eq!(max_events_for_buffer(4_096), 4_112);
        // 0 frames returns 16 (slack only) — defensive but harmless.
        assert_eq!(max_events_for_buffer(0), 16);
    }

    /// Verify that `events` capacity stays unchanged across a
    /// callback call when pre-sized via `max_events_for_buffer`.
    /// Mirrors the Plan 13 T0b proptest on `tick_stream_into`,
    /// elevated here to pin the callback's allocation-free
    /// contract end-to-end.
    #[test]
    fn callback_does_not_realloc_events() {
        let (mut state, _cons) = build_state();
        let cap_before = state.events.capacity();
        let input = vec![0.0_f32; 24_000];
        let mut output: [f32; 0] = [];
        let mut io = AudioIo::new(&input, &mut output, 0, 48_000, 24_000);
        state.on_buffer(&mut io, None);
        assert_eq!(
            state.events.capacity(),
            cap_before,
            "callback grew events capacity — `max_events_for_buffer` undersized?"
        );
    }
}
