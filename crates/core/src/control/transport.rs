//! layer: control
//! depends-on: sink, channel, time, conn
//!
//! N-channel orchestrator. Owned by host-side runners (host-cpal's
//! callback, the integration tests). Stateless w.r.t. the host —
//! holds only musical + transport state.
//!
//! `Playhead` is the N-channel runtime that:
//!
//! 1. Feeds input PCM into a [`PhaseSource`] (Internal / External
//!    PLL / Custom).
//! 2. Computes a single per-buffer transport byte from a
//!    [`TransportPolicy`] and a control-thread stop flag.
//! 3. Schedules + renders each channel's clock through
//!    [`render_midi_channel`], emitting the transport byte once ahead
//!    of the per-channel clock streams (transport bytes are global to
//!    the MIDI port, not per-channel).
//!
//! The audio thread is the only thread that drives `on_buffer`; the
//! control thread interacts only through the [`PlayheadStopHandle`]'s
//! [`AtomicBool`].

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::channel::{Channel, ScheduledEvent};
use crate::conn::sample::SampleTime;
use crate::conn::tempo::Tempo;
use crate::control::event::tick_stream_into;
use crate::control::sync::PhaseSource;
use crate::sink::audio::{AudioIo, render_audio_click_block};
use crate::sink::midi::{MidiRtByte, MidiSink, render_midi_channel};
use crate::time::conn::SampleTickConn;

const COMMAND_TRANSPORT_CAPACITY: usize = 128;

/// N-channel runtime state. Built on the control thread, moved into
/// the audio callback closure, never mutated from the control thread
/// thereafter except via the [`PlayheadStopHandle`].
pub struct Playhead<R: SampleTime> {
    /// All channels share one PhaseSource and one tick→sample
    /// conversion. Per-channel divider/swing/delay live inside each
    /// [`Channel`].
    ///
    /// **Crate-private** because `bar_counters` and `click_counters`
    /// are indexed in lock-step with this `Vec`. External mutation
    /// (push/remove/reorder) would either OOB-panic in `on_buffer`
    /// or silently associate counter state with the wrong channel.
    /// Construct via [`Playhead::new`] (which sizes the parallel
    /// counter vecs) and treat the channel set as immutable for
    /// the `Playhead`'s lifetime — matches the struct-level
    /// "never mutated from the control thread thereafter" contract.
    pub(crate) channels: Vec<Channel>,
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
    /// Per-channel pre-filter counter for `Channel.bar_multiplier`.
    /// Index parallels `channels`. Slot is meaningful only for
    /// channels with `bar_multiplier = Some(_)`. Counts every
    /// `tick_stream_into` event from this channel; the filter keeps
    /// only events where `counter % multiplier == 0`. Reset to 0
    /// when transport stops (see [`Playhead::on_buffer`]).
    bar_counters: Vec<u32>,
    /// Per-channel emitted-click counter for `MidiClickAccent`.
    /// Index parallels `channels`. Slot is meaningful only for
    /// `Channel::Midi { role: MidiRole::Click(_) }` channels.
    /// Threaded through [`render_midi_channel`] (audit P3, Plan 21)
    /// into the click-rendering path via its `Option<&mut u32>`
    /// counter parameter; advanced once per emitted Note On. Reset
    /// to 0 when transport stops.
    click_counters: Vec<u32>,
    /// Per-channel emitted-click counter for fixed audio-click
    /// accents. Index parallels `channels`; meaningful only for
    /// `Channel::Audio { role: AudioRole::Click }` channels.
    audio_click_counters: Vec<u32>,
    /// Cross-thread stop signal. `PlayheadStopHandle::request_stop`
    /// flips this; the next [`Playhead::on_buffer`] reads it and
    /// emits [`MidiRtByte::Stop`].
    stop_flag: Arc<AtomicBool>,
    /// Command-bridge transport bytes staged by the audio thread at
    /// the buffer boundary. Separate from `stop_flag`, which is the
    /// teardown latch.
    command_transport: CommandTransportQueue,
}

struct CommandTransportQueue {
    bytes: [MidiRtByte; COMMAND_TRANSPORT_CAPACITY],
    head: usize,
    len: usize,
}

impl CommandTransportQueue {
    const fn new() -> Self {
        Self {
            bytes: [MidiRtByte::Start; COMMAND_TRANSPORT_CAPACITY],
            head: 0,
            len: 0,
        }
    }

    fn push_back(&mut self, byte: MidiRtByte) -> bool {
        if self.len == self.bytes.len() {
            return false;
        }
        let tail = (self.head + self.len) % self.bytes.len();
        self.bytes[tail] = byte;
        self.len += 1;
        true
    }

