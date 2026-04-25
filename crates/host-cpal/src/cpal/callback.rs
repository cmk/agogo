//! Audio callback hot loop — `CallbackState::on_buffer`.
//!
//! Plan 14 reshapes this from the Plan 13 single-channel
//! [`agogo_core::channel::Channel`] holder into a thin wrapper
//! around an N-channel [`agogo_core::machine::Machine`] plus the
//! [`RtProducer`] that pushes onto the SPSC ring. All scheduling +
//! rendering logic now lives inside `Machine::on_buffer`; the
//! callback is left with `feed → schedule → render → enqueue`
//! reduced to one delegating call.
//!
//! No allocation, no locks. The audio thread spends time only in
//! integer arithmetic and one ring-buffer push per emitted event.

use crate::cpal::control::RtProducer;
use agogo_core::fxp::SampleTime;
use agogo_core::host::AudioIo;
use agogo_core::machine::Machine;

/// State the audio thread owns by-value across the stream's
/// lifetime. Built on the control thread, moved into the cpal
/// callback closure, never touched from the control thread again
/// except via [`Machine::stop_handle`].
///
/// The `R: SampleTime` parameter binds the [`Machine`]'s rate at
/// compile time. The CLI dispatches it via a static match on
/// `--sr` (Plan 14 T5: `S44 | S48 | S88 | S96 | S176 | S192`).
pub struct CallbackState<R: SampleTime> {
    /// N-channel orchestrator. Owns channels, phase source,
    /// transport policy, and the per-channel scratch buffer.
    pub machine: Machine<R>,
    /// SPSC producer onto the drain thread's ring. `Machine`
    /// renders via this sink (`RtProducer: MidiSink`).
    pub producer: RtProducer,
}

impl<R: SampleTime> CallbackState<R> {
    /// Per-buffer entry point. Called by `CpalHost`'s data callback
    /// once per audio buffer. No allocation, no locks.
    ///
    /// Transport bytes are policy-driven inside [`Machine`]; the
    /// callback no longer takes a `transport: Option<MidiRtByte>`
    /// parameter. Plan 14 T0's `TransportPolicy` (Internal /
    /// LinkDriven / Scripted) decides what byte (if any) to emit
    /// each buffer.
    pub fn on_buffer(&mut self, io: &mut AudioIo) {
        self.machine.on_buffer(io, &self.producer);
    }
}

/// Re-export of the canonical helper. The implementation moved to
/// [`agogo_core::channel::scheduler::max_events_for_buffer`] in
/// Plan 14 so [`agogo_core::machine::Machine`] can size its pool
/// without depending on `host-cpal`. Kept here so existing call
/// sites (the demo CLI handler) compile unchanged.
pub use agogo_core::channel::scheduler::max_events_for_buffer;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpal::control::spsc;
    use agogo_core::channel::{Channel, ChannelMode};
    use agogo_core::fxp::{Micro, S48, Tempo};
    use agogo_core::machine::TransportPolicy;
    use agogo_core::sync::PhaseSource;
    use agogo_core::time::grid::Grid;
    use agogo_core::time::swing::SwingConfig;
    use agogo_core::time::tbase::TBase;
    use agogo_core::time::tick::PPQN;
    use std::collections::VecDeque;

    fn build_state(
        bpm: Tempo,
        divider: Grid,
        frames: usize,
    ) -> (
        CallbackState<S48>,
        crate::cpal::control::ControlConsumer,
    ) {
        let (producer, consumer) = spsc(1024);
        let channel = Channel {
            mode: ChannelMode::MidiClock,
            divider,
            shuffle: SwingConfig {
                resolution: TBase::T16,
                amount: 0,
            },
            shift: Micro::ZERO,
            offset: Micro::ZERO,
            snap_to_quantum: None,
        };
        let machine = Machine::<S48>::new(
            vec![channel],
            PhaseSource::Internal { bpm },
            48_000,
            bpm,
            PPQN,
            // No transport bytes — Plan 13 demo's exact behaviour.
            TransportPolicy::Scripted {
                schedule: VecDeque::new(),
            },
            frames,
        );
        let state = CallbackState::<S48> { machine, producer };
        (state, consumer)
    }

    /// Plan 13 property `callback_emits_expected_clock_schedule`,
    /// re-pinned against the Machine-backed callback. Driving
    /// `on_buffer` with `PhaseSource::Internal` at `(120 BPM,
    /// 48 kHz, T4 divider, 24_000 frames)` for 4 contiguous
    /// buffers produces SPSC messages at samples `{0, 24_000,
    /// 48_000, 72_000}`. The byte-for-byte match with Plan 13 is
    /// the regression check that the host-cpal callback rewrite
    /// stays semantics-equivalent.
    #[test]
    fn callback_emits_expected_clock_schedule() {
        let (mut state, mut cons) = build_state(
            Tempo::from_bpm_integer(120),
            Grid::T4,
            24_000,
        );
        let input = vec![0.0_f32; 24_000];
        let mut output: [f32; 0] = []; // PCM ABI
        for b in 0..4u64 {
            let mut io = AudioIo::new(
                &input,
                &mut output,
                b * 24_000,
                48_000,
                24_000,
            );
            state.on_buffer(&mut io);
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

    /// Plan 14 callback alloc-free contract. The Machine's
    /// `events_pool` is pre-sized in its constructor; this test
    /// checks the callback path doesn't grow it. Pins the
    /// allocation-free contract end-to-end.
    #[test]
    fn callback_does_not_realloc_events() {
        let (mut state, _cons) = build_state(
            Tempo::from_bpm_integer(120),
            Grid::T4,
            24_000,
        );
        let cap_before = state.machine.max_events_per_buffer();
        let input = vec![0.0_f32; 24_000];
        let mut output: [f32; 0] = []; // PCM ABI
        let mut io = AudioIo::new(&input, &mut output, 0, 48_000, 24_000);
        state.on_buffer(&mut io);
        assert_eq!(
            state.machine.max_events_per_buffer(),
            cap_before,
            "Machine grew events pool capacity — `max_events_for_buffer` undersized?"
        );
    }
}
