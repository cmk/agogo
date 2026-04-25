//! RT control plane — `rtrb` SPSC + drain thread.
//!
//! The audio callback (Plan 13 T4) cannot call
//! [`MidiSink::send_at`] directly: midir locks, CoreMIDI / JACK can
//! allocate, any of those would stall the audio thread. Instead the
//! callback enqueues a fixed-size [`MidiMessage`] into an `rtrb`
//! ring; a dedicated drain thread (spawned by
//! [`ControlConsumer::spawn_drain`]) dequeues and forwards to the
//! caller's [`MidiSink`].
//!
//! See `doc/designs/output.md` ("Two-phase dispatch") for the
//! upstream design rationale.

use agogo_core::out::midi::MidiSink;
use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;

/// Stack-allocated MIDI message payload. Plan 12 only emits
/// single-byte System Real-Time messages (`0xF8` / `0xFA` / `0xFB`
/// / `0xFC`); the 3-byte capacity leaves room for Plan 14's
/// `MidiCc` rendering without growing the ring's element size.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct MidiMessage {
    pub at_sample: u64,
    pub bytes: [u8; 3],
    /// Valid prefix length of `bytes`. Must be ≤ 3.
    pub bytes_len: u8,
}

impl MidiMessage {
    /// Build a `MidiMessage` from a slice. Truncates to 3 bytes if
    /// `msg` is longer (debug-asserts in development) — Plan 12's
    /// renderers always send ≤ 3 bytes, so the truncation path is
    /// reachable only via programmer error.
    pub fn from_slice(msg: &[u8], at_sample: u64) -> Self {
        debug_assert!(
            msg.len() <= 3,
            "MidiMessage capacity is 3 bytes; got {}",
            msg.len()
        );
        let mut bytes = [0u8; 3];
        let n = msg.len().min(3);
        bytes[..n].copy_from_slice(&msg[..n]);
        Self {
            at_sample,
            bytes,
            bytes_len: n as u8,
        }
    }

    /// Slice view of the valid prefix.
    pub fn as_slice(&self) -> &[u8] {
        &self.bytes[..self.bytes_len as usize]
    }
}

/// Construct a paired producer / consumer with a ring buffer of
/// `capacity` `MidiMessage` slots.
///
/// Default Plan 13 sizing: 1024. At 48 kHz / 120 BPM / PPQN 192 /
/// `divider = T32t` (24 PPQN MIDI clock), the producer emits ~48
/// messages per second; 1024 is ~20 s of buffered headroom. The
/// drain thread typically processes each message in well under a
/// millisecond, so the queue stays nearly empty in normal
/// operation; the headroom only matters for transient drain
/// stalls (e.g. a midir send that briefly blocks).
pub fn spsc(capacity: usize) -> (RtProducer, ControlConsumer) {
    let (prod, cons) = rtrb::RingBuffer::new(capacity);
    let dropped = Arc::new(AtomicU64::new(0));
    (
        RtProducer {
            inner: RefCell::new(prod),
            dropped: Arc::clone(&dropped),
        },
        ControlConsumer {
            inner: cons,
            _dropped: dropped,
        },
    )
}

/// Producer side of the control-plane SPSC. Lives on the audio
/// thread; never blocks, never allocates. On a full ring,
/// [`Self::send_at`] (and [`Self::try_push`]) increments the
/// dropped counter and drops the message — the audio thread MUST
/// NOT block waiting for the drain thread to catch up.
///
/// `RefCell` interior allows the [`MidiSink`] impl's `&self`
/// receiver to mutate the underlying `rtrb::Producer` without
/// `unsafe`. Sound because rtrb's producer is single-threaded by
/// design (it's the SP in SPSC) — `RtProducer` is `Send` but
/// `!Sync`, which is exactly the audio-thread ownership contract.
pub struct RtProducer {
    inner: RefCell<rtrb::Producer<MidiMessage>>,
    dropped: Arc<AtomicU64>,
}