    fn pop_front(&mut self) -> Option<MidiRtByte> {
        if self.len == 0 {
            return None;
        }
        let byte = self.bytes[self.head];
        self.head = (self.head + 1) % self.bytes.len();
        self.len -= 1;
        Some(byte)
    }

    fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn clear(&mut self) {
        self.head = 0;
        self.len = 0;
    }
}

/// Caller's transport policy:
///
/// - [`TransportPolicy::Internal`] — emit `Start` on first call to
///   `on_buffer`; emit `Stop` after [`PlayheadStopHandle::request_stop`]
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

/// Wraps [`TransportPolicy`] with the local runtime gate
/// (`running`) that controls whether clock events are emitted.
#[derive(Debug)]
pub struct TransportState {
    pub policy: TransportPolicy,
    /// Local clock gate. Set to `true` at construction. A teardown
    /// stop signal or an accepted command-driven Stop sets it to
    /// `false`; an accepted command-driven Start can set it back to
    /// `true` unless teardown has been requested. Policy-driven Stop
    /// bytes (`LinkDriven` transitions, `Scripted` schedules) do
    /// **not** clear this flag — they pass through as one-shot bytes,
    /// preserving the option to resume clock + transport later.
    ///
    /// While `false`, [`Playhead::on_buffer`] emits no clock events.
    /// Teardown is stronger than command stop because the stop flag
    /// stays set; command Start/Stop staging is rejected and the
    /// stream stays silent until the host audio stream is dropped.
    running: bool,
}

/// Result of trying to stage a command-driven transport byte.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum TransportCommandApply {
    Applied,
    UnsupportedPolicy,
    QueueFull,
    TeardownRequested,
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
    /// (latched into `Playhead::stop_flag`).
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
/// down. Flips an `AtomicBool` the next [`Playhead::on_buffer`] reads
/// with `Acquire` ordering. The audio callback emits a final
/// [`MidiRtByte::Stop`] at the buffer-start sample, then falls
/// silent — both clock events and transport bytes are suppressed
/// for that buffer and every subsequent buffer until the host
/// audio stream is dropped.
///
/// Cheaply cloneable; multiple threads (e.g. the Ctrl-C handler and
/// the main loop) can hold one each.
#[derive(Clone)]
pub struct PlayheadStopHandle {
    flag: Arc<AtomicBool>,
}

impl PlayheadStopHandle {
    /// Idempotent: every call sets the flag to `true`. Effects on
    /// the next `on_buffer`:
    ///
    /// 1. Emit `MidiRtByte::Stop` once at the buffer-start sample.
    /// 2. Skip the per-channel clock pass for that buffer and
    ///    every subsequent buffer — the stream falls silent.
    ///
    /// The `Playhead` itself keeps spinning (no panic, no
    /// allocation, no state corruption); silence is achieved by
    /// short-circuiting the clock-render path. Drop the host audio
    /// stream to actually tear down the cpal callback.
    pub fn request_stop(&self) {
        self.flag.store(true, Ordering::Release);
    }

    /// Read the current flag state. Mostly useful for test
    /// observability; the audio thread calls `Acquire` directly.
    pub fn is_stop_requested(&self) -> bool {
        self.flag.load(Ordering::Acquire)
    }
}

