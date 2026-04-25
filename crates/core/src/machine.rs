//! N-channel orchestrator. Owned by host-side runners (host-cpal's
//! callback, the integration tests). Stateless w.r.t. the host —
//! holds only musical + transport state.
//!
//! Plan 14 generalises Plan 13's single-channel `CallbackState` into
//! an N-channel `Machine` that:
//!
//! 1. Feeds input PCM into a [`PhaseSource`] (Internal / External
//!    PLL / Custom).
//! 2. Computes a single per-buffer transport byte from a
//!    [`TransportPolicy`] and a control-thread stop flag.
//! 3. Schedules + renders each channel's clock through Plan 12's
//!    [`render_channel_block`], emitting the transport byte once
//!    ahead of the per-channel clock streams (transport bytes are
//!    global to the MIDI port, not per-channel).
//!
//! The audio thread is the only thread that drives `on_buffer`; the
//! control thread interacts only through the [`MachineStopHandle`]'s
//! [`AtomicBool`].

pub mod spec;

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::channel::scheduler::tick_stream_into;
use crate::channel::{Channel, ScheduledEvent};
use crate::fxp::{SampleTime, Tempo};
use crate::host::AudioIo;
use crate::out::midi::{MidiRtByte, MidiSink, render_channel_block};
use crate::sync::PhaseSource;
use crate::time::conn::SampleTickConn;

pub use spec::{ChannelDev, ChannelSpec, ChannelSpecError};

/// N-channel runtime state. Built on the control thread, moved into
/// the audio callback closure, never mutated from the control thread
/// thereafter except via the [`MachineStopHandle`].
pub struct Machine<R: SampleTime> {
    /// All channels share one PhaseSource and one tick→sample
    /// conversion. Per-channel divider/swing/shift live inside each
    /// [`Channel`].
    pub channels: Vec<Channel>,
    /// Sample-rate-typed phase source. `R: SampleTime` binds the
    /// rate at compile time so the Internal/External arms inside
    /// `PhaseSource` can monomorphise.
    pub phase_source: PhaseSource<R>,
    /// Sample↔Tick conversion shared across channels.
    pub stc: SampleTickConn,
    /// Transport policy + running flag.
    pub transport: TransportState,
    /// Reused per-channel scratch buffer. Pre-sized to
    /// `max_events_for_buffer(buffer_frames)` so [`tick_stream_into`]
    /// never reallocates inside the audio callback.
    events_pool: Vec<ScheduledEvent>,
    /// Cross-thread stop signal. `MachineStopHandle::request_stop`
    /// flips this; the next [`Machine::on_buffer`] reads it and
    /// emits [`MidiRtByte::Stop`].
    stop_flag: Arc<AtomicBool>,
}

/// Caller's transport policy. Plan 14 ships three:
///
/// - [`TransportPolicy::Internal`] — emit `Start` on first call to
///   `on_buffer`; emit `Stop` after [`MachineStopHandle::request_stop`]
///   fires.
/// - [`TransportPolicy::LinkDriven`] — read `is_playing` from a
///   query closure each buffer; emit `Start` on false→true
///   transitions, `Stop` on true→false. `request_stop` still forces
///   a final `Stop`.
/// - [`TransportPolicy::Scripted`] — test fixture: bytes are a
///   pre-loaded deterministic schedule.
pub enum TransportPolicy {
    Internal {
        start_emitted: bool,
    },
    LinkDriven {
        prev_playing: bool,
        query: Box<dyn FnMut() -> bool + Send>,
    },
    Scripted {
        schedule: VecDeque<Option<MidiRtByte>>,
    },
}

impl std::fmt::Debug for TransportPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Internal { start_emitted } => f
                .debug_struct("Internal")
                .field("start_emitted", start_emitted)
                .finish(),
            Self::LinkDriven { prev_playing, .. } => f
                .debug_struct("LinkDriven")
                .field("prev_playing", prev_playing)
                .field("query", &"<closure>")
                .finish(),
            Self::Scripted { schedule } => f
                .debug_struct("Scripted")
                .field("schedule_len", &schedule.len())
                .finish(),
        }
    }
}

