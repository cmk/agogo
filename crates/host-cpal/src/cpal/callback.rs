//! Audio callback hot loop — `CallbackState::on_buffer`.
//!
//! This is a thin wrapper around an N-channel
//! [`agogo::core::control::Playhead`] plus the [`RtProducer`] that
//! pushes onto the SPSC ring. All scheduling +
//! rendering logic now lives inside `Playhead::on_buffer`; the
//! callback is left with `feed → schedule → render → enqueue`
//! reduced to one delegating call.
//!
//! No allocation, no locks. The audio thread spends time only in
//! integer arithmetic and one ring-buffer push per emitted event.

use crate::cpal::control::RtProducer;
use agogo::core::conn::sample::SampleTime;
use agogo::core::control::Playhead;
use agogo::core::sink::audio::AudioIo;

/// State the audio thread owns by-value across the stream's
/// lifetime. Built on the control thread, moved into the cpal
/// callback closure, never touched from the control thread again
/// except via [`Playhead::stop_handle`].
///
/// The `R: SampleTime` parameter binds the [`Playhead`]'s rate at
/// compile time. The CLI dispatches it via a static match on
/// `--sr` (`S044 | S048 | S088 | S096 | S176 | S192`).
pub struct CallbackState<R: SampleTime> {
    /// N-channel orchestrator. Owns channels, phase source,
    /// transport policy, and the per-channel scratch buffer.
    pub playhead: Playhead<R>,
    /// SPSC producer onto the drain thread's ring. `Playhead`
    /// renders via this sink (`RtProducer: MidiSink`).
    pub producer: RtProducer,
}

impl<R: SampleTime> CallbackState<R> {
    /// Per-buffer entry point. Called by `CpalHost`'s data callback
    /// once per audio buffer. No allocation, no locks.
    ///
    /// Transport bytes are policy-driven inside [`Playhead`]; the
    /// callback no longer takes a `transport: Option<MidiRtByte>`
    /// parameter. `TransportPolicy` (Internal / LinkDriven / Scripted)
    /// decides what byte (if any) to emit each buffer.
    pub fn on_buffer(&mut self, io: &mut AudioIo) {
        self.playhead.on_buffer(io, &self.producer);
    }
}

/// Re-export of the canonical helper. The implementation moved to
/// [`agogo::core::control::event::max_events_for_buffer`] in
/// [`agogo::core::control::Playhead`] can size its pool without
/// depending on `host-cpal`. Kept here so existing call
/// sites (the demo CLI handler) compile unchanged.
pub use agogo::core::control::event::max_events_for_buffer;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpal::control::spsc;
    use agogo::core::channel::{Channel, ChannelCommon, MidiRole};
    use agogo::core::conn::fixed::Micro;
    use agogo::core::conn::sample::S048;
    use agogo::core::conn::tempo::Tempo;
    use agogo::core::control::TransportPolicy;
    use agogo::core::control::sync::PhaseSource;
    use agogo::core::time::grid::Grid;
    use agogo::core::time::swing::SwingConfig;
    use agogo::core::time::tbase::TBase;
    use agogo::core::time::tick::PPQN;
    use std::collections::VecDeque;

    fn build_state(
        bpm: Tempo,
        divider: Grid,
        frames: usize,
    ) -> (CallbackState<S048>, crate::cpal::control::ControlConsumer) {
        let (producer, consumer) = spsc(1024);
        let channel = Channel::Midi {
            common: ChannelCommon {
                divider,
                shuffle: SwingConfig {
                    resolution: TBase::T16,
                    amount: 0,
                },
                delay: Micro::ZERO,
                offset: Micro::ZERO,
                bar_multiplier: None,
            },
            role: MidiRole::Clock,
        };
        let playhead = Playhead::<S048>::new(
            vec![channel],
            PhaseSource::Internal { bpm },
            48_000,
            bpm,
            PPQN,
            // No transport bytes — preserves the demo's clock-only behaviour.
            TransportPolicy::Scripted {
                schedule: VecDeque::new(),
            },
            frames,
        );
        let state = CallbackState::<S048> { playhead, producer };
        (state, consumer)
    }

    /// Regression for `callback_emits_expected_clock_schedule`,
    /// pinned against the Playhead-backed callback. Driving
    /// `on_buffer` with `PhaseSource::Internal` at `(120 BPM,
    /// 48 kHz, T4 divider, 24_000 frames)` for 4 contiguous
    /// buffers produces SPSC messages at samples `{0, 24_000,
    /// 48_000, 72_000}`. This is the regression check that the
    /// host-cpal callback stays semantics-equivalent.
    #[test]
    fn callback_emits_expected_clock_schedule() {
        let (mut state, mut cons) = build_state(Tempo::from_bpm_integer(120), Grid::T4, 24_000);
        let input = vec![0.0_f32; 24_000];
        let mut output: [f32; 0] = []; // PCM ABI
        for b in 0..4u64 {
            let mut io = AudioIo::new(&input, &mut output, b * 24_000, 48_000, 24_000);
            state.on_buffer(&mut io);
        }
        let mut samples = Vec::new();
        while let Some(m) = cons.try_pop() {
            samples.push(m.at_sample);
        }
        assert_eq!(samples, vec![0, 24_000, 48_000, 72_000]);
    }

    /// Pin `max_events_for_buffer`'s contract: at the demo's
    /// canonical sizing, the bound covers the
    /// `tick_stream_into_no_realloc` test domain.
    #[test]
    fn max_events_for_buffer_covers_realistic_sizing() {
        // 4 096 frames is the typical cpal default; +16 slack.
        assert_eq!(max_events_for_buffer(4_096), 4_112);
        // 0 frames returns 16 (slack only) — defensive but harmless.
        assert_eq!(max_events_for_buffer(0), 16);
    }

    /// Callback alloc-free contract. The Playhead's `events_pool` is
    /// pre-sized in its constructor; this test checks the callback
    /// path doesn't grow it. Pins the allocation-free contract
    /// end-to-end.
    #[test]
    fn callback_does_not_realloc_events() {
        let (mut state, _cons) = build_state(Tempo::from_bpm_integer(120), Grid::T4, 24_000);
        let cap_before = state.playhead.max_events_per_buffer();
        let input = vec![0.0_f32; 24_000];
        let mut output: [f32; 0] = []; // PCM ABI
        let mut io = AudioIo::new(&input, &mut output, 0, 48_000, 24_000);
        state.on_buffer(&mut io);
        assert_eq!(
            state.playhead.max_events_per_buffer(),
            cap_before,
            "Playhead grew events pool capacity — `max_events_for_buffer` undersized?"
        );
    }
}
