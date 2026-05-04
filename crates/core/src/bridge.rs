//! layer: bridge
//! depends-on: transport, event, snapshot
//!
//! RT-safe async-to-audio control bridge.
//!
//! The async side writes through [`ControlProducer`]. The audio side
//! owns [`ControlConsumer`] and reads once per buffer. Last-value
//! controls use atomics; ordered controls use an SPSC ring. The audio
//! side never locks and never allocates.

use std::fmt;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, PoisonError};

use crate::conn::tempo::Tempo;
use crate::transport::{Playhead, TransportCommandApply};
use rust_fsm::state_machine;

pub const MAX_SOURCE_ID_LEN: usize = 64;
pub const MAX_COALESCE_KEY_LEN: usize = 64;
const NO_PENDING_TEMPO: u64 = 0;
const WRITING_TEMPO_BIT: u64 = 1 << 62;
const CLAIMED_TEMPO_BIT: u64 = 1 << 63;
const TEMPO_GENERATION_MASK: u64 = WRITING_TEMPO_BIT - 1;
// Soft-side calls may spin briefly after publishing into a buffer
// boundary race, but must never peg a core waiting for RT progress.
const TEMPO_PRODUCER_SETTLE_SPINS: usize = 64;
// RT gets a small CAS retry budget against the single serialized
// tempo producer. If the producer keeps changing the slot for the
// whole budget, the callback leaves the slot untouched and reuses the
// last stable tempo rather than spending unbounded time in the audio
// thread.
const TEMPO_RT_CLAIM_RETRIES: usize = 8;

state_machine! {
    #[derive(Debug, Clone, PartialEq, Eq)]
    tempo_slot_fsm(Empty)

    Empty => {
        BeginWrite => Writing,
        MarkSnapshotEmpty => Empty,
    },
    Pending => {
        BeginWrite => Writing,
        Claim => Claimed,
        ClearStale => Empty,
    },
    Writing => {
        PublishReplacement => Pending,
        AbortToEmpty => Empty,
        AbortToPending => Pending,
        ClaimBackup => Claimed,
        MarkSnapshotEmpty => Empty,
    },
    Claimed => {
        BeginWrite => Writing,
        PublishReplacement => Pending,
        AbortToEmpty => Empty,
    },
}

type TempoSlotEvent = tempo_slot_fsm::Input;
type TempoSlotFsm = tempo_slot_fsm::StateMachine;
type TempoSlotState = tempo_slot_fsm::State;

/// Ordered commands that must not silently coalesce.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ControlCommand {
    ChannelConfigure { channel: u32 },
    Start,
    Stop,
    Locate { tick: u32 },
}

/// Semantic id assigned before a command enters the RT bridge.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct CommandId(pub u64);

impl CommandId {
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Fixed-capacity source id carried through the RT command envelope.
#[derive(Copy, Clone, Eq, PartialEq, Hash)]
pub struct SourceId {
    bytes: [u8; MAX_SOURCE_ID_LEN],
    len: u8,
}

impl SourceId {
    pub fn new(value: &str) -> Option<Self> {
        fixed_string::<MAX_SOURCE_ID_LEN>(value).map(|(bytes, len)| Self { bytes, len })
    }

    pub fn as_str(&self) -> &str {
        fixed_string_as_str(&self.bytes, self.len)
    }
}

impl Default for SourceId {
    fn default() -> Self {
        // boundary-panic-ok: fixed literal, not user input.
        Self::new("agogo.driver").expect("default source id fits fixed storage")
    }
}

impl fmt::Debug for SourceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("SourceId").field(&self.as_str()).finish()
    }
}

/// Optional coalescing key. Only last-value controls use this in v0.2.
#[derive(Copy, Clone, Eq, PartialEq, Hash)]
pub struct CoalesceKey {
    bytes: [u8; MAX_COALESCE_KEY_LEN],
    len: u8,
}

impl CoalesceKey {
    pub fn new(value: &str) -> Option<Self> {
        fixed_string::<MAX_COALESCE_KEY_LEN>(value).map(|(bytes, len)| Self { bytes, len })
    }

    pub fn tempo() -> Self {
        // boundary-panic-ok: fixed literal, not user input.
        Self::new("tempo").expect("tempo coalesce key fits fixed storage")
    }

    pub fn as_str(&self) -> &str {
        fixed_string_as_str(&self.bytes, self.len)
    }
}

impl fmt::Debug for CoalesceKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("CoalesceKey").field(&self.as_str()).finish()
    }
}

fn fixed_string<const N: usize>(value: &str) -> Option<([u8; N], u8)> {
    if value.len() > N || value.len() > u8::MAX as usize {
        return None;
    }
    let mut bytes = [0_u8; N];
    bytes[..value.len()].copy_from_slice(value.as_bytes());
    Some((bytes, value.len() as u8))
}

fn fixed_string_as_str(bytes: &[u8], len: u8) -> &str {
    std::str::from_utf8(&bytes[..usize::from(len)])
        // boundary-panic-ok: bytes only enter through `fixed_string(&str)`.
        .expect("fixed bridge string is built from utf-8 input")
}

/// Time domain attached to a command deadline.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum CommandTimeDomain {
    RtBuffer,
    HostTime,
    Tick,
    Link,
    Unknown,
}

impl CommandTimeDomain {
    pub fn parse(value: &str) -> Self {
        match value {
            "rt_buffer" => Self::RtBuffer,
            "host_time" => Self::HostTime,
            "tick" => Self::Tick,
            "link" => Self::Link,
            _ => Self::Unknown,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RtBuffer => "rt_buffer",
            Self::HostTime => "host_time",
            Self::Tick => "tick",
            Self::Link => "link",
            Self::Unknown => "unknown",
        }
    }
}

/// Deadline for a command. v0.2 accepts RT-buffer deadlines only.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct CommandDeadline {
    pub time_domain: CommandTimeDomain,
    pub buffer: u64,
}

impl CommandDeadline {
    pub const fn rt_buffer(buffer: u64) -> Self {
        Self {
            time_domain: CommandTimeDomain::RtBuffer,
            buffer,
        }
    }
}

/// Metadata required before any command may affect the RT side.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct AdmissionMetadata {
    pub command_id: CommandId,
    pub source_id: SourceId,
    pub deadline: CommandDeadline,
    pub coalesce_key: Option<CoalesceKey>,
}

impl AdmissionMetadata {
    pub fn next_buffer(command_id: CommandId, source_id: SourceId, current_epoch: u64) -> Self {
        Self {
            command_id,
            source_id,
            deadline: CommandDeadline::rt_buffer(current_epoch.saturating_add(1)),
            coalesce_key: None,
        }
    }

    pub fn tempo(command_id: CommandId, source_id: SourceId, current_epoch: u64) -> Self {
        Self {
            coalesce_key: Some(CoalesceKey::tempo()),
            ..Self::next_buffer(command_id, source_id, current_epoch)
        }
    }
}

/// Ordered RT queue payload.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct CommandEnvelope {
    pub metadata: AdmissionMetadata,
    pub command: ControlCommand,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum AdmissionStatus {
    Accepted,
    Rejected,
    Late,
}

impl AdmissionStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
            Self::Late => "late",
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum AdmissionRejectReason {
    QueueFull,
    QueuePoisoned,
    LateDeadline,
    UnsupportedTimeDomain,
    UnsupportedCommandClass,
}

impl AdmissionRejectReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::QueueFull => "queue_full",
            Self::QueuePoisoned => "queue_poisoned",
            Self::LateDeadline => "late_deadline",
            Self::UnsupportedTimeDomain => "unsupported_time_domain",
            Self::UnsupportedCommandClass => "unsupported_command_class",
        }
    }
}

/// Structured result of one admission attempt.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct AdmissionOutcome {
    pub status: AdmissionStatus,
    pub metadata: AdmissionMetadata,
    pub reason: Option<AdmissionRejectReason>,
}