impl<R: SampleTime> Playhead<R> {
    /// Construct a new [`Playhead`]. `bpm` and `ppqn` configure the
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
        let cap = crate::control::event::max_events_for_buffer(buffer_frames);
        let n = channels.len();
        Self {
            channels,
            phase_source,
            stc: SampleTickConn::new(sr, bpm, ppqn),
            transport: TransportState::new(transport),
            events_pool: Vec::with_capacity(cap),
            bar_counters: vec![0; n],
            click_counters: vec![0; n],
            audio_click_counters: vec![0; n],
            stop_flag: Arc::new(AtomicBool::new(false)),
            command_transport: CommandTransportQueue::new(),
        }
    }

    /// Mint a control-thread handle. The CLI installs the `ctrlc`
    /// handler with a clone of the returned handle.
    pub fn stop_handle(&self) -> PlayheadStopHandle {
        PlayheadStopHandle {
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

    /// Apply a tempo change at a buffer boundary. This is the
    /// command-bridge path: the host consumes admitted tempo metadata,
    /// then updates the runtime before scheduling the buffer.
    pub fn apply_tempo(&mut self, bpm: Tempo) -> bool {
        if self.stc.bpm() == bpm {
            return false;
        }
        self.stc = SampleTickConn::new(self.stc.sr(), bpm, self.stc.ppqn());
        if let PhaseSource::Internal { bpm: source_bpm } = &mut self.phase_source {
            *source_bpm = bpm;
        }
        true
    }

    /// Apply a command-driven transport start. The next
    /// [`Self::on_buffer`] call emits the start byte for internal
    /// transport and resumes clock output.
    pub fn apply_transport_start(&mut self) -> TransportCommandApply {
        if self.stop_flag.load(Ordering::Acquire) {
            return TransportCommandApply::TeardownRequested;
        }
        if !matches!(self.transport.policy, TransportPolicy::Internal { .. }) {
            return TransportCommandApply::UnsupportedPolicy;
        }
        if self.command_transport.push_back(MidiRtByte::Start) {
            TransportCommandApply::Applied
        } else {
            TransportCommandApply::QueueFull
        }
    }

    /// Apply a command-driven transport stop. The next
    /// [`Self::on_buffer`] call emits Stop and suppresses subsequent
    /// clock output unless a later queued command starts transport
    /// again in FIFO order.
    pub fn apply_transport_stop(&mut self) -> TransportCommandApply {
        if self.stop_flag.load(Ordering::Acquire) {
            return TransportCommandApply::TeardownRequested;
        }
        if !matches!(self.transport.policy, TransportPolicy::Internal { .. }) {
            return TransportCommandApply::UnsupportedPolicy;
        }
        if self.command_transport.push_back(MidiRtByte::Stop) {
            TransportCommandApply::Applied
        } else {
            TransportCommandApply::QueueFull
        }
    }

    /// Buffer-driven dispatch. RT-safe: no allocations, no locks
    /// (assuming the `PhaseSource` and `MidiSink` impls obey the
    /// same contract — `RtProducer` does; the `LinkSession`
    /// adapter takes a sub-µs `Mutex` once per buffer per
    /// `LinkPhaseSource`'s docs).
    pub fn on_buffer(&mut self, io: &mut AudioIo, sink: &dyn MidiSink) {
        // Output buffers arrive with undefined content; when a host
        // opens output for audio-click rendering, write silence before
        // mixing scheduled clicks.
        io.output.fill(0.0_f32); // PCM ABI

        // 1. Feed PCM into the PhaseSource.
        self.phase_source
            .feed_samples(io.input, io.buffer_start_sample);

        // 2. Compute the per-buffer transport byte.
        let stop_pending = self.stop_flag.load(Ordering::Acquire);
        if stop_pending {
            self.command_transport.clear();
            if let Some(t) = self.transport.next_byte(true) {
                sink.send_at(&[t.status_byte()], io.buffer_start_sample);
            }
        } else if !self.command_transport.is_empty() {
            while let Some(command_transport) = self.command_transport.pop_front() {
                let transport = match command_transport {
                    MidiRtByte::Start => {
                        if let TransportPolicy::Internal { start_emitted } =
                            &mut self.transport.policy
                        {
                            *start_emitted = true;
                        }
                        self.transport.running = true;
                        Some(MidiRtByte::Start)
                    }
                    MidiRtByte::Stop => {
                        if self.transport.running {
                            self.transport.running = false;
                            Some(MidiRtByte::Stop)
                        } else {
                            None
                        }
                    }
                    MidiRtByte::Continue => Some(MidiRtByte::Continue),
                };
                if let Some(t) = transport {
                    sink.send_at(&[t.status_byte()], io.buffer_start_sample);
                }
            }
        } else if let Some(t) = self.transport.next_byte(false) {
            sink.send_at(&[t.status_byte()], io.buffer_start_sample);
        }

        // 3. If the host has requested teardown, skip clock for this
        //    buffer and all future buffers. The Stop byte (if any)
        //    has already been emitted above; the stream now stays
        //    silent until the cpal stream is dropped. See
        //    `TransportState::running` for the full latch contract.
        //    Per-channel counter state (bar_counters /
        //    click_counters) resets to 0 here so that any subsequent
        //    fresh `Playhead` (or future resume of this one) starts
        //    bar-multiplier filtering and click-accent placement
        //    from a known phase.
        if !self.transport.running {
            self.bar_counters.iter_mut().for_each(|c| *c = 0);
            self.click_counters.iter_mut().for_each(|c| *c = 0);
            self.audio_click_counters.iter_mut().for_each(|c| *c = 0);
            return;
        }

        // 5. Per-channel scheduling + rendering. Channels are
        //    independent so we can iterate them without cross-talk;
        //    `events_pool` is reused (cleared) between channels.
        for (idx, ch) in self.channels.iter().enumerate() {
            let common = ch.common();
            self.events_pool.clear();
            tick_stream_into(
                &mut self.events_pool,
                common,
                &self.stc,
                io.buffer_start_sample,
                io.frames,
            );
            // Apply bar_multiplier filter pre-render (Plan
            // 2026-04-25-03 T3): keep only every Nth event from
            // this channel's tick stream, advancing the per-channel
            // bar counter once per pre-filter event. Filter runs
            // before render dispatch so the renderer (clock or
            // click) sees only the kept ticks.
            if let Some(m) = common.bar_multiplier {
                let counter = &mut self.bar_counters[idx];
                let m = m.get() as u32;
                self.events_pool.retain(|_| {
                    let keep = *counter % m == 0;
                    *counter = counter.wrapping_add(1);
                    keep
                });
            }
            // Plan 21 (audit P3) dispatches on the outer Channel
            // variant so the typed `render_midi_channel` only ever
            // sees MIDI roles. `Din` / `Cv` channels have no
            // renderer in v0.1 — same effective behaviour as the
            // pre-P3 silent no-op, but now the lack-of-renderer is
            // visible at the dispatch site rather than buried in a
            // catch-all match arm inside the renderer.
            match ch {
                Channel::Midi {
                    common: midi_common,
                    role,
                } => {
                    render_midi_channel(
                        midi_common,
                        role,
                        &self.events_pool,
                        None, // transport byte already emitted globally
                        io.buffer_start_sample,
                        Some(&mut self.click_counters[idx]),
                        sink,
                    );
                }
                Channel::Audio { role, .. } => {
                    render_audio_click_block(
                        &self.events_pool,
                        role,
                        &mut self.audio_click_counters[idx],
                        io,
                    );
                }
                Channel::Din { .. } | Channel::Cv { .. } => {
                    // No renderer for these targets in v0.1; the
                    // sink is MIDI-only.
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channel::{AudioRole, ChannelCommon, MidiRole};
    use crate::conn::fixed::Micro;
    use crate::conn::sample::S048;
    use crate::sink::midi::{MIDI_CLOCK, MIDI_START, MIDI_STOP, TestSink};
    use crate::time::grid::Grid;
    use crate::time::swing::SwingConfig;
    use crate::time::tbase::TBase;
    use crate::time::tick::PPQN;
    use proptest::prelude::*;

    fn zero_channel(divider: Grid) -> Channel {
        Channel::Midi {
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
        }
    }

    fn drive_buffers<R: SampleTime>(
        playhead: &mut Playhead<R>,
        sink: &TestSink,
        n_buffers: u64,
        frames: usize,
        sr: u32,
    ) {
        let input = vec![0.0_f32; frames];
        let mut output: [f32; 0] = []; // PCM ABI
        for b in 0..n_buffers {
            let mut io = AudioIo::new(&input, &mut output, b * frames as u64, sr, frames);
            playhead.on_buffer(&mut io, sink);
        }
    }

    /// A single-channel Playhead with `PhaseSource::Internal { 120 BPM }`
    /// at 48 kHz, T4 divider, 24 000 frames, no transport, emits
    /// `0xF8` clock bytes at samples `{0, 24_000, 48_000, 72_000}` —
    /// the exact schedule `callback_emits_expected_clock_schedule`
    /// asserts on `CallbackState`.
    #[test]
    fn playhead_buffer_matches_plan13_demo() {
        let bpm = Tempo::from_bpm_integer(120);
        let mut playhead = Playhead::<S048>::new(
            vec![zero_channel(Grid::T4)],
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
        drive_buffers(&mut playhead, &sink, 4, 24_000, 48_000);
        let samples: Vec<u64> = sink
            .records()
            .into_iter()
            .filter(|r| r.bytes == vec![MIDI_CLOCK])
            .map(|r| r.at_sample)
            .collect();
        assert_eq!(samples, vec![0, 24_000, 48_000, 72_000]);
    }

    /// `TransportPolicy::Internal` emits exactly one `0xFA` at
    /// sample 0 of buffer 0, then exactly one `0xFC` at sample 0 of
    /// the buffer following the `request_stop()` call. No other
    /// transport bytes.
    ///
    /// Also verifies the **stop-clock-immediately** contract: once
    /// `request_stop()` has been observed, no `0xF8` clock events
    /// fire on the stop-buffer or any subsequent buffer. At T4
    /// divider / 120 BPM / 48 kHz, clock events would naturally
    /// land at samples 0, 24_000, 48_000, … — under the latch
    /// only the first survives because 24_000 lands inside the
    /// stop-buffer (samples 20_480..24_576).
    #[test]
    fn transport_internal_emits_start_then_stop() {
        let bpm = Tempo::from_bpm_integer(120);
        let mut playhead = Playhead::<S048>::new(
            vec![zero_channel(Grid::T4)],
            PhaseSource::Internal { bpm },
            48_000,
            bpm,
            PPQN,
            TransportPolicy::Internal {
                start_emitted: false,
            },
            4_096,
        );
        let stop = playhead.stop_handle();
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
            let mut io = AudioIo::new(&input, &mut output, b * frames as u64, 48_000, frames);
            playhead.on_buffer(&mut io, &sink);
        }

        let transport_records: Vec<(u64, u8)> = sink
            .records()
            .into_iter()
            .filter(|r| r.bytes.iter().any(|&b| b == MIDI_START || b == MIDI_STOP))
            .map(|r| (r.at_sample, r.bytes[0]))
            .collect();

        assert_eq!(
            transport_records,
            vec![(0, MIDI_START), (5 * frames as u64, MIDI_STOP)],
        );
        assert!(!playhead.is_running());

        // Clock-suppression assertion: only the clock at sample 0
        // survives; the next natural clock at sample 24_000 lands
        // in buffer 5 (samples 20_480..24_576), which is exactly
        // the buffer where the stop latch flipped — so the clock
        // is suppressed alongside the Stop byte.
        let clock_samples: Vec<u64> = sink
            .records()
            .into_iter()
            .filter(|r| r.bytes == vec![MIDI_CLOCK])
            .map(|r| r.at_sample)
            .collect();
        assert_eq!(
            clock_samples,
            vec![0],
            "clock should fall silent immediately after request_stop"
        );
    }

    #[test]
    fn command_start_does_not_cancel_teardown_stop() {
        let bpm = Tempo::from_bpm_integer(120);
        let mut playhead = Playhead::<S048>::new(
            vec![zero_channel(Grid::T4)],
            PhaseSource::Internal { bpm },
            48_000,
            bpm,
            PPQN,
            TransportPolicy::Internal {
                start_emitted: true,
            },
            4_096,
        );
        let stop = playhead.stop_handle();
        let sink = TestSink::new();
        let input = vec![0.0_f32; 4_096]; // PCM ABI
        let mut output: [f32; 0] = []; // PCM ABI

        stop.request_stop();
        assert_eq!(
            playhead.apply_transport_start(),
            TransportCommandApply::TeardownRequested
        );
        let mut io = AudioIo::new(&input, &mut output, 0, 48_000, 4_096);
        playhead.on_buffer(&mut io, &sink);

        let transport_records: Vec<u8> = sink
            .records()
            .into_iter()
            .filter_map(|r| r.bytes.first().copied())
            .filter(|b| *b == MIDI_START || *b == MIDI_STOP)
            .collect();
        assert_eq!(transport_records, vec![MIDI_STOP]);
        assert!(!playhead.is_running());
        assert!(stop.is_stop_requested());
    }

    proptest! {
        /// For an arbitrary `is_playing[0..N]` sequence, `LinkDriven`
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
            let mut playhead = Playhead::<S048>::new(
                vec![zero_channel(Grid::T4)],
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
            drive_buffers(&mut playhead, &sink, states.len() as u64, 4_096, 48_000);

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

    /// A manually-loaded schedule deterministically replays through
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
        let mut playhead = Playhead::<S048>::new(
            vec![zero_channel(Grid::T4)],
            PhaseSource::Internal { bpm },
            48_000,
            bpm,
            PPQN,
            TransportPolicy::Scripted { schedule },
            4_096,
        );
        let sink = TestSink::new();
        drive_buffers(&mut playhead, &sink, 4, 4_096, 48_000);

        let transport: Vec<(u64, u8)> = sink
            .records()
            .into_iter()
            .filter(|r| r.bytes.iter().any(|&b| b == MIDI_START || b == MIDI_STOP))
            .map(|r| (r.at_sample, r.bytes[0]))
            .collect();

        // The Scripted policy emits exactly the bytes the schedule
        // dictates: Start at buffer 1, Stop at buffer 3. Unlike the
        // `stop_pending` path, a Scripted Stop does NOT flip
        // `running` to false — it's a fixture byte, not a stop-and-
        // silence command. Subsequent buffers would keep draining
        // the (now-empty) schedule with `next_byte` returning None.
        assert_eq!(transport, vec![(4_096, MIDI_START), (3 * 4_096, MIDI_STOP)],);
        assert!(
            playhead.is_running(),
            "Scripted Stop should not flip running"
        );
    }

    proptest! {
        /// A K-channel Playhead emits the union of K independent
        /// single-channel runs. Channel-independence invariant — no
        /// cross-talk in scheduling or rendering.
        #[test]
        fn multi_channel_independent_dispatch(
            dividers in prop::collection::vec(
                prop::sample::select(&[
                    Grid::T4, Grid::T8, Grid::T16, Grid::T32,
                ]),
                1usize..=4,
            ),
            n_buffers in 1u64..=8,
        ) {
            let bpm = Tempo::from_bpm_integer(120);
            let frames = 4_096usize;

            // Multi-channel run.
            let mut multi = Playhead::<S048>::new(
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
            // collected, then sorted+merged. The Playhead's
            // per-buffer ordering is `for ch in channels { ... }`,
            // so within a buffer events appear in channel order.
            // Collect with channel-aware grouping.
            let mut reference: Vec<u64> = Vec::new();
            for b in 0..n_buffers {
                for d in &dividers {
                    let mut single = Playhead::<S048>::new(
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

    // ── Plan 2026-04-25-03: bar_multiplier + click counter tests ──

    use crate::channel::role::{MidiClickAccent, MidiClickConfig};
    use crate::conn::midi::{U4, U7};
    use crate::sink::midi::{MIDI_NOTE_OFF, MIDI_NOTE_ON};
    use core::num::{NonZeroU16, NonZeroU32};

    fn click_channel(
        divider: Grid,
        cfg: MidiClickConfig,
        bar_multiplier: Option<NonZeroU16>,
    ) -> Channel {
        Channel::Midi {
            common: ChannelCommon {
                divider,
                shuffle: SwingConfig {
                    resolution: TBase::T16,
                    amount: 0,
                },
                delay: Micro::ZERO,
                offset: Micro::ZERO,
                bar_multiplier,
            },
            role: MidiRole::Click(cfg),
        }
    }

    fn audio_click_channel(divider: Grid, bar_multiplier: Option<NonZeroU16>) -> Channel {
        Channel::Audio {
            common: ChannelCommon {
                divider,
                shuffle: SwingConfig {
                    resolution: TBase::T16,
                    amount: 0,
                },
                delay: Micro::ZERO,
                offset: Micro::ZERO,
                bar_multiplier,
            },
            role: AudioRole::Click,
        }
    }

    /// Plan 2026-04-25-03 spot check: a click channel with no
    /// `bar_multiplier` emits Note On at every scheduled tick
    /// produced by the divider. At T4 / 120 BPM / 48 kHz, that's
    /// samples `{0, 24_000, 48_000, 72_000}` over 4 buffers.
    #[test]
    fn click_channel_emits_note_on_per_divider_tick() {
        let bpm = Tempo::from_bpm_integer(120);
        let cfg = MidiClickConfig {
            note: U7(76),
            vel: U7(100),
            ch: U4(9),
            accent: None,
        };
        let mut playhead = Playhead::<S048>::new(
            vec![click_channel(Grid::T4, cfg, None)],
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
        drive_buffers(&mut playhead, &sink, 4, 24_000, 48_000);
        let on_samples: Vec<u64> = sink
            .records()
            .into_iter()
            .filter(|r| r.bytes[0] == (MIDI_NOTE_ON | 9))
            .map(|r| r.at_sample)
            .collect();
        assert_eq!(on_samples, vec![0, 24_000, 48_000, 72_000]);
    }

    #[test]
    fn audio_click_channel_writes_pcm_per_divider_tick() {
        let bpm = Tempo::from_bpm_integer(120);
        let mut playhead = Playhead::<S048>::new(
            vec![audio_click_channel(Grid::T4, None)],
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
        let input = vec![0.0_f32; 24_000]; // PCM ABI
        let mut output = vec![0.0_f32; 24_000]; // PCM ABI
        let mut io = AudioIo::new(&input, &mut output, 0, 48_000, 24_000);

        playhead.on_buffer(&mut io, &sink);

        assert!(io.output.iter().any(|&s| s != 0.0));
        assert_eq!(playhead.audio_click_counters[0], 1);
    }

    /// Helper for the bars-filter proptest + spot check: extract a
    /// channel's emitted "tick" sample positions from a TestSink,
    /// filtering by mode (clock channels emit `0xF8`; click
    /// channels emit `0x9X` Note Ons).
    fn collect_tick_samples(sink: &TestSink, mode_is_click: bool, ch: U4) -> Vec<u64> {
        let ch_byte: u8 = ch.into();
        sink.records()
            .into_iter()
            .filter(|r| {
                if mode_is_click {
                    r.bytes[0] == (MIDI_NOTE_ON | ch_byte)
                } else {
                    r.bytes == vec![MIDI_CLOCK]
                }
            })
            .map(|r| r.at_sample)
            .collect()
    }

    proptest! {
        /// Plan 2026-04-25-03 property
        /// `bars_filter_emits_every_nth_grid_event`: a `bars=N` channel
        /// emits exactly the i-th `tick_stream_into` event iff
        /// `i % N == 0`. Strategy varies divider across `Grid::ALL`
        /// and mode across clock/click.
        ///
        /// Domain notes:
        /// - `bars` is bounded `1..=8` so each iteration exercises
        ///   filtering on event sequences containing multiple
        ///   accept-then-reject cycles. With `bars > total_events`
        ///   only the very first event passes, which is true but
        ///   useless for differentiating filter implementations.
        ///   The full `NonZeroU16::MAX` boundary is covered by
        ///   `bars_filter_huge_n_keeps_only_first_event` below per
        ///   CLAUDE.md's "spot-check the un-sampled boundary" rule.
        /// - `n_buffers` capped at 4 to keep proptest runtime
        ///   reasonable across the 36 dividers (T512P generates
        ///   ~3840 events per bar).
        #[test]
        fn bars_filter_emits_every_nth_grid_event(
            divider in prop::sample::select(Grid::ALL.as_slice()),
            mode_is_click in any::<bool>(),
            bars in 1u16..=8,
            n_buffers in 1u64..=4,
        ) {
            let bpm = Tempo::from_bpm_integer(120);
            let frames: usize = 24_000; // 0.5 sec / buffer @ 48k
            let sr: u32 = 48_000;
            let mch: U4 = U4(9);

            let make_role = || {
                if mode_is_click {
                    MidiRole::Click(MidiClickConfig {
                        note: U7(76), vel: U7(100), ch: mch, accent: None,
                    })
                } else {
                    MidiRole::Clock
                }
            };
            let mk_channel = |bm: Option<NonZeroU16>| Channel::Midi {
                common: ChannelCommon {
                    divider,
                    shuffle: SwingConfig { resolution: TBase::T16, amount: 0 },
                    delay: Micro::ZERO,
                    offset: Micro::ZERO,
                    bar_multiplier: bm,
                },
                role: make_role(),
            };

            let mut m_un = Playhead::<S048>::new(
                vec![mk_channel(None)],
                PhaseSource::Internal { bpm },
                sr, bpm, PPQN,
                TransportPolicy::Scripted { schedule: VecDeque::new() },
                frames,
            );
            let mut m_fi = Playhead::<S048>::new(
                vec![mk_channel(Some(NonZeroU16::new(bars).unwrap()))],
                PhaseSource::Internal { bpm },
                sr, bpm, PPQN,
                TransportPolicy::Scripted { schedule: VecDeque::new() },
                frames,
            );

            let s_un = TestSink::new();
            let s_fi = TestSink::new();
            drive_buffers(&mut m_un, &s_un, n_buffers, frames, sr);
            drive_buffers(&mut m_fi, &s_fi, n_buffers, frames, sr);

            let on_un = collect_tick_samples(&s_un, mode_is_click, mch);
            let on_fi = collect_tick_samples(&s_fi, mode_is_click, mch);

            let expected: Vec<u64> = on_un
                .iter()
                .enumerate()
                .filter_map(|(i, &s)| (i as u64 % bars as u64 == 0).then_some(s))
                .collect();
            prop_assert_eq!(on_fi, expected,
                "divider={:?}, mode_is_click={}, bars={}, n_buffers={}",
                divider, mode_is_click, bars, n_buffers);
        }
    }

    /// Spot check at the un-sampled `bars` boundary
    /// (`NonZeroU16::MAX` = 65,535) per CLAUDE.md proptest-bound
    /// rule. With bars far larger than any realistic per-buffer
    /// event count, only the very first event ever satisfies
    /// `counter % bars == 0`; subsequent events are filtered out.
    #[test]
    fn bars_filter_huge_n_keeps_only_first_event() {
        let bpm = Tempo::from_bpm_integer(120);
        let cfg = MidiClickConfig {
            note: U7(76),
            vel: U7(100),
            ch: U4(9),
            accent: None,
        };
        let mut playhead = Playhead::<S048>::new(
            vec![click_channel(Grid::T16, cfg, NonZeroU16::new(u16::MAX))],
            PhaseSource::Internal { bpm },
            48_000,
            bpm,
            PPQN,
            TransportPolicy::Scripted {
                schedule: VecDeque::new(),
            },
            48_000, // 1 sec at 48k = ~16 sixteenth-note events
        );
        let sink = TestSink::new();
        drive_buffers(&mut playhead, &sink, 4, 48_000, 48_000);
        let on_samples = collect_tick_samples(&sink, true, U4(9));
        assert_eq!(
            on_samples.len(),
            1,
            "bars=u16::MAX must filter all but the first event, got {} clicks",
            on_samples.len(),
        );
        assert_eq!(on_samples[0], 0, "first event lands at sample 0");
    }

    proptest! {
        /// Plan 2026-04-25-03 properties
        /// `click_counter_resets_on_transport_stop` +
        /// `bars_counter_resets_on_transport_stop`: after
        /// `PlayheadStopHandle::request_stop` latches the running flag
        /// off and a subsequent buffer hits the stop arm, both
        /// counter vecs are reset to zero — regardless of how
        /// non-zero they were before. Strategy varies the divider,
        /// pre-stop drive duration, bars multiplier, and accent
        /// period so the reset arm fires from a wide range of
        /// pre-stop counter states.
        #[test]
        fn counters_reset_on_transport_stop(
            divider in prop::sample::select(Grid::ALL.as_slice()),
            n_buffers_before_stop in 1u64..=8,
            bars in 1u16..=8,
            every in 1u32..=8,
        ) {
            let bpm = Tempo::from_bpm_integer(120);
            let cfg = MidiClickConfig {
                note: U7(37), vel: U7(70), ch: U4(9),
                accent: Some(MidiClickAccent {
                    every: NonZeroU32::new(every).unwrap(),
                    note: U7(38), vel: U7(120),
                }),
            };
            let mut playhead = Playhead::<S048>::new(
                vec![click_channel(divider, cfg, NonZeroU16::new(bars))],
                PhaseSource::Internal { bpm },
                48_000, bpm, PPQN,
                TransportPolicy::Scripted { schedule: VecDeque::new() },
                24_000,
            );

            let sink = TestSink::new();
            drive_buffers(&mut playhead, &sink, n_buffers_before_stop, 24_000, 48_000);

            // Trigger transport stop. The next on_buffer will emit
            // Stop, latch running off, and reset the counters.
            playhead.stop_handle().request_stop();
            drive_buffers(&mut playhead, &sink, 1, 24_000, 48_000);

            prop_assert_eq!(
                playhead.bar_counters[0], 0,
                "bar counter must reset (divider={:?}, bars={}, before={})",
                divider, bars, n_buffers_before_stop,
            );
            prop_assert_eq!(
                playhead.click_counters[0], 0,
                "click counter must reset (divider={:?}, every={}, before={})",
                divider, every, n_buffers_before_stop,
            );

            // And further buffers beyond the stop must stay zero —
            // the !running early-return loops back through the
            // reset arm idempotently.
            drive_buffers(&mut playhead, &sink, 1, 24_000, 48_000);
            prop_assert_eq!(playhead.bar_counters[0], 0);
            prop_assert_eq!(playhead.click_counters[0], 0);
        }
    }

    #[test]
    fn audio_click_counter_resets_on_transport_stop() {
        let bpm = Tempo::from_bpm_integer(120);
        let mut playhead = Playhead::<S048>::new(
            vec![audio_click_channel(Grid::T4, None)],
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
        let input = vec![0.0_f32; 24_000]; // PCM ABI
        let mut output = vec![0.0_f32; 24_000]; // PCM ABI
        let mut io = AudioIo::new(&input, &mut output, 0, 48_000, 24_000);
        playhead.on_buffer(&mut io, &sink);
        assert!(playhead.audio_click_counters[0] > 0);

        playhead.stop_handle().request_stop();
        let mut io = AudioIo::new(&input, &mut output, 24_000, 48_000, 24_000);
        playhead.on_buffer(&mut io, &sink);

        assert_eq!(playhead.audio_click_counters[0], 0);
    }

    /// Plan 2026-04-25-03 spot check: `bar_multiplier` interacts
    /// correctly with click accent. A `grid=t1,bars=2,accent-every=2`
    /// click channel emits a click every 2 bars; the accent counter
    /// advances per *emitted* click (not per pre-filter event), so
    /// every 2nd emitted click is accented. Concretely: bars 0, 2,
    /// 4, 6 emit clicks; clicks 0, 2 (i.e. bars 0 and 4) are
    /// accented, clicks 1, 3 (bars 2 and 6) are not.
    #[test]
    fn bars_and_accent_compose_correctly() {
        let bpm = Tempo::from_bpm_integer(120);
        let cfg = MidiClickConfig {
            note: U7(37),
            vel: U7(70),
            ch: U4(9),
            accent: Some(MidiClickAccent {
                every: NonZeroU32::new(2).unwrap(),
                note: U7(38),
                vel: U7(120),
            }),
        };
        let mut playhead = Playhead::<S048>::new(
            vec![click_channel(
                Grid::T1,
                cfg,
                Some(NonZeroU16::new(2).unwrap()),
            )],
            PhaseSource::Internal { bpm },
            48_000,
            bpm,
            PPQN,
            TransportPolicy::Scripted {
                schedule: VecDeque::new(),
            },
            96_000,
        );
        let sink = TestSink::new();
        // 8 buffers = 8 bars; bars=2 → 4 emitted clicks; accent=2 →
        // first and third are accented.
        drive_buffers(&mut playhead, &sink, 8, 96_000, 48_000);
        let records = sink.records();
        let notes: Vec<u8> = records
            .iter()
            .filter(|r| r.bytes[0] == (MIDI_NOTE_ON | 9))
            .map(|r| r.bytes[1])
            .collect();
        // Same accent pattern echoes through the Note Off stream —
        // `render_midi_click_block` always emits a same-sample
        // Note Off matching the Note On's note number.
        let note_offs: Vec<u8> = records
            .iter()
            .filter(|r| r.bytes[0] == (MIDI_NOTE_OFF | 9))
            .map(|r| r.bytes[1])
            .collect();
        assert_eq!(notes, vec![38, 37, 38, 37]);
        assert_eq!(note_offs, vec![38, 37, 38, 37]);
    }
}