impl RtProducer {
    /// Non-blocking enqueue. Returns `true` if the message landed
    /// in the ring; `false` (with `dropped_count` bumped) if the
    /// ring was full.
    pub fn try_push(&self, msg: MidiMessage) -> bool {
        match self.inner.borrow_mut().push(msg) {
            Ok(()) => true,
            Err(_) => {
                self.dropped.fetch_add(1, Ordering::Relaxed);
                false
            }
        }
    }

    /// Total messages dropped on overrun since this producer was
    /// constructed. Read from any thread that holds the producer
    /// — `Relaxed` is sufficient because the counter is
    /// informational (a health gauge), not a synchronization
    /// signal.
    pub fn dropped_count(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
}

impl MidiSink for RtProducer {
    fn send_at(&self, msg: &[u8], at_sample: u64) {
        self.try_push(MidiMessage::from_slice(msg, at_sample));
    }
}

/// Consumer side of the control-plane SPSC. Owns the drain thread
/// once [`Self::spawn_drain`] is called.
pub struct ControlConsumer {
    inner: rtrb::Consumer<MidiMessage>,
    /// Holds the `Arc<AtomicU64>` so the producer's `dropped`
    /// counter stays alive across the SPSC pair's lifetime, even
    /// if the consumer is dropped before the producer. Never read
    /// here directly.
    _dropped: Arc<AtomicU64>,
}

impl ControlConsumer {
    /// Spawn a drain thread that forwards every dequeued message
    /// to `sink`. Returns a [`DrainHandle`] whose `Drop` impl
    /// flushes any in-flight messages and joins the thread.
    ///
    /// The drain thread sleeps briefly between drains when the
    /// ring is empty to avoid busy-waiting; latency from message
    /// pushed to message sent is bounded by the sleep period
    /// (default 1 ms, well under midir's own ~1 ms USB-bus jitter
    /// so no perceptible additional latency).
    ///
    /// `sink` is `Arc<dyn MidiSink + Send + Sync>` — the explicit
    /// `Send + Sync` bounds let the `Arc` move into the drain
    /// thread. `MidiSink` itself only requires `Send` (per
    /// `doc/agogo.md` §5); call sites supplying back-end sinks
    /// (`MidirSink`, future `CoreMidiSink`, …) all satisfy
    /// `Send + Sync` in practice via their `Mutex`-wrapped state.
    pub fn spawn_drain(self, sink: Arc<dyn MidiSink + Send + Sync>) -> DrainHandle {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = Arc::clone(&stop);
        let mut consumer = self.inner;

        let thread = thread::Builder::new()
            .name("agogo-midi-drain".into())
            .spawn(move || {
                loop {
                    while let Ok(msg) = consumer.pop() {
                        sink.send_at(msg.as_slice(), msg.at_sample);
                    }
                    if stop_thread.load(Ordering::Acquire) {
                        // Final flush after stop signal — any
                        // last-minute pushes that landed during
                        // the previous drain pass are picked up
                        // here.
                        while let Ok(msg) = consumer.pop() {
                            sink.send_at(msg.as_slice(), msg.at_sample);
                        }
                        break;
                    }
                    thread::sleep(std::time::Duration::from_millis(1));
                }
            })
            .expect("agogo-midi-drain thread spawn");

        DrainHandle {
            stop,
            thread: Some(thread),
        }
    }
}

/// RAII handle for the drain thread. Dropping signals the thread
/// to stop, waits for it to flush + exit, and joins. A panicked
/// drain thread is swallowed by `JoinHandle::join`'s `Result` —
/// the process is about to lose the audio stream regardless.
pub struct DrainHandle {
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Drop for DrainHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agogo_core::out::midi::TestSink;
    use proptest::prelude::*;

    fn arb_msg() -> impl Strategy<Value = MidiMessage> {
        (any::<u64>(), any::<[u8; 3]>(), 0u8..=3).prop_map(
            |(at_sample, bytes, bytes_len)| MidiMessage {
                at_sample,
                bytes,
                bytes_len,
            },
        )
    }

