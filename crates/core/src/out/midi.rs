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

#[cfg(test)]
mod tests {
    use super::*;

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
}