impl AdmissionOutcome {
    pub const fn accepted(metadata: AdmissionMetadata) -> Self {
        Self {
            status: AdmissionStatus::Accepted,
            metadata,
            reason: None,
        }
    }

    pub const fn rejected(metadata: AdmissionMetadata, reason: AdmissionRejectReason) -> Self {
        Self {
            status: AdmissionStatus::Rejected,
            metadata,
            reason: Some(reason),
        }
    }

    pub const fn late(metadata: AdmissionMetadata) -> Self {
        Self {
            status: AdmissionStatus::Late,
            metadata,
            reason: Some(AdmissionRejectReason::LateDeadline),
        }
    }

    pub const fn is_accepted(self) -> bool {
        matches!(self.status, AdmissionStatus::Accepted)
    }
}

/// RT-side command drain result.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum RtCommandDrain {
    Command(CommandEnvelope),
    MissedDeadline(CommandEnvelope),
}

/// Result of applying bridge state to a `Playhead` at one RT buffer boundary.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct CommandApplyReport {
    pub params: ControlParams,
    pub tempo_updated: bool,
    pub applied_commands: u32,
    pub missed_deadlines: u32,
    pub unsupported_commands: u32,
    pub transport_queue_full: u32,
    pub teardown_rejected_commands: u32,
}

/// Per-buffer scalar snapshot read by the audio callback.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct ControlParams {
    pub tempo: Tempo,
    pub buffer_epoch: u64,
}

#[derive(Debug)]
struct SharedControlParams {
    tempo_raw: AtomicU32,
    pending_tempo_raw: AtomicU32,
    pending_tempo_deadline: AtomicU64,
    pending_tempo_state: AtomicU64,
    staged_tempo_raw: AtomicU32,
    staged_tempo_deadline: AtomicU64,
    backup_tempo_raw: AtomicU32,
    backup_tempo_deadline: AtomicU64,
    tempo_snapshot_epoch: AtomicU64,
    next_tempo_generation: AtomicU64,
    buffer_epoch: AtomicU64,
}

impl SharedControlParams {
    fn new(tempo: Tempo) -> Self {
        Self {
            tempo_raw: AtomicU32::new(tempo.0),
            pending_tempo_raw: AtomicU32::new(tempo.0),
            pending_tempo_deadline: AtomicU64::new(0),
            pending_tempo_state: AtomicU64::new(NO_PENDING_TEMPO),
            staged_tempo_raw: AtomicU32::new(tempo.0),
            staged_tempo_deadline: AtomicU64::new(0),
            backup_tempo_raw: AtomicU32::new(tempo.0),
            backup_tempo_deadline: AtomicU64::new(0),
            tempo_snapshot_epoch: AtomicU64::new(0),
            next_tempo_generation: AtomicU64::new(1),
            buffer_epoch: AtomicU64::new(0),
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct TempoWriteGuard {
    previous_generation: u64,
    previous_state: u64,
    writing_state: u64,
}

/// Async/control-side bridge handle.
pub struct ControlProducer {
    shared: Arc<SharedControlParams>,
    tempo_producer: Mutex<()>,
    producer: Mutex<rtrb::Producer<CommandEnvelope>>,
}

impl fmt::Debug for ControlProducer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ControlProducer")
            .field("tempo", &self.tempo())
            .field("buffer_epoch", &self.current_buffer_epoch())
            .finish_non_exhaustive()
    }
}

/// Audio-thread bridge handle.
pub struct ControlConsumer {
    shared: Arc<SharedControlParams>,
    consumer: rtrb::Consumer<CommandEnvelope>,
    deferred_command: Option<CommandEnvelope>,
}

impl fmt::Debug for ControlConsumer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ControlConsumer")
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
    #[error("agogo control command missed its deadline")]
    Late,
    #[error("agogo control command uses an unsupported time domain")]
    UnsupportedTimeDomain,
    #[error("agogo control command class is unsupported")]
    UnsupportedCommandClass,
}

impl<T> From<PoisonError<T>> for BridgeError {
    fn from(_: PoisonError<T>) -> Self {
        Self::QueuePoisoned
    }
}

/// Build a paired async producer / RT consumer.
pub fn spsc(capacity: usize, initial_tempo: Tempo) -> (ControlProducer, ControlConsumer) {
    let (producer, consumer) = rtrb::RingBuffer::new(capacity);
    let shared = Arc::new(SharedControlParams::new(initial_tempo));
    (
        ControlProducer {
            shared: Arc::clone(&shared),
            tempo_producer: Mutex::new(()),
            producer: Mutex::new(producer),
        },
        ControlConsumer {
            shared,
            consumer,
            deferred_command: None,
        },
    )
}

impl ControlProducer {
    /// Store the latest tempo. The RT side observes it on its next
    /// per-buffer snapshot.
    pub fn set_tempo(&self, tempo: Tempo) {
        self.shared.tempo_raw.store(tempo.0, Ordering::Release);
    }

    /// Admit a last-value tempo control.
    pub fn admit_tempo(&self, tempo: Tempo, metadata: AdmissionMetadata) -> AdmissionOutcome {
        if metadata.coalesce_key != Some(CoalesceKey::tempo()) {
            return AdmissionOutcome::rejected(
                metadata,
                AdmissionRejectReason::UnsupportedCommandClass,
            );
        }
        let Ok(_guard) = self.tempo_producer.lock() else {
            return AdmissionOutcome::rejected(metadata, AdmissionRejectReason::QueuePoisoned);
        };
        if let Some(outcome) = self.reject_if_not_admissible(metadata) {
            return outcome;
        }

        self.shared
            .staged_tempo_raw
            .store(tempo.0, Ordering::Release);
        self.shared
            .staged_tempo_deadline
            .store(metadata.deadline.buffer, Ordering::Release);
        let generation = self.next_tempo_generation();
        let write = self.begin_tempo_write();
        if !self.publish_tempo_write(write, generation, tempo, metadata.deadline.buffer) {
            self.abort_tempo_write(write);
            return AdmissionOutcome::late(metadata);
        }

        if metadata.deadline.buffer <= self.current_buffer_epoch()
            && !self.settle_published_tempo(generation, metadata.deadline.buffer)
        {
            return AdmissionOutcome::late(metadata);
        }
        AdmissionOutcome::accepted(metadata)
    }

    fn settle_published_tempo(&self, generation: u64, deadline: u64) -> bool {
        for _ in 0..TEMPO_PRODUCER_SETTLE_SPINS {
            let state = self.shared.pending_tempo_state.load(Ordering::Acquire);
            if state == claimed_tempo_state(generation) {
                return true;
            }
            if state != generation {
                return false;
            }
            if self.shared.tempo_snapshot_epoch.load(Ordering::Acquire) >= deadline {
                if self
                    .shared
                    .pending_tempo_state
                    .compare_exchange(
                        generation,
                        NO_PENDING_TEMPO,
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    )
                    .is_ok()
                {
                    return false;
                }
                continue;
            }
            std::hint::spin_loop();
        }

        self.cancel_published_tempo(generation)
    }

