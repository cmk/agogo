//! RT-safe async-to-audio control bridge.
//!
//! The async side writes through [`ControlProducer`]. The audio side
//! owns [`RtControlConsumer`] and reads once per buffer. Last-value
//! controls use atomics; ordered controls use an SPSC ring. The audio
//! side never locks and never allocates.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, PoisonError};

use agogo_core::conn::tempo::Tempo;

/// Ordered commands that must not silently coalesce.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ControlCommand {
    ChannelConfigure { channel: u32 },
    Start,
    Stop,
    Locate { tick: u32 },
}

/// Per-buffer scalar snapshot read by the audio callback.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct RtParams {
    pub tempo: Tempo,
}

#[derive(Debug)]
struct SharedParams {
    tempo_raw: AtomicU32,
}

impl SharedParams {
    fn new(tempo: Tempo) -> Self {
        Self {
            tempo_raw: AtomicU32::new(tempo.0),
        }
    }
}

/// Async/control-side bridge handle.
pub struct ControlProducer {
    shared: Arc<SharedParams>,
    producer: Mutex<rtrb::Producer<ControlCommand>>,
}

impl std::fmt::Debug for ControlProducer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ControlProducer")
            .field("tempo", &self.tempo())
            .finish_non_exhaustive()
    }
}

/// Audio-thread bridge handle.
pub struct RtControlConsumer {
    shared: Arc<SharedParams>,
    consumer: rtrb::Consumer<ControlCommand>,
}

impl std::fmt::Debug for RtControlConsumer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RtControlConsumer")
            .field("snapshot", &self.snapshot())
            .finish_non_exhaustive()
    }
}

#[derive(Debug, thiserror::Error, Eq, PartialEq)]
pub enum BridgeError {
    #[error("agogo control queue is full")]
    QueueFull,
    #[error("agogo control queue lock is poisoned")]
    QueuePoisoned,
}

impl<T> From<PoisonError<T>> for BridgeError {
    fn from(_: PoisonError<T>) -> Self {
        Self::QueuePoisoned
    }
}

/// Build a paired async producer / RT consumer.
pub fn spsc(capacity: usize, initial_tempo: Tempo) -> (ControlProducer, RtControlConsumer) {
    let (producer, consumer) = rtrb::RingBuffer::new(capacity);
    let shared = Arc::new(SharedParams::new(initial_tempo));
    (
        ControlProducer {
            shared: Arc::clone(&shared),
            producer: Mutex::new(producer),
        },
        RtControlConsumer { shared, consumer },
    )
}

impl ControlProducer {
    /// Store the latest tempo. The RT side observes it on its next
    /// per-buffer snapshot.
    pub fn set_tempo(&self, tempo: Tempo) {
        self.shared.tempo_raw.store(tempo.0, Ordering::Relaxed);
    }

    /// Read the latest tempo from the async side. Mainly for
    /// inverse-op capture and tests.
    pub fn tempo(&self) -> Tempo {
        Tempo(self.shared.tempo_raw.load(Ordering::Relaxed))
    }

    /// Push one ordered command without blocking. A full queue is a
    /// caller-visible error; it is not a silent drop.
    pub fn try_push(&self, command: ControlCommand) -> Result<(), BridgeError> {
        self.producer
            .lock()?
            .push(command)
            .map_err(|_| BridgeError::QueueFull)
    }
}

impl RtControlConsumer {
    /// Read last-value controls once per buffer.
    pub fn snapshot(&self) -> RtParams {
        RtParams {
            tempo: Tempo(self.shared.tempo_raw.load(Ordering::Relaxed)),
        }
    }

    /// Drain one ordered command. Non-blocking and allocation-free.
    pub fn try_pop(&mut self) -> Option<ControlCommand> {
        self.consumer.pop().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tempo_set_applies_within_one_buffer() {
        let (producer, consumer) = spsc(4, Tempo::from_bpm_integer(120));
        producer.set_tempo(Tempo::from_bpm_integer(140));
        assert_eq!(consumer.snapshot().tempo, Tempo::from_bpm_integer(140));
    }

    #[test]
    fn queue_full_returns_error() {
        let (producer, mut consumer) = spsc(1, Tempo::from_bpm_integer(120));
        producer
            .try_push(ControlCommand::Start)
            .expect("first push");
        assert_eq!(
            producer.try_push(ControlCommand::Stop),
            Err(BridgeError::QueueFull)
        );
        assert_eq!(consumer.try_pop(), Some(ControlCommand::Start));
        assert_eq!(consumer.try_pop(), None);
    }

    #[test]
    fn rt_consumer_reads_commands_in_fifo_order() {
        let (producer, mut consumer) = spsc(4, Tempo::from_bpm_integer(120));
        producer.try_push(ControlCommand::Start).expect("start");
        producer
            .try_push(ControlCommand::Locate { tick: 960 })
            .expect("locate");
        producer.try_push(ControlCommand::Stop).expect("stop");

        assert_eq!(consumer.try_pop(), Some(ControlCommand::Start));
        assert_eq!(
            consumer.try_pop(),
            Some(ControlCommand::Locate { tick: 960 })
        );
        assert_eq!(consumer.try_pop(), Some(ControlCommand::Stop));
        assert_eq!(consumer.try_pop(), None);
    }
}
