//! MIDI clock byte emission + the [`MidiSink`] trait.
//!
//! Plan 12 (v0.1 output-chain slot 1 of 3). Pure-logic: defines the
//! back-end-agnostic sink contract and emits `0xF8` clock bytes +
//! `0xFA`/`0xFB`/`0xFC` transport bytes. Real back-ends (midir,
//! CoreMIDI, JACK, …) live in sibling crates and implement
//! [`MidiSink`]. Plan 13 ships the first real back-end
//! (`crates/host-midi`).

// ── Status bytes (MIDI 1.0 §System Real-Time Messages) ──────────────

/// Timing clock. Emitted 24 × per quarter note by the master.
pub const MIDI_CLOCK: u8 = 0xF8;
/// Start playback from position 0.
pub const MIDI_START: u8 = 0xFA;
/// Resume playback from the current position.
pub const MIDI_CONTINUE: u8 = 0xFB;
/// Stop playback.
pub const MIDI_STOP: u8 = 0xFC;

// ── Core trait ──────────────────────────────────────────────────────

/// Back-end-agnostic MIDI output sink.
///
/// Each back-end converts `at_sample` to its native timebase inside
/// `send_at` — mach time for CoreMIDI, frame index for JACK,
/// `QueryPerformanceCounter` for WinMM (`doc/agogo.md` §5). The core
/// only ever sees monotonic sample counts.
///
/// **RT safety.** Implementations may allocate or take locks
/// (`midir` does both). Plan 13's `rt/control.rs` wires an `rtrb`
/// drain thread so the audio callback enqueues `(bytes, at_sample)`
/// pairs without calling `send_at` directly.
pub trait MidiSink: Send {
    fn send_at(&self, msg: &[u8], at_sample: u64);
}

// ── Synthetic in-memory sink for tests ──────────────────────────────

/// One capture produced by [`TestSink::send_at`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TestRecord {
    pub at_sample: u64,
    pub bytes: Vec<u8>,
}

/// In-memory [`MidiSink`] for tests. Stores every `send_at` call in
/// FIFO order. Not RT-safe (takes a `Mutex`); Plan 13's real
/// back-ends are the production path.
#[derive(Default, Debug)]
pub struct TestSink {
    inner: std::sync::Mutex<Vec<TestRecord>>,
}

impl TestSink {
    pub fn new() -> Self {
        Self::default()
    }

    /// Cloned snapshot of every record, in FIFO order.
    pub fn records(&self) -> Vec<TestRecord> {
        self.inner.lock().unwrap().clone()
    }

    pub fn clear(&self) {
        self.inner.lock().unwrap().clear();
    }

    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl MidiSink for TestSink {
    fn send_at(&self, msg: &[u8], at_sample: u64) {
        self.inner.lock().unwrap().push(TestRecord {
            at_sample,
            bytes: msg.to_vec(),
        });
    }
}

// ── Clock rendering ─────────────────────────────────────────────────

use crate::channel::ScheduledEvent;

/// Render a block of MidiClock [`ScheduledEvent`]s. Emits one
/// `0xF8` byte per event at its `sample_index`. No allocation — the
/// one-byte slice is stack-local per iteration.
pub fn render_clock_block(events: &[ScheduledEvent], sink: &dyn MidiSink) {
    for ev in events {
        sink.send_at(&[MIDI_CLOCK], ev.sample_index);
    }
}

// ── Transport bytes ─────────────────────────────────────────────────

/// Single-byte MIDI System Real-Time transport messages.
///
/// Plan 12 exposes the byte-level enum only. Plan 14's transport FSM
/// (`doc/designs/transport.md`) owns the higher-level `TransportEvent`
/// type (`Play`, `Stop`, `Locate`, `PhaseSource*`) and maps its
/// transitions down to these bytes per buffer.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum MidiRtByte {
    Start,
    Continue,
    Stop,
}