    fn cancel_published_tempo(&self, generation: u64) -> bool {
        loop {
            let state = self.shared.pending_tempo_state.load(Ordering::Acquire);
            if state == claimed_tempo_state(generation) {
                return true;
            }
            if state != generation {
                return false;
            }
            if self
                .shared
                .pending_tempo_state
                .compare_exchange(
                    generation,
                    NO_PENDING_TEMPO,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_ok()
            {
                return false;
            }
        }
    }

    /// Read the latest tempo from the async side. Mainly for
    /// inverse-op capture and tests.
    pub fn tempo(&self) -> Tempo {
        Tempo(self.shared.tempo_raw.load(Ordering::Acquire))
    }

    /// Return the most recent RT buffer epoch published by the
    /// consumer.
    pub fn current_buffer_epoch(&self) -> u64 {
        self.shared.buffer_epoch.load(Ordering::Acquire)
    }

    /// Default deadline for a command admitted at the current soft
    /// side instant.
    pub fn default_deadline_buffer(&self) -> u64 {
        self.current_buffer_epoch().saturating_add(1)
    }

    /// Push one ordered command without blocking.
    ///
    /// A full queue is a caller-visible error; it is not a silent
    /// drop. This compatibility path can also report a poisoned
    /// producer lock or a late default next-buffer deadline if the
    /// RT side crosses the deadline during a boundary race.
    pub fn try_push(&self, command: ControlCommand) -> Result<(), BridgeError> {
        let metadata = AdmissionMetadata::next_buffer(
            CommandId(0),
            SourceId::default(),
            self.current_buffer_epoch(),
        );
        bridge_result_for_outcome(self.admit_ordered(command, metadata))
    }

    /// Admit one ordered command envelope without blocking.
    pub fn admit_ordered(
        &self,
        command: ControlCommand,
        metadata: AdmissionMetadata,
    ) -> AdmissionOutcome {
        if let Some(outcome) = self.reject_if_not_admissible(metadata) {
            return outcome;
        }
        if metadata.coalesce_key.is_some() {
            return AdmissionOutcome::rejected(
                metadata,
                AdmissionRejectReason::UnsupportedCommandClass,
            );
        }
        match self.producer.lock() {
            Ok(mut producer) => producer
                .push(CommandEnvelope { metadata, command })
                .map(|()| AdmissionOutcome::accepted(metadata))
                .unwrap_or_else(|_| {
                    AdmissionOutcome::rejected(metadata, AdmissionRejectReason::QueueFull)
                }),
            Err(_) => AdmissionOutcome::rejected(metadata, AdmissionRejectReason::QueuePoisoned),
        }
    }

    fn reject_if_not_admissible(&self, metadata: AdmissionMetadata) -> Option<AdmissionOutcome> {
        if metadata.deadline.time_domain != CommandTimeDomain::RtBuffer {
            return Some(AdmissionOutcome::rejected(
                metadata,
                AdmissionRejectReason::UnsupportedTimeDomain,
            ));
        }
        if metadata.deadline.buffer <= self.current_buffer_epoch() {
            return Some(AdmissionOutcome::late(metadata));
        }
        None
    }

    fn next_tempo_generation(&self) -> u64 {
        let generation = self.shared.next_tempo_generation.fetch_update(
            Ordering::AcqRel,
            Ordering::Acquire,
            |current| {
                Some(
                    current
                        .checked_add(1)
                        .filter(|next| *next != 0 && *next < WRITING_TEMPO_BIT)
                        .unwrap_or(1),
                )
            },
        );
        generation
            // boundary-panic-ok: fetch_update closure always returns Some.
            .expect("tempo generation update closure always returns Some")
            .max(1)
    }

    fn begin_tempo_write(&self) -> TempoWriteGuard {
        loop {
            let state = self.shared.pending_tempo_state.load(Ordering::Acquire);
            let slot_state = tempo_slot_state(state);
            if matches!(slot_state, TempoSlotState::Writing) {
                std::hint::spin_loop();
                continue;
            }

            let previous_generation = if matches!(slot_state, TempoSlotState::Pending) {
                tempo_state_generation(state)
            } else {
                0
            };
            let previous_state = if previous_generation == 0 {
                NO_PENDING_TEMPO
            } else {
                state
            };
            if previous_generation != 0 {
                self.shared.backup_tempo_raw.store(
                    self.shared.pending_tempo_raw.load(Ordering::Acquire),
                    Ordering::Release,
                );
                self.shared.backup_tempo_deadline.store(
                    self.shared.pending_tempo_deadline.load(Ordering::Acquire),
                    Ordering::Release,
                );
            }

            let next = tempo_slot_transition(slot_state, TempoSlotEvent::BeginWrite);
            debug_assert!(matches!(next, TempoSlotState::Writing));
            let writing_state = writing_tempo_state(previous_generation);
            if self
                .shared
                .pending_tempo_state
                .compare_exchange(state, writing_state, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return TempoWriteGuard {
                    previous_generation,
                    previous_state,
                    writing_state,
                };
            }
        }
    }

    fn publish_tempo_write(
        &self,
        write: TempoWriteGuard,
        generation: u64,
        tempo: Tempo,
        deadline: u64,
    ) -> bool {
        loop {
            let state = self.shared.pending_tempo_state.load(Ordering::Acquire);
            if state == write.writing_state {
                if write.previous_generation != 0
                    && self.shared.backup_tempo_deadline.load(Ordering::Acquire)
                        <= self.current_buffer_epoch()
                {
                    return false;
                }
                self.store_pending_tempo(tempo, deadline);
                let next = tempo_slot_transition(
                    TempoSlotState::Writing,
                    TempoSlotEvent::PublishReplacement,
                );
                debug_assert!(matches!(next, TempoSlotState::Pending));
                if self
                    .shared
                    .pending_tempo_state
                    .compare_exchange(state, generation, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    return true;
                }
                continue;
            }

            if write.previous_generation != 0
                && state == claimed_tempo_state(write.previous_generation)
            {
                if deadline <= self.current_buffer_epoch() {
                    return false;
                }
                self.store_pending_tempo(tempo, deadline);
                let next = tempo_slot_transition(
                    TempoSlotState::Claimed,
                    TempoSlotEvent::PublishReplacement,
                );
                debug_assert!(matches!(next, TempoSlotState::Pending));
                if self
                    .shared
                    .pending_tempo_state
                    .compare_exchange(state, generation, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    return true;
                }
                continue;
            }

            return false;
        }
    }

    fn store_pending_tempo(&self, tempo: Tempo, deadline: u64) {
        self.shared
            .pending_tempo_raw
            .store(tempo.0, Ordering::Release);
        self.shared
            .pending_tempo_deadline
            .store(deadline, Ordering::Release);
    }

    fn abort_tempo_write(&self, write: TempoWriteGuard) {
        loop {
            let state = self.shared.pending_tempo_state.load(Ordering::Acquire);
            if state != write.writing_state {
                return;
            }

            let event = if write.previous_state == NO_PENDING_TEMPO {
                TempoSlotEvent::AbortToEmpty
            } else {
                TempoSlotEvent::AbortToPending
            };
            let next = tempo_slot_transition(TempoSlotState::Writing, event);
            debug_assert_eq!(next, tempo_slot_state(write.previous_state));
            if self
                .shared
                .pending_tempo_state
                .compare_exchange(
                    state,
                    write.previous_state,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_ok()
            {
                return;
            }
        }
    }
}

fn bridge_result_for_outcome(outcome: AdmissionOutcome) -> Result<(), BridgeError> {
    match outcome.reason {
        None if outcome.is_accepted() => Ok(()),
        Some(AdmissionRejectReason::QueueFull) => Err(BridgeError::QueueFull),
        Some(AdmissionRejectReason::QueuePoisoned) => Err(BridgeError::QueuePoisoned),
        Some(AdmissionRejectReason::LateDeadline) => Err(BridgeError::Late),
        Some(AdmissionRejectReason::UnsupportedTimeDomain) => {
            Err(BridgeError::UnsupportedTimeDomain)
        }
        Some(AdmissionRejectReason::UnsupportedCommandClass) => {
            Err(BridgeError::UnsupportedCommandClass)
        }
        None => Err(BridgeError::UnsupportedCommandClass),
    }
}

impl ControlConsumer {
    /// Advance to the next RT buffer epoch and read last-value
    /// controls once for this buffer.
    pub fn begin_buffer(&self) -> ControlParams {
        let epoch = self
            .shared
            .buffer_epoch
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                Some(current.saturating_add(1))
            })
            // boundary-panic-ok: fetch_update closure always returns Some.
            .expect("buffer epoch update closure always returns Some")
            .saturating_add(1);
        let tempo = self
            .consume_pending_tempo(epoch)
            .unwrap_or_else(|| Tempo(self.shared.tempo_raw.load(Ordering::Acquire)));
        self.shared
            .tempo_snapshot_epoch
            .store(epoch, Ordering::Release);
        ControlParams {
            tempo,
            buffer_epoch: epoch,
        }
    }