    #[test]
    fn message_from_slice_round_trip() {
        let m = MidiMessage::from_slice(&[0xF8], 1024);
        assert_eq!(m.as_slice(), &[0xF8]);
        assert_eq!(m.at_sample, 1024);
    }

    #[test]
    fn rt_producer_impls_midi_sink_via_try_push() {
        let (prod, mut cons) = spsc(8);
        // Use the trait method to push, mirroring how Plan 12's
        // `render_clock_block` invokes the sink.
        MidiSink::send_at(&prod, &[0xF8], 24_000);
        let drained = cons.inner.pop().expect("ring should have one msg");
        assert_eq!(drained.at_sample, 24_000);
        assert_eq!(drained.as_slice(), &[0xF8]);
    }

    proptest! {
        /// Plan 13 property `spsc_push_pop_fifo`: bounded sequences
        /// of messages pushed into the ring emerge in FIFO order on
        /// the consumer side, with the count preserved up to the
        /// ring's effective capacity.
        #[test]
        fn spsc_push_pop_fifo(msgs in prop::collection::vec(arb_msg(), 0..32)) {
            let cap = 64;
            let (prod, mut cons) = spsc(cap);
            for m in &msgs {
                prop_assert!(prod.try_push(*m), "unexpected overrun");
            }
            prop_assert_eq!(prod.dropped_count(), 0);
            let mut out = Vec::with_capacity(msgs.len());
            while let Ok(m) = cons.inner.pop() {
                out.push(m);
            }
            prop_assert_eq!(out, msgs);
        }

        /// Plan 13 property `spsc_overrun_is_counted`: when N
        /// items push against a ring of capacity C with no
        /// consumer activity, every rejected push bumps
        /// `dropped_count()` by exactly 1.
        ///
        /// rtrb's *effective* capacity may be one less than the
        /// requested `cap` (one slot reserved for the empty-vs-full
        /// distinction), so the test computes the threshold
        /// dynamically by counting accept/reject responses rather
        /// than asserting `cap - n`.
        #[test]
        fn spsc_overrun_is_counted(
            cap in 1usize..=8,
            n in 0usize..=32,
        ) {
            let (prod, _cons) = spsc(cap);
            let mut accepted = 0usize;
            let mut rejected = 0usize;
            for _ in 0..n {
                let m = MidiMessage {
                    at_sample: 0,
                    bytes: [0; 3],
                    bytes_len: 0,
                };
                if prod.try_push(m) {
                    accepted += 1;
                } else {
                    rejected += 1;
                }
            }
            prop_assert_eq!(accepted + rejected, n);
            prop_assert_eq!(prod.dropped_count(), rejected as u64);
        }
    }

    /// Plan 13 property `drain_thread_forwards_all_messages`: the
    /// drain thread forwards every producer-pushed message to the
    /// sink in FIFO order, and `DrainHandle::drop` flushes the
    /// ring before joining.
    ///
    /// Implemented as a deterministic unit test (rather than a
    /// proptest) because it exercises a timing path; randomized
    /// sizing doesn't surface bugs that 100 messages don't.
    #[test]
    fn drain_thread_forwards_all_messages() {
        let (prod, cons) = spsc(256);
        let test_sink = Arc::new(TestSink::new());
        let drain_sink: Arc<dyn MidiSink + Send + Sync> = test_sink.clone();
        let drain = cons.spawn_drain(drain_sink);

        for i in 0..100u64 {
            assert!(prod.try_push(MidiMessage::from_slice(&[0xF8], i * 24_000)));
        }

        // Drop the handle — flushes + joins the drain thread. Once
        // `drop` returns, every message in the ring has been
        // forwarded to the sink.
        drop(drain);

        let recs = test_sink.records();
        assert_eq!(recs.len(), 100);
        for (i, r) in recs.iter().enumerate() {
            assert_eq!(r.at_sample, i as u64 * 24_000);
            assert_eq!(r.bytes, vec![0xF8]);
        }
    }
}