impl MidiRtByte {
    pub fn status_byte(self) -> u8 {
        match self {
            Self::Start => MIDI_START,
            Self::Continue => MIDI_CONTINUE,
            Self::Stop => MIDI_STOP,
        }
    }
}

/// Per-buffer render: emits the optional transport byte at
/// `buffer_start_sample` ahead of the clock stream, then the clock
/// events. Callers that want to place the transport byte mid-buffer
/// can call [`MidiSink::send_at`] directly with a custom sample
/// index.
pub fn render_buffer(
    events: &[ScheduledEvent],
    transport: Option<MidiRtByte>,
    buffer_start_sample: u64,
    sink: &dyn MidiSink,
) {
    if let Some(t) = transport {
        sink.send_at(&[t.status_byte()], buffer_start_sample);
    }
    render_clock_block(events, sink);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::tick::Tick;
    use proptest::prelude::*;

    fn ev(at_sample: u64) -> ScheduledEvent {
        ScheduledEvent {
            sample_index: at_sample,
            tick: Tick(0),
        }
    }

    #[test]
    fn status_byte_constants_match_spec() {
        assert_eq!(MIDI_CLOCK, 0xF8);
        assert_eq!(MIDI_START, 0xFA);
        assert_eq!(MIDI_CONTINUE, 0xFB);
        assert_eq!(MIDI_STOP, 0xFC);
    }

    #[test]
    fn test_sink_records_round_trip() {
        let sink = TestSink::new();
        assert!(sink.is_empty());
        sink.send_at(&[MIDI_CLOCK], 0);
        sink.send_at(&[MIDI_START], 1024);
        assert_eq!(sink.len(), 2);
        let recs = sink.records();
        assert_eq!(
            recs,
            vec![
                TestRecord {
                    at_sample: 0,
                    bytes: vec![MIDI_CLOCK],
                },
                TestRecord {
                    at_sample: 1024,
                    bytes: vec![MIDI_START],
                },
            ],
        );
        sink.clear();
        assert!(sink.is_empty());
    }

    /// `TestSink` is `Send + Sync` — two threads pushing 1 000 records
    /// each produce 2 000 total records with no loss, confirming the
    /// `Mutex`-backed design handles concurrent callers. The trait
    /// bound is `Send`; `TestSink` is `Sync` by virtue of its field
    /// types, which is what callers sharing an `Arc<TestSink>` across
    /// threads rely on.
    #[test]
    fn test_sink_send_across_threads() {
        use std::sync::Arc;

        let sink = Arc::new(TestSink::new());
        let s1 = Arc::clone(&sink);
        let s2 = Arc::clone(&sink);

        std::thread::scope(|scope| {
            scope.spawn(move || {
                for i in 0..1000 {
                    s1.send_at(&[MIDI_CLOCK], i);
                }
            });
            scope.spawn(move || {
                for i in 0..1000 {
                    s2.send_at(&[MIDI_CLOCK], 1_000_000 + i);
                }
            });
        });

        assert_eq!(sink.len(), 2_000);
    }

    // ── render_clock_block ────────────────────────────────────────

    #[test]
    fn render_clock_block_spot_check_four_events() {
        let evs = [ev(0), ev(24_000), ev(48_000), ev(72_000)];
        let sink = TestSink::new();
        render_clock_block(&evs, &sink);
        let recs = sink.records();
        assert_eq!(recs.len(), 4);
        for (r, e) in recs.iter().zip(evs.iter()) {
            assert_eq!(r.at_sample, e.sample_index);
            assert_eq!(r.bytes, vec![MIDI_CLOCK]);
        }
    }

    #[test]
    fn render_clock_block_empty_is_noop() {
        let sink = TestSink::new();
        render_clock_block(&[], &sink);
        assert!(sink.is_empty());
    }

    proptest! {
        /// Plan 12 property `clock_every_event_produces_one_record`:
        /// rendering N events produces exactly N records, each a
        /// single `0xF8` byte.
        #[test]
        fn clock_every_event_produces_one_record(
            samples in prop::collection::vec(any::<u64>(), 0..64),
        ) {
            let evs: Vec<ScheduledEvent> = samples.iter().copied().map(ev).collect();
            let sink = TestSink::new();
            render_clock_block(&evs, &sink);
            let recs = sink.records();
            prop_assert_eq!(recs.len(), evs.len());
            for r in &recs {
                prop_assert_eq!(r.bytes.as_slice(), &[MIDI_CLOCK]);
            }
        }

        /// Plan 12 property `clock_sample_order_preserved`: record
        /// `at_sample` values appear in the same FIFO order as input
        /// `ScheduledEvent.sample_index` values.
        #[test]
        fn clock_sample_order_preserved(
            samples in prop::collection::vec(any::<u64>(), 0..64),
        ) {
            let evs: Vec<ScheduledEvent> = samples.iter().copied().map(ev).collect();
            let sink = TestSink::new();
            render_clock_block(&evs, &sink);
            let emitted: Vec<u64> = sink.records().iter().map(|r| r.at_sample).collect();
            prop_assert_eq!(emitted, samples);
        }
    }

    // ── MidiRtByte + render_buffer ────────────────────────────────

    #[test]
    fn midi_rt_byte_status_values() {
        assert_eq!(MidiRtByte::Start.status_byte(), MIDI_START);
        assert_eq!(MidiRtByte::Continue.status_byte(), MIDI_CONTINUE);
        assert_eq!(MidiRtByte::Stop.status_byte(), MIDI_STOP);
    }

    #[test]
    fn render_buffer_transport_only_no_events() {
        let sink = TestSink::new();
        render_buffer(&[], Some(MidiRtByte::Start), 1024, &sink);
        let recs = sink.records();
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].at_sample, 1024);
        assert_eq!(recs[0].bytes, vec![MIDI_START]);
    }

    #[test]
    fn render_buffer_no_transport_just_clock_events() {
        let sink = TestSink::new();
        render_buffer(&[ev(0), ev(24_000)], None, 0, &sink);
        let recs = sink.records();
        assert_eq!(recs.len(), 2);
        for r in &recs {
            assert_eq!(r.bytes, vec![MIDI_CLOCK]);
        }
    }

    proptest! {
        /// Plan 12 property `midi_rt_byte_in_expected_range`: every
        /// variant maps to the MIDI 1.0 real-time range
        /// `{0xFA, 0xFB, 0xFC}`.
        #[test]
        fn midi_rt_byte_in_expected_range(
            variant in prop::sample::select(&[
                MidiRtByte::Start,
                MidiRtByte::Continue,
                MidiRtByte::Stop,
            ]),
        ) {
            let b = variant.status_byte();
            prop_assert!(b == MIDI_START || b == MIDI_CONTINUE || b == MIDI_STOP);
        }

        /// Plan 12 property `render_buffer_emits_rt_byte_first`: when
        /// `transport = Some(t)`, the first `TestSink` record is the
        /// one-byte `[t.status_byte()]` at `buffer_start_sample`, even
        /// if a clock event also shares that sample.
        #[test]
        fn render_buffer_emits_rt_byte_first(
            variant in prop::sample::select(&[
                MidiRtByte::Start,
                MidiRtByte::Continue,
                MidiRtByte::Stop,
            ]),
            buffer_start in any::<u64>(),
            samples in prop::collection::vec(any::<u64>(), 0..16),
        ) {
            let evs: Vec<ScheduledEvent> = samples.iter().copied().map(ev).collect();
            let sink = TestSink::new();
            render_buffer(&evs, Some(variant), buffer_start, &sink);
            let recs = sink.records();
            prop_assert_eq!(recs.len(), evs.len() + 1);
            prop_assert_eq!(recs[0].at_sample, buffer_start);
            prop_assert_eq!(recs[0].bytes.as_slice(), &[variant.status_byte()]);
        }
    }
}