    /// Read last-value controls without advancing the buffer epoch.
    pub fn snapshot(&self) -> ControlParams {
        ControlParams {
            tempo: Tempo(self.shared.tempo_raw.load(Ordering::Acquire)),
            buffer_epoch: self.current_buffer_epoch(),
        }
    }

    pub fn current_buffer_epoch(&self) -> u64 {
        self.shared.buffer_epoch.load(Ordering::Acquire)
    }

    fn consume_pending_tempo(&self, epoch: u64) -> Option<Tempo> {
        for _ in 0..TEMPO_RT_CLAIM_RETRIES {
            let state = self.shared.pending_tempo_state.load(Ordering::Acquire);
            match tempo_slot_state(state) {
                TempoSlotState::Empty | TempoSlotState::Claimed => return None,
                TempoSlotState::Pending => {
                    let deadline = self.shared.pending_tempo_deadline.load(Ordering::Acquire);
                    let tempo = Tempo(self.shared.pending_tempo_raw.load(Ordering::Acquire));
                    if deadline < epoch {
                        let next = tempo_slot_transition(
                            TempoSlotState::Pending,
                            TempoSlotEvent::ClearStale,
                        );
                        debug_assert!(matches!(next, TempoSlotState::Empty));
                        if self
                            .shared
                            .pending_tempo_state
                            .compare_exchange(
                                state,
                                NO_PENDING_TEMPO,
                                Ordering::AcqRel,
                                Ordering::Acquire,
                            )
                            .is_ok()
                        {
                            return None;
                        }
                        continue;
                    }
                    let next =
                        tempo_slot_transition(TempoSlotState::Pending, TempoSlotEvent::Claim);
                    debug_assert!(matches!(next, TempoSlotState::Claimed));
                    if self
                        .shared
                        .pending_tempo_state
                        .compare_exchange(
                            state,
                            claimed_tempo_state(tempo_state_generation(state)),
                            Ordering::AcqRel,
                            Ordering::Acquire,
                        )
                        .is_err()
                    {
                        continue;
                    }

                    self.shared.tempo_raw.store(tempo.0, Ordering::Release);
                    return Some(tempo);
                }
                TempoSlotState::Writing => {
                    let staged_deadline = self.shared.staged_tempo_deadline.load(Ordering::Acquire);
                    let previous_generation = tempo_state_generation(state);
                    if previous_generation == 0 {
                        if staged_deadline <= epoch {
                            if self.mark_writing_snapshot_empty(state) {
                                return None;
                            }
                            continue;
                        }
                        return None;
                    }

                    let deadline = self.shared.backup_tempo_deadline.load(Ordering::Acquire);
                    let tempo = Tempo(self.shared.backup_tempo_raw.load(Ordering::Acquire));
                    if deadline < epoch {
                        if staged_deadline <= epoch {
                            if self.mark_writing_snapshot_empty(state) {
                                return None;
                            }
                            continue;
                        }
                        return None;
                    }
                    let next =
                        tempo_slot_transition(TempoSlotState::Writing, TempoSlotEvent::ClaimBackup);
                    debug_assert!(matches!(next, TempoSlotState::Claimed));
                    if self
                        .shared
                        .pending_tempo_state
                        .compare_exchange(
                            state,
                            claimed_tempo_state(previous_generation),
                            Ordering::AcqRel,
                            Ordering::Acquire,
                        )
                        .is_err()
                    {
                        continue;
                    }

                    self.shared.tempo_raw.store(tempo.0, Ordering::Release);
                    return Some(tempo);
                }
            }
        }

        None
    }