/// Wraps [`TransportPolicy`] with the `running` flag the policy
/// machinery uses to gate emissions after a Stop has fired.
#[derive(Debug)]
pub struct TransportState {
    pub policy: TransportPolicy,
    /// `true` until the first `Stop` is emitted; `false` thereafter.
    /// While `false`, no transport bytes are emitted regardless of
    /// policy — the stream stays clock-only.
    running: bool,
}

impl TransportState {
    pub fn new(policy: TransportPolicy) -> Self {
        Self {
            policy,
            running: true,
        }
    }

    /// Decide whether to emit a transport byte for the upcoming
    /// buffer. `stop_pending` is the control-thread's stop signal
    /// (latched into `Machine::stop_flag`).
    fn next_byte(&mut self, stop_pending: bool) -> Option<MidiRtByte> {
        // Stop request takes precedence: emit Stop once, go silent.
        if stop_pending && self.running {
            self.running = false;
            return Some(MidiRtByte::Stop);
        }
        if !self.running {
            return None;
        }
        match &mut self.policy {
            TransportPolicy::Internal { start_emitted } => {
                if !*start_emitted {
                    *start_emitted = true;
                    Some(MidiRtByte::Start)
                } else {
                    None
                }
            }
            TransportPolicy::LinkDriven {
                prev_playing,
                query,
            } => {
                let curr = query();
                let byte = match (*prev_playing, curr) {
                    (false, true) => Some(MidiRtByte::Start),
                    (true, false) => Some(MidiRtByte::Stop),
                    _ => None,
                };
                *prev_playing = curr;
                byte
            }
            TransportPolicy::Scripted { schedule } => schedule.pop_front().flatten(),
        }
    }
}

/// Control-thread handle for signalling the audio callback to wind
/// down. Flips an `AtomicBool` the next [`Machine::on_buffer`] reads
/// with `Acquire` ordering. The audio callback emits a final
/// [`MidiRtByte::Stop`] and falls silent — clock and transport bytes
/// alike — until the stream is torn down.
///
/// Cheaply cloneable; multiple threads (e.g. the Ctrl-C handler and
/// the main loop) can hold one each.
#[derive(Clone)]
pub struct MachineStopHandle {
    flag: Arc<AtomicBool>,
}

impl MachineStopHandle {
    /// Idempotent: every call sets the flag to `true`. Subsequent
    /// `on_buffer` calls see it once and emit `Stop` exactly once.
    pub fn request_stop(&self) {
        self.flag.store(true, Ordering::Release);
    }

    /// Read the current flag state. Mostly useful for test
    /// observability; the audio thread calls `Acquire` directly.
    pub fn is_stop_requested(&self) -> bool {
        self.flag.load(Ordering::Acquire)
    }
}

impl<R: SampleTime> Machine<R> {
    /// Construct a new [`Machine`]. `bpm` and `ppqn` configure the
    /// shared [`SampleTickConn`]; `buffer_frames` sizes the
    /// preallocated scratch buffer so the per-channel render path
    /// stays allocation-free.
    pub fn new(
        channels: Vec<Channel>,
        phase_source: PhaseSource<R>,
        sr: u32,
        bpm: Tempo,
        ppqn: u32,
        transport: TransportPolicy,
        buffer_frames: usize,
    ) -> Self {
        let cap = crate::channel::scheduler::max_events_for_buffer(buffer_frames);
        Self {
            channels,
            phase_source,
            stc: SampleTickConn::new(sr, bpm, ppqn),
            transport: TransportState::new(transport),
            events_pool: Vec::with_capacity(cap),
            stop_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Mint a control-thread handle. Plan 14's CLI installs the
    /// `ctrlc` handler with a clone of the returned handle.
    pub fn stop_handle(&self) -> MachineStopHandle {
        MachineStopHandle {
            flag: Arc::clone(&self.stop_flag),
        }
    }

    /// Pre-render upper bound on `events_pool` capacity. Used by
    /// tests + the CLI's allocation-free pre-flight check.
    pub fn max_events_per_buffer(&self) -> usize {
        self.events_pool.capacity()
    }

    /// Has the running flag been latched off (Stop emitted)?
    /// Test-only; production code should not branch on this.
    #[doc(hidden)]
    pub fn is_running(&self) -> bool {
        self.transport.running
    }

    /// Buffer-driven dispatch. RT-safe: no allocations, no locks
    /// (assuming the `PhaseSource` and `MidiSink` impls obey the
    /// same contract — Plan 13's `RtProducer` does; the `LinkSession`
    /// adapter takes a sub-µs `Mutex` once per buffer per
    /// `LinkPhaseSource`'s docs).
    pub fn on_buffer(&mut self, io: &mut AudioIo, sink: &dyn MidiSink) {
        // 1. Feed PCM into the PhaseSource.
        self.phase_source
            .feed_samples(io.input, io.buffer_start_sample);

        // 2. Compute the per-buffer transport byte.
        let stop_pending = self.stop_flag.load(Ordering::Acquire);
        let transport = self.transport.next_byte(stop_pending);

        // 3. Emit transport once, ahead of all channels' clock.
        //    Transport bytes are global to the MIDI port (one stream
        //    per port shared by all channels in v0.1).
        if let Some(t) = transport {
            sink.send_at(&[t.status_byte()], io.buffer_start_sample);
        }

        // 4. Per-channel scheduling + rendering. Channels are
        //    independent so we can iterate them without cross-talk;
        //    `events_pool` is reused (cleared) between channels.
        for ch in &self.channels {
            self.events_pool.clear();
            tick_stream_into(
                &mut self.events_pool,
                ch,
                &self.stc,
                io.buffer_start_sample,
                io.frames,
            );
            render_channel_block(
                ch,
                &self.events_pool,
                None, // transport byte already emitted globally
                io.buffer_start_sample,
                sink,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channel::ChannelMode;
    use crate::fxp::{Micro, S48};
    use crate::out::midi::{MIDI_CLOCK, MIDI_START, MIDI_STOP, TestSink};
    use crate::time::swing::SwingConfig;
    use crate::time::tbase::TBase;
    use crate::time::tick::PPQN;
    use proptest::prelude::*;

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

    fn drive_buffers<R: SampleTime>(
        machine: &mut Machine<R>,
        sink: &TestSink,
        n_buffers: u64,
        frames: usize,
        sr: u32,
    ) {
        let input = vec![0.0_f32; frames];
        let mut output: [f32; 0] = []; // PCM ABI
        for b in 0..n_buffers {
            let mut io = AudioIo::new(
                &input,
                &mut output,
                b * frames as u64,
                sr,
                frames,
            );
            machine.on_buffer(&mut io, sink);
        }
    }

    /// Plan 14 property `machine_buffer_matches_plan13_demo`: a
    /// single-channel Machine with `PhaseSource::Internal { 120 BPM }`
    /// at 48 kHz, T4 divider, 24 000 frames, no transport, emits
    /// `0xF8` clock bytes at samples `{0, 24_000, 48_000, 72_000}` —
    /// the exact schedule Plan 13's `callback_emits_expected_clock_schedule`
    /// asserts on `CallbackState`.
    #[test]
    fn machine_buffer_matches_plan13_demo() {
        let bpm = Tempo::from_bpm_integer(120);
        let mut machine = Machine::<S48>::new(
            vec![zero_channel(TBase::T4)],
            PhaseSource::Internal { bpm },
            48_000,
            bpm,
            PPQN,
            TransportPolicy::Scripted {
                schedule: VecDeque::new(),
            },
            24_000,
        );
        let sink = TestSink::new();
        drive_buffers(&mut machine, &sink, 4, 24_000, 48_000);
        let samples: Vec<u64> = sink
            .records()
            .into_iter()
            .filter(|r| r.bytes == vec![MIDI_CLOCK])
            .map(|r| r.at_sample)
            .collect();
        assert_eq!(samples, vec![0, 24_000, 48_000, 72_000]);
    }

    /// Plan 14 property `transport_internal_emits_start_then_stop`:
    /// `TransportPolicy::Internal` emits exactly one `0xFA` at
    /// sample 0 of buffer 0, then exactly one `0xFC` at sample 0 of
    /// the buffer following the `request_stop()` call. No other
    /// transport bytes.
    #[test]
    fn transport_internal_emits_start_then_stop() {
        let bpm = Tempo::from_bpm_integer(120);
        let mut machine = Machine::<S48>::new(
            vec![zero_channel(TBase::T4)],
            PhaseSource::Internal { bpm },
            48_000,
            bpm,
            PPQN,
            TransportPolicy::Internal {
                start_emitted: false,
            },
            4_096,
        );
        let stop = machine.stop_handle();
        let sink = TestSink::new();
        let frames = 4_096;
        let input = vec![0.0_f32; frames];
        let mut output: [f32; 0] = []; // PCM ABI

        // Buffer 0: expect Start at sample 0.
        // Buffers 1..=4: nothing transport-wise.
        // After buffer 4: request_stop().
        // Buffer 5: expect Stop at sample 5*frames.
        // Buffers 6..=15: nothing.
        for b in 0..16u64 {
            if b == 5 {
                stop.request_stop();
            }
            let mut io = AudioIo::new(
                &input,
                &mut output,
                b * frames as u64,
                48_000,
                frames,
            );
            machine.on_buffer(&mut io, &sink);
        }

        let transport_records: Vec<(u64, u8)> = sink
            .records()
            .into_iter()
            .filter(|r| {
                r.bytes
                    .iter()
                    .any(|&b| b == MIDI_START || b == MIDI_STOP)
            })
            .map(|r| (r.at_sample, r.bytes[0]))
            .collect();

        assert_eq!(
            transport_records,
            vec![(0, MIDI_START), (5 * frames as u64, MIDI_STOP)],
        );
        assert!(!machine.is_running());
    }

    proptest! {
        /// Plan 14 property `transport_link_driven_emits_on_transitions`:
        /// for an arbitrary `is_playing[0..N]` sequence, `LinkDriven`
        /// emits `0xFA` at false→true and `0xFC` at true→false
        /// transitions, no transport byte otherwise.
        #[test]
        fn transport_link_driven_emits_on_transitions(
            states in prop::collection::vec(any::<bool>(), 1..16),
        ) {
            let bpm = Tempo::from_bpm_integer(120);
            let queue: Arc<std::sync::Mutex<VecDeque<bool>>> =
                Arc::new(std::sync::Mutex::new(states.iter().copied().collect()));
            let q_for_query = Arc::clone(&queue);
            let mut machine = Machine::<S48>::new(
                vec![zero_channel(TBase::T4)],
                PhaseSource::Internal { bpm },
                48_000,
                bpm,
                PPQN,
                TransportPolicy::LinkDriven {
                    prev_playing: false,
                    query: Box::new(move || {
                        q_for_query.lock().unwrap().pop_front().unwrap_or(false)
                    }),
                },
                4_096,
            );
            let sink = TestSink::new();
            drive_buffers(&mut machine, &sink, states.len() as u64, 4_096, 48_000);

            let mut expected = Vec::new();
            let mut prev = false;
            for (i, &s) in states.iter().enumerate() {
                let at = (i as u64) * 4_096;
                match (prev, s) {
                    (false, true) => expected.push((at, MIDI_START)),
                    (true, false) => expected.push((at, MIDI_STOP)),
                    _ => {}
                }
                prev = s;
            }

            let actual: Vec<(u64, u8)> = sink
                .records()
                .into_iter()
                .filter(|r| {
                    r.bytes
                        .iter()
                        .any(|&b| b == MIDI_START || b == MIDI_STOP)
                })
                .map(|r| (r.at_sample, r.bytes[0]))
                .collect();
            prop_assert_eq!(actual, expected);
        }
    }

    /// Plan 14 property `transport_scripted_replays_schedule`:
    /// a manually-loaded schedule deterministically replays through
    /// `on_buffer`.
    #[test]
    fn transport_scripted_replays_schedule() {
        let bpm = Tempo::from_bpm_integer(120);
        let schedule = VecDeque::from(vec![
            None,
            Some(MidiRtByte::Start),
            None,
            Some(MidiRtByte::Stop),
        ]);
        let mut machine = Machine::<S48>::new(
            vec![zero_channel(TBase::T4)],
            PhaseSource::Internal { bpm },
            48_000,
            bpm,
            PPQN,
            TransportPolicy::Scripted { schedule },
            4_096,
        );
        let sink = TestSink::new();
        drive_buffers(&mut machine, &sink, 4, 4_096, 48_000);

        let transport: Vec<(u64, u8)> = sink
            .records()
            .into_iter()
            .filter(|r| {
                r.bytes
                    .iter()
                    .any(|&b| b == MIDI_START || b == MIDI_STOP)
            })
            .map(|r| (r.at_sample, r.bytes[0]))
            .collect();

        // After buffer 3, Stop has been emitted and `running` is
        // false. The Scripted schedule had drained anyway, so it
        // doesn't matter if more buffers came.
        assert_eq!(
            transport,
            vec![(4_096, MIDI_START), (3 * 4_096, MIDI_STOP)],
        );
    }

    proptest! {
        /// Plan 14 property `multi_channel_independent_dispatch`: a
        /// K-channel Machine emits the union of K independent
        /// single-channel runs. Channel-independence invariant — no
        /// cross-talk in scheduling or rendering.
        #[test]
        fn multi_channel_independent_dispatch(
            dividers in prop::collection::vec(
                prop::sample::select(&[
                    TBase::T4, TBase::T8, TBase::T16, TBase::T32,
                ]),
                1usize..=4,
            ),
            n_buffers in 1u64..=8,
        ) {
            let bpm = Tempo::from_bpm_integer(120);
            let frames = 4_096usize;

            // Multi-channel run.
            let mut multi = Machine::<S48>::new(
                dividers.iter().copied().map(zero_channel).collect(),
                PhaseSource::Internal { bpm },
                48_000,
                bpm,
                PPQN,
                TransportPolicy::Scripted {
                    schedule: VecDeque::new(),
                },
                frames,
            );
            let multi_sink = TestSink::new();
            drive_buffers(&mut multi, &multi_sink, n_buffers, frames, 48_000);

            let multi_clocks: Vec<u64> = multi_sink
                .records()
                .into_iter()
                .filter(|r| r.bytes == vec![MIDI_CLOCK])
                .map(|r| r.at_sample)
                .collect();

            // Reference: K independent single-channel runs, each
            // collected, then sorted+merged. The Machine's
            // per-buffer ordering is `for ch in channels { ... }`,
            // so within a buffer events appear in channel order.
            // Collect with channel-aware grouping.
            let mut reference: Vec<u64> = Vec::new();
            for b in 0..n_buffers {
                for d in &dividers {
                    let mut single = Machine::<S48>::new(
                        vec![zero_channel(*d)],
                        PhaseSource::Internal { bpm },
                        48_000,
                        bpm,
                        PPQN,
                        TransportPolicy::Scripted {
                            schedule: VecDeque::new(),
                        },
                        frames,
                    );
                    let sink = TestSink::new();
                    let input = vec![0.0_f32; frames];
                    let mut output: [f32; 0] = []; // PCM ABI
                    let mut io = AudioIo::new(
                        &input,
                        &mut output,
                        b * frames as u64,
                        48_000,
                        frames,
                    );
                    single.on_buffer(&mut io, &sink);
                    for r in sink.records() {
                        if r.bytes == vec![MIDI_CLOCK] {
                            reference.push(r.at_sample);
                        }
                    }
                }
            }

            prop_assert_eq!(multi_clocks, reference);
        }
    }
}