    fn mark_writing_snapshot_empty(&self, state: u64) -> bool {
        let next =
            tempo_slot_transition(TempoSlotState::Writing, TempoSlotEvent::MarkSnapshotEmpty);
        debug_assert!(matches!(next, TempoSlotState::Empty));
        self.shared
            .pending_tempo_state
            .compare_exchange(state, NO_PENDING_TEMPO, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    /// Drain one ordered command envelope. Non-blocking and
    /// allocation-free.
    pub fn try_pop(&mut self) -> Option<CommandEnvelope> {
        self.deferred_command
            .take()
            .or_else(|| self.consumer.pop().ok())
    }

    /// Compatibility helper for callers that only care about the
    /// command payload.
    pub fn try_pop_command(&mut self) -> Option<ControlCommand> {
        self.try_pop().map(|envelope| envelope.command)
    }

    /// Drain one command that is due at the current buffer boundary.
    ///
    /// A FIFO head with a future deadline is retained inside the
    /// consumer and reported on a later call once its deadline is due
    /// or missed.
    pub fn drain_due_command(&mut self) -> Option<RtCommandDrain> {
        let envelope = self.try_pop()?;
        let current_epoch = self.current_buffer_epoch();
        match envelope.metadata.deadline.buffer.cmp(&current_epoch) {
            std::cmp::Ordering::Greater => {
                self.deferred_command = Some(envelope);
                None
            }
            std::cmp::Ordering::Less => Some(RtCommandDrain::MissedDeadline(envelope)),
            std::cmp::Ordering::Equal => Some(RtCommandDrain::Command(envelope)),
        }
    }
}

/// Apply one RT buffer worth of admitted control state to `playhead`.
///
/// This is the command-bridge companion to `Playhead::on_buffer`: call
/// it at the top of the audio buffer, then render the buffer. The
/// function does not allocate; it consumes the fixed-capacity bridge
/// queue and reports anything it cannot apply.
pub fn apply_control_to_playhead<R>(
    consumer: &mut ControlConsumer,
    playhead: &mut Playhead<R>,
) -> CommandApplyReport {
    let params = consumer.begin_buffer();
    let tempo_updated = playhead.apply_tempo(params.tempo);
    let mut report = CommandApplyReport {
        params,
        tempo_updated,
        applied_commands: 0,
        missed_deadlines: 0,
        unsupported_commands: 0,
        transport_queue_full: 0,
        teardown_rejected_commands: 0,
    };

    while let Some(drain) = consumer.drain_due_command() {
        match drain {
            RtCommandDrain::MissedDeadline(_) => {
                report.missed_deadlines = report.missed_deadlines.saturating_add(1);
            }
            RtCommandDrain::Command(envelope) => match envelope.command {
                ControlCommand::Start => {
                    report.record_transport_apply(playhead.apply_transport_start());
                }
                ControlCommand::Stop => {
                    report.record_transport_apply(playhead.apply_transport_stop());
                }
                ControlCommand::ChannelConfigure { .. } | ControlCommand::Locate { .. } => {
                    report.unsupported_commands = report.unsupported_commands.saturating_add(1);
                }
            },
        }
    }

    report
}

impl CommandApplyReport {
    fn record_transport_apply(&mut self, result: TransportCommandApply) {
        match result {
            TransportCommandApply::Applied => {
                self.applied_commands = self.applied_commands.saturating_add(1);
            }
            TransportCommandApply::UnsupportedPolicy => {
                self.unsupported_commands = self.unsupported_commands.saturating_add(1);
            }
            TransportCommandApply::QueueFull => {
                self.transport_queue_full = self.transport_queue_full.saturating_add(1);
            }
            TransportCommandApply::TeardownRequested => {
                self.teardown_rejected_commands = self.teardown_rejected_commands.saturating_add(1);
            }
        }
    }
}

fn tempo_slot_transition(state: TempoSlotState, event: TempoSlotEvent) -> TempoSlotState {
    let mut fsm = TempoSlotFsm::from_state(state);
    fsm.consume(&event)
        // boundary-panic-ok: transition table is static rust-fsm state.
        .expect("tempo slot transition is declared in rust-fsm");
    fsm.state().clone()
}

const fn tempo_state_generation(state: u64) -> u64 {
    state & TEMPO_GENERATION_MASK
}

const fn writing_tempo_state(generation: u64) -> u64 {
    WRITING_TEMPO_BIT | generation
}

const fn claimed_tempo_state(generation: u64) -> u64 {
    CLAIMED_TEMPO_BIT | generation
}

fn tempo_slot_state(state: u64) -> TempoSlotState {
    if state == NO_PENDING_TEMPO {
        TempoSlotState::Empty
    } else if state & CLAIMED_TEMPO_BIT != 0 {
        TempoSlotState::Claimed
    } else if state & WRITING_TEMPO_BIT != 0 {
        TempoSlotState::Writing
    } else {
        TempoSlotState::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TransportPolicy;
    use crate::channel::{Channel, ChannelCommon, MidiRole};
    use crate::conn::fixed::Micro;
    use crate::conn::rate::R048;
    use crate::control::PhaseSource;
    use crate::sink::audio::AudioIo;
    use crate::sink::midi::{MIDI_START, MIDI_STOP, TestSink};
    use crate::time::grid::Grid;
    use crate::time::swing::SwingConfig;
    use crate::time::tbase::TBase;
    use proptest::prelude::*;
    use std::collections::VecDeque;

    fn metadata(id: u64, deadline: u64) -> AdmissionMetadata {
        AdmissionMetadata {
            command_id: CommandId(id),
            source_id: SourceId::default(),
            deadline: CommandDeadline::rt_buffer(deadline),
            coalesce_key: None,
        }
    }

    fn domain_strategy() -> impl Strategy<Value = CommandTimeDomain> {
        prop_oneof![
            Just(CommandTimeDomain::RtBuffer),
            Just(CommandTimeDomain::HostTime),
            Just(CommandTimeDomain::Tick),
            Just(CommandTimeDomain::Link),
            Just(CommandTimeDomain::Unknown),
        ]
    }

    fn tempo_slot_event_strategy() -> impl Strategy<Value = TempoSlotEvent> {
        prop_oneof![
            Just(TempoSlotEvent::BeginWrite),
            Just(TempoSlotEvent::PublishReplacement),
            Just(TempoSlotEvent::AbortToEmpty),
            Just(TempoSlotEvent::AbortToPending),
            Just(TempoSlotEvent::ClaimBackup),
            Just(TempoSlotEvent::Claim),
            Just(TempoSlotEvent::ClearStale),
            Just(TempoSlotEvent::MarkSnapshotEmpty),
        ]
    }

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

    fn playhead(bpm: Tempo, transport: TransportPolicy) -> Playhead<R048> {
        Playhead::<R048>::new(
            vec![zero_channel(Grid::T4)],
            PhaseSource::Internal { bpm },
            48_000,
            bpm,
            transport,
            24_000,
        )
    }

    fn tempo_generation_strategy() -> impl Strategy<Value = u64> {
        prop_oneof![
            any::<u64>().prop_map(|value| (value & TEMPO_GENERATION_MASK).max(1)),
            Just(1),
            Just(TEMPO_GENERATION_MASK),
        ]
    }

    #[test]
    fn tempo_set_applies_within_one_buffer() {
        let (producer, consumer) = spsc(4, Tempo::from_bpm_integer(120));
        let outcome = producer.admit_tempo(
            Tempo::from_bpm_integer(140),
            AdmissionMetadata::tempo(CommandId(1), SourceId::default(), 0),
        );
        assert_eq!(outcome.status, AdmissionStatus::Accepted);
        assert_eq!(
            consumer.begin_buffer(),
            ControlParams {
                tempo: Tempo::from_bpm_integer(140),
                buffer_epoch: 1,
            }
        );
    }

    #[test]
    fn queue_full_rejects_with_reason_and_preserves_queue() {
        let (producer, mut consumer) = spsc(1, Tempo::from_bpm_integer(120));
        let first = producer.admit_ordered(ControlCommand::Start, metadata(1, 1));
        assert_eq!(first.status, AdmissionStatus::Accepted);

        let second = producer.admit_ordered(ControlCommand::Stop, metadata(2, 1));
        assert_eq!(second.status, AdmissionStatus::Rejected);
        assert_eq!(second.reason, Some(AdmissionRejectReason::QueueFull));

        assert_eq!(
            consumer.try_pop().map(|envelope| envelope.command),
            Some(ControlCommand::Start)
        );
        assert_eq!(consumer.try_pop(), None);
    }

    #[test]
    fn rt_consumer_reads_commands_in_fifo_order() {
        let (producer, mut consumer) = spsc(4, Tempo::from_bpm_integer(120));
        producer.admit_ordered(ControlCommand::Start, metadata(1, 1));
        producer.admit_ordered(ControlCommand::Locate { tick: 960 }, metadata(2, 1));
        producer.admit_ordered(ControlCommand::Stop, metadata(3, 1));

        assert_eq!(
            consumer.try_pop().map(|envelope| envelope.command),
            Some(ControlCommand::Start)
        );
        assert_eq!(
            consumer.try_pop().map(|envelope| envelope.command),
            Some(ControlCommand::Locate { tick: 960 })
        );
        assert_eq!(
            consumer.try_pop().map(|envelope| envelope.command),
            Some(ControlCommand::Stop)
        );
        assert_eq!(consumer.try_pop(), None);
    }

    #[test]
    fn stale_deadline_returns_late_and_does_not_enqueue() {
        let (producer, mut consumer) = spsc(4, Tempo::from_bpm_integer(120));
        consumer.begin_buffer();
        let outcome = producer.admit_ordered(ControlCommand::Start, metadata(1, 1));

        assert_eq!(outcome.status, AdmissionStatus::Late);
        assert_eq!(outcome.reason, Some(AdmissionRejectReason::LateDeadline));
        assert_eq!(consumer.try_pop(), None);
    }

    #[test]
    fn ordered_coalesce_key_is_rejected() {
        let (producer, mut consumer) = spsc(4, Tempo::from_bpm_integer(120));
        let mut meta = metadata(1, 1);
        meta.coalesce_key = Some(CoalesceKey::new("transport").expect("key fits"));

        let outcome = producer.admit_ordered(ControlCommand::Start, meta);

        assert_eq!(outcome.status, AdmissionStatus::Rejected);
        assert_eq!(
            outcome.reason,
            Some(AdmissionRejectReason::UnsupportedCommandClass)
        );
        assert_eq!(consumer.try_pop(), None);
    }

    #[test]
    fn tempo_rejects_non_tempo_coalesce_key() {
        let (producer, consumer) = spsc(4, Tempo::from_bpm_integer(120));
        let mut meta = AdmissionMetadata::tempo(CommandId(1), SourceId::default(), 0);
        meta.coalesce_key = Some(CoalesceKey::new("transport").expect("key fits"));

        let outcome = producer.admit_tempo(Tempo::from_bpm_integer(140), meta);

        assert_eq!(outcome.status, AdmissionStatus::Rejected);
        assert_eq!(
            outcome.reason,
            Some(AdmissionRejectReason::UnsupportedCommandClass)
        );
        assert_eq!(consumer.begin_buffer().tempo, Tempo::from_bpm_integer(120));
    }

    #[test]
    fn unsupported_time_domain_rejected_before_enqueue() {
        let (producer, mut consumer) = spsc(4, Tempo::from_bpm_integer(120));
        let mut meta = metadata(1, 1);
        meta.deadline.time_domain = CommandTimeDomain::HostTime;

        let outcome = producer.admit_ordered(ControlCommand::Start, meta);

        assert_eq!(outcome.status, AdmissionStatus::Rejected);
        assert_eq!(
            outcome.reason,
            Some(AdmissionRejectReason::UnsupportedTimeDomain)
        );
        assert_eq!(consumer.try_pop(), None);
    }

    #[test]
    fn missed_deadline_fault_is_visible_on_drain() {
        let (producer, mut consumer) = spsc(4, Tempo::from_bpm_integer(120));
        let outcome = producer.admit_ordered(ControlCommand::Start, metadata(1, 2));
        assert_eq!(outcome.status, AdmissionStatus::Accepted);
        consumer.begin_buffer();
        consumer.begin_buffer();
        consumer.begin_buffer();

        assert!(matches!(
            consumer.drain_due_command(),
            Some(RtCommandDrain::MissedDeadline(CommandEnvelope {
                command: ControlCommand::Start,
                ..
            }))
        ));
    }

    #[test]
    fn last_value_tempo_coalesces_by_key() {
        let (producer, consumer) = spsc(4, Tempo::from_bpm_integer(120));
        let first = producer.admit_tempo(
            Tempo::from_bpm_integer(130),
            AdmissionMetadata::tempo(CommandId(1), SourceId::default(), 0),
        );
        let second = producer.admit_tempo(
            Tempo::from_bpm_integer(140),
            AdmissionMetadata::tempo(CommandId(2), SourceId::default(), 0),
        );

        assert_eq!(first.status, AdmissionStatus::Accepted);
        assert_eq!(second.status, AdmissionStatus::Accepted);
        assert_eq!(consumer.begin_buffer().tempo, Tempo::from_bpm_integer(140));
    }

    #[test]
    fn stale_tempo_admission_does_not_change_scalar() {
        let (producer, consumer) = spsc(4, Tempo::from_bpm_integer(120));
        consumer.begin_buffer();

        let outcome = producer.admit_tempo(
            Tempo::from_bpm_integer(140),
            AdmissionMetadata::tempo(CommandId(1), SourceId::default(), 0),
        );

        assert_eq!(outcome.status, AdmissionStatus::Late);
        assert_eq!(producer.tempo(), Tempo::from_bpm_integer(120));
    }

    #[test]
    fn published_tempo_admission_is_not_revised_to_late() {
        let (producer, consumer) = spsc(4, Tempo::from_bpm_integer(120));

        let outcome = producer.admit_tempo(
            Tempo::from_bpm_integer(140),
            AdmissionMetadata::tempo(CommandId(1), SourceId::default(), 0),
        );

        assert_eq!(outcome.status, AdmissionStatus::Accepted);
        assert_eq!(outcome.reason, None);
        assert_eq!(outcome.metadata.deadline, CommandDeadline::rt_buffer(1));
        assert_eq!(consumer.begin_buffer().tempo, Tempo::from_bpm_integer(140));
    }

    #[test]
    fn tempo_write_in_progress_preserves_prior_pending() {
        let (producer, consumer) = spsc(4, Tempo::from_bpm_integer(120));
        let first = producer.admit_tempo(
            Tempo::from_bpm_integer(130),
            AdmissionMetadata::tempo(CommandId(1), SourceId::default(), 0),
        );
        assert_eq!(first.status, AdmissionStatus::Accepted);

        producer
            .shared
            .staged_tempo_raw
            .store(Tempo::from_bpm_integer(140).0, Ordering::Release);
        producer
            .shared
            .staged_tempo_deadline
            .store(1, Ordering::Release);
        let write = producer.begin_tempo_write();

        assert_eq!(consumer.begin_buffer().tempo, Tempo::from_bpm_integer(130));
        producer.abort_tempo_write(write);
        assert_eq!(producer.tempo(), Tempo::from_bpm_integer(130));
    }

    #[test]
    fn tempo_write_without_prior_pending_cannot_publish_after_snapshot() {
        let (producer, consumer) = spsc(4, Tempo::from_bpm_integer(120));
        let generation = producer.next_tempo_generation();
        producer
            .shared
            .staged_tempo_raw
            .store(Tempo::from_bpm_integer(140).0, Ordering::Release);
        producer
            .shared
            .staged_tempo_deadline
            .store(1, Ordering::Release);
        let write = producer.begin_tempo_write();

        assert_eq!(consumer.begin_buffer().tempo, Tempo::from_bpm_integer(120));
        assert!(!producer.publish_tempo_write(write, generation, Tempo::from_bpm_integer(140), 1));
        assert_eq!(producer.tempo(), Tempo::from_bpm_integer(120));
    }

    #[test]
    fn tempo_replacement_does_not_publish_over_due_backup() {
        let (producer, consumer) = spsc(4, Tempo::from_bpm_integer(120));
        let first = producer.admit_tempo(
            Tempo::from_bpm_integer(130),
            AdmissionMetadata::tempo(CommandId(1), SourceId::default(), 0),
        );
        assert_eq!(first.status, AdmissionStatus::Accepted);
        producer.shared.buffer_epoch.store(1, Ordering::Release);

        let generation = producer.next_tempo_generation();
        producer
            .shared
            .staged_tempo_raw
            .store(Tempo::from_bpm_integer(140).0, Ordering::Release);
        producer
            .shared
            .staged_tempo_deadline
            .store(2, Ordering::Release);
        let write = producer.begin_tempo_write();

        assert!(!producer.publish_tempo_write(write, generation, Tempo::from_bpm_integer(140), 2));
        producer.abort_tempo_write(write);
        assert_eq!(
            consumer.consume_pending_tempo(1),
            Some(Tempo::from_bpm_integer(130))
        );
    }

    #[test]
    fn published_tempo_settle_has_bounded_late_fallback() {
        let (producer, _consumer) = spsc(4, Tempo::from_bpm_integer(120));
        let generation = producer.next_tempo_generation();
        let write = producer.begin_tempo_write();
        assert!(producer.publish_tempo_write(write, generation, Tempo::from_bpm_integer(140), 1));
        producer.shared.buffer_epoch.store(1, Ordering::Release);

        assert!(!producer.settle_published_tempo(generation, 1));
        assert_eq!(
            producer.shared.pending_tempo_state.load(Ordering::Acquire),
            NO_PENDING_TEMPO
        );
    }

    #[test]
    fn compatibility_push_late_outcome_is_error() {
        let outcome = AdmissionOutcome::late(metadata(1, 0));
        assert_eq!(bridge_result_for_outcome(outcome), Err(BridgeError::Late));
    }

    #[test]
    fn max_deadline_ordered_admission_preserves_boundary() {
        let (producer, _consumer) = spsc(4, Tempo::from_bpm_integer(120));
        let outcome = producer.admit_ordered(ControlCommand::Start, metadata(1, u64::MAX));

        assert_eq!(outcome.status, AdmissionStatus::Accepted);
        assert_eq!(
            outcome.metadata.deadline,
            CommandDeadline::rt_buffer(u64::MAX)
        );
    }

    #[test]
    fn default_deadline_saturates_at_u64_max() {
        let (producer, _consumer) = spsc(4, Tempo::from_bpm_integer(120));
        producer
            .shared
            .buffer_epoch
            .store(u64::MAX, Ordering::Release);

        assert_eq!(producer.default_deadline_buffer(), u64::MAX);
    }

    #[test]
    fn begin_buffer_saturates_stored_epoch_at_u64_max() {
        let (_producer, consumer) = spsc(4, Tempo::from_bpm_integer(120));
        consumer
            .shared
            .buffer_epoch
            .store(u64::MAX - 1, Ordering::Release);

        assert_eq!(consumer.begin_buffer().buffer_epoch, u64::MAX);
        assert_eq!(consumer.current_buffer_epoch(), u64::MAX);
        assert_eq!(consumer.begin_buffer().buffer_epoch, u64::MAX);
        assert_eq!(consumer.current_buffer_epoch(), u64::MAX);
    }

    #[test]
    fn accepted_tempo_applies_by_deadline() {
        let initial = Tempo::from_bpm_integer(120);
        let next = Tempo::from_bpm_integer(240);
        let (producer, mut consumer) = spsc(4, initial);
        let mut playhead = playhead(
            initial,
            TransportPolicy::Scripted {
                schedule: VecDeque::new(),
            },
        );

        let outcome = producer.admit_tempo(
            next,
            AdmissionMetadata::tempo(CommandId(1), SourceId::default(), 0),
        );
        assert_eq!(outcome.status, AdmissionStatus::Accepted);

        let report = apply_control_to_playhead(&mut consumer, &mut playhead);

        assert!(report.tempo_updated);
        assert_eq!(report.params.tempo, next);
        assert_eq!(playhead.bpm, next);
        match playhead.phase_source {
            PhaseSource::Internal { bpm } => assert_eq!(bpm, next),
            _ => panic!("test playhead uses internal source"),
        }
    }

    #[test]
    fn accepted_stop_applies_by_deadline() {
        let bpm = Tempo::from_bpm_integer(120);
        let (producer, mut consumer) = spsc(4, bpm);
        let mut playhead = playhead(
            bpm,
            TransportPolicy::Internal {
                start_emitted: true,
            },
        );

        let outcome = producer.admit_ordered(ControlCommand::Stop, metadata(1, 1));
        assert_eq!(outcome.status, AdmissionStatus::Accepted);

        let report = apply_control_to_playhead(&mut consumer, &mut playhead);
        assert_eq!(report.applied_commands, 1);
        assert!(!playhead.stop_handle().is_stop_requested());
    }

    #[test]
    fn accepted_start_applies_by_deadline() {
        let bpm = Tempo::from_bpm_integer(120);
        let (producer, mut consumer) = spsc(4, bpm);
        let mut playhead = playhead(
            bpm,
            TransportPolicy::Internal {
                start_emitted: true,
            },
        );
        let stop = producer.admit_ordered(ControlCommand::Stop, metadata(1, 1));
        assert_eq!(stop.status, AdmissionStatus::Accepted);
        apply_control_to_playhead(&mut consumer, &mut playhead);
        let sink = TestSink::new();
        let input = [];
        let mut output = [];
        let mut io = AudioIo::new(&input, &mut output, 0, 48_000, 24_000);
        playhead.on_buffer(&mut io, &sink);
        assert!(!playhead.is_running());
        assert!(!playhead.stop_handle().is_stop_requested());

        let start = producer.admit_ordered(
            ControlCommand::Start,
            AdmissionMetadata::next_buffer(CommandId(2), SourceId::default(), 1),
        );
        assert_eq!(start.status, AdmissionStatus::Accepted);
        let report = apply_control_to_playhead(&mut consumer, &mut playhead);
        assert_eq!(report.applied_commands, 1);
        assert!(!playhead.stop_handle().is_stop_requested());
        let mut io = AudioIo::new(&input, &mut output, 24_000, 48_000, 24_000);
        playhead.on_buffer(&mut io, &sink);
        assert!(playhead.is_running());
    }

    #[test]
    fn future_deadline_waits_until_declared_buffer() {
        let bpm = Tempo::from_bpm_integer(120);
        let (producer, mut consumer) = spsc(4, bpm);
        let mut playhead = playhead(
            bpm,
            TransportPolicy::Internal {
                start_emitted: true,
            },
        );

        let outcome = producer.admit_ordered(ControlCommand::Start, metadata(1, 3));
        assert_eq!(outcome.status, AdmissionStatus::Accepted);

        let report = apply_control_to_playhead(&mut consumer, &mut playhead);
        assert_eq!(report.params.buffer_epoch, 1);
        assert_eq!(report.applied_commands, 0);

        let report = apply_control_to_playhead(&mut consumer, &mut playhead);
        assert_eq!(report.params.buffer_epoch, 2);
        assert_eq!(report.applied_commands, 0);

        let report = apply_control_to_playhead(&mut consumer, &mut playhead);
        assert_eq!(report.params.buffer_epoch, 3);
        assert_eq!(report.applied_commands, 1);
    }

    #[test]
    fn ordered_transport_commands_emit_fifo_in_one_buffer() {
        let bpm = Tempo::from_bpm_integer(120);
        let (producer, mut consumer) = spsc(4, bpm);
        let mut playhead = playhead(
            bpm,
            TransportPolicy::Internal {
                start_emitted: true,
            },
        );
        let sink = TestSink::new();
        let input = [];
        let mut output = [];

        let start = producer.admit_ordered(ControlCommand::Start, metadata(1, 1));
        let stop = producer.admit_ordered(ControlCommand::Stop, metadata(2, 1));
        assert_eq!(start.status, AdmissionStatus::Accepted);
        assert_eq!(stop.status, AdmissionStatus::Accepted);

        let report = apply_control_to_playhead(&mut consumer, &mut playhead);
        assert_eq!(report.applied_commands, 2);
        let mut io = AudioIo::new(&input, &mut output, 0, 48_000, 24_000);
        playhead.on_buffer(&mut io, &sink);

        let transport_records: Vec<u8> = sink
            .records()
            .into_iter()
            .filter_map(|r| r.bytes.first().copied())
            .filter(|b| *b == MIDI_START || *b == MIDI_STOP)
            .collect();
        assert_eq!(transport_records, vec![MIDI_START, MIDI_STOP]);
        assert!(!playhead.is_running());
    }

    #[test]
    fn command_transport_rejects_link_driven_policy() {
        let bpm = Tempo::from_bpm_integer(120);
        let (producer, mut consumer) = spsc(4, bpm);
        let mut playhead = playhead(
            bpm,
            TransportPolicy::LinkDriven {
                prev_playing: false,
                query: Box::new(|| false),
            },
        );

        let start = producer.admit_ordered(ControlCommand::Start, metadata(1, 1));
        let stop = producer.admit_ordered(ControlCommand::Stop, metadata(2, 1));
        assert_eq!(start.status, AdmissionStatus::Accepted);
        assert_eq!(stop.status, AdmissionStatus::Accepted);

        let report = apply_control_to_playhead(&mut consumer, &mut playhead);

        assert_eq!(report.applied_commands, 0);
        assert_eq!(report.unsupported_commands, 2);
        assert!(playhead.is_running());
    }

    #[test]
    fn transport_queue_full_reports_separately() {
        let bpm = Tempo::from_bpm_integer(120);
        let (producer, mut consumer) = spsc(129, bpm);
        let mut playhead = playhead(
            bpm,
            TransportPolicy::Internal {
                start_emitted: true,
            },
        );

        for id in 1..=129 {
            assert_eq!(
                producer
                    .admit_ordered(ControlCommand::Start, metadata(id, 1))
                    .status,
                AdmissionStatus::Accepted
            );
        }

        let report = apply_control_to_playhead(&mut consumer, &mut playhead);

        assert_eq!(report.applied_commands, 128);
        assert_eq!(report.transport_queue_full, 1);
        assert_eq!(report.unsupported_commands, 0);
        assert_eq!(report.teardown_rejected_commands, 0);
    }

    #[test]
    fn teardown_requested_reports_without_staging_command() {
        let bpm = Tempo::from_bpm_integer(120);
        let (producer, mut consumer) = spsc(4, bpm);
        let mut playhead = playhead(
            bpm,
            TransportPolicy::Internal {
                start_emitted: true,
            },
        );
        playhead.stop_handle().request_stop();

        let start = producer.admit_ordered(ControlCommand::Start, metadata(1, 1));
        let stop = producer.admit_ordered(ControlCommand::Stop, metadata(2, 1));
        assert_eq!(start.status, AdmissionStatus::Accepted);
        assert_eq!(stop.status, AdmissionStatus::Accepted);

        let report = apply_control_to_playhead(&mut consumer, &mut playhead);

        assert_eq!(report.applied_commands, 0);
        assert_eq!(report.teardown_rejected_commands, 2);
        assert_eq!(report.transport_queue_full, 0);
        assert_eq!(report.unsupported_commands, 0);
    }

    #[test]
    fn rt_command_application_no_realloc() {
        let bpm = Tempo::from_bpm_integer(120);
        let (producer, mut consumer) = spsc(4, bpm);
        let mut playhead = playhead(
            bpm,
            TransportPolicy::Internal {
                start_emitted: true,
            },
        );
        let cap_before = playhead.max_events_per_buffer();

        assert_eq!(
            producer
                .admit_ordered(ControlCommand::Start, metadata(1, 1))
                .status,
            AdmissionStatus::Accepted
        );
        assert_eq!(
            producer
                .admit_ordered(ControlCommand::Stop, metadata(2, 1))
                .status,
            AdmissionStatus::Accepted
        );
        assert_eq!(
            producer
                .admit_ordered(ControlCommand::Start, metadata(3, 1))
                .status,
            AdmissionStatus::Accepted
        );
        assert_eq!(
            producer
                .admit_ordered(ControlCommand::Stop, metadata(4, 1))
                .status,
            AdmissionStatus::Accepted
        );

        let report = apply_control_to_playhead(&mut consumer, &mut playhead);

        assert_eq!(report.applied_commands, 4);
        assert_eq!(report.transport_queue_full, 0);
        assert_eq!(report.teardown_rejected_commands, 0);
        assert_eq!(playhead.max_events_per_buffer(), cap_before);
    }

    #[test]
    fn late_ordered_command_reports_fault() {
        let bpm = Tempo::from_bpm_integer(120);
        let (producer, mut consumer) = spsc(4, bpm);
        let mut playhead = playhead(
            bpm,
            TransportPolicy::Scripted {
                schedule: VecDeque::new(),
            },
        );

        let outcome = producer.admit_ordered(ControlCommand::Start, metadata(1, 1));
        assert_eq!(outcome.status, AdmissionStatus::Accepted);
        consumer.begin_buffer();

        let report = apply_control_to_playhead(&mut consumer, &mut playhead);

        assert_eq!(report.missed_deadlines, 1);
        assert_eq!(report.applied_commands, 0);
    }

    #[test]
    fn unsupported_ordered_command_reports_fault() {
        let bpm = Tempo::from_bpm_integer(120);
        let (producer, mut consumer) = spsc(4, bpm);
        let mut playhead = playhead(
            bpm,
            TransportPolicy::Scripted {
                schedule: VecDeque::new(),
            },
        );

        let outcome = producer.admit_ordered(ControlCommand::Locate { tick: 960 }, metadata(1, 1));
        assert_eq!(outcome.status, AdmissionStatus::Accepted);

        let report = apply_control_to_playhead(&mut consumer, &mut playhead);

        assert_eq!(report.unsupported_commands, 1);
        assert_eq!(report.applied_commands, 0);
    }

    proptest! {
        #[test]
        fn command_admission_is_total(
            id in any::<u64>(),
            deadline in any::<u64>(),
            domain in domain_strategy(),
            has_coalesce_key in any::<bool>(),
        ) {
            let (producer, _consumer) = spsc(1, Tempo::from_bpm_integer(120));
            let mut meta = metadata(id, deadline);
            meta.deadline.time_domain = domain;
            if has_coalesce_key {
                meta.coalesce_key = Some(CoalesceKey::new("transport").expect("key fits"));
            }

            let outcome = producer.admit_ordered(ControlCommand::Start, meta);

            prop_assert!(matches!(
                outcome.status,
                AdmissionStatus::Accepted | AdmissionStatus::Rejected | AdmissionStatus::Late
            ));
            prop_assert_eq!(outcome.reason.is_none(), outcome.status == AdmissionStatus::Accepted);
        }

        #[test]
        fn accepted_command_has_declared_deadline(
            id in any::<u64>(),
            deadline in any::<u64>(),
        ) {
            prop_assume!(deadline > 0);
            let (producer, _consumer) = spsc(4, Tempo::from_bpm_integer(120));
            let meta = metadata(id, deadline);
            let outcome = producer.admit_ordered(ControlCommand::Start, meta);

            prop_assert_eq!(outcome.status, AdmissionStatus::Accepted);
            prop_assert_eq!(outcome.metadata.command_id, CommandId(id));
            prop_assert_eq!(outcome.metadata.source_id, SourceId::default());
            prop_assert_eq!(outcome.metadata.deadline, CommandDeadline::rt_buffer(deadline));
        }

        // This property advances the simulated callback once per generated
        // buffer, so it intentionally samples a small latency window. The
        // u64 deadline boundary is covered by
        // `max_deadline_ordered_admission_preserves_boundary`.
        #[test]
        fn accepted_command_applies_by_deadline(deadline in 1_u64..8) {
            let (producer, mut consumer) = spsc(4, Tempo::from_bpm_integer(120));
            let outcome = producer.admit_ordered(ControlCommand::Start, metadata(1, deadline));
            prop_assert_eq!(outcome.status, AdmissionStatus::Accepted);

            for _ in 0..deadline {
                consumer.begin_buffer();
            }

            let drained = consumer.drain_due_command();
            let applied = match drained {
                Some(RtCommandDrain::Command(envelope)) => {
                    envelope.command == ControlCommand::Start
                }
                _ => false,
            };
            prop_assert!(applied);
        }

        #[test]
        fn tempo_slot_state_encoding_is_disjoint(generation in tempo_generation_strategy()) {
            let pending = generation;
            let writing = writing_tempo_state(generation);
            let claimed = claimed_tempo_state(generation);

            prop_assert_eq!(tempo_slot_state(NO_PENDING_TEMPO), TempoSlotState::Empty);
            prop_assert_eq!(tempo_slot_state(pending), TempoSlotState::Pending);
            prop_assert_eq!(tempo_slot_state(writing), TempoSlotState::Writing);
            prop_assert_eq!(tempo_slot_state(claimed), TempoSlotState::Claimed);
            prop_assert_eq!(tempo_state_generation(pending), generation);
            prop_assert_eq!(tempo_state_generation(writing), generation);
            prop_assert_eq!(tempo_state_generation(claimed), generation);
            prop_assert_ne!(pending, writing);
            prop_assert_ne!(pending, claimed);
            prop_assert_ne!(writing, claimed);
        }

        #[test]
        fn tempo_slot_fsm_is_deterministic(
            events in proptest::collection::vec(tempo_slot_event_strategy(), 0..64),
        ) {
            let mut left = TempoSlotFsm::new();
            let mut right = TempoSlotFsm::new();
            for event in events {
                let left_result = left.consume(&event);
                let right_result = right.consume(&event);
                prop_assert_eq!(left_result.is_ok(), right_result.is_ok());
                prop_assert_eq!(left.state(), right.state());
            }
        }

        #[test]
        fn tempo_replacement_in_progress_preserves_prior_pending_property(
            first_raw in any::<u32>(),
            replacement_raw in any::<u32>(),
        ) {
            prop_assume!(first_raw != replacement_raw);
            let (producer, consumer) = spsc(4, Tempo::from_bpm_integer(120));
            let first_tempo = Tempo(first_raw);
            let replacement_tempo = Tempo(replacement_raw);
            let first = producer.admit_tempo(
                first_tempo,
                AdmissionMetadata::tempo(CommandId(1), SourceId::default(), 0),
            );
            prop_assert_eq!(first.status, AdmissionStatus::Accepted);

            producer
                .shared
                .staged_tempo_raw
                .store(replacement_tempo.0, Ordering::Release);
            producer
                .shared
                .staged_tempo_deadline
                .store(1, Ordering::Release);
            let write = producer.begin_tempo_write();

            prop_assert_eq!(consumer.begin_buffer().tempo, first_tempo);
            producer.abort_tempo_write(write);
            prop_assert_eq!(producer.tempo(), first_tempo);
        }
    }
}
