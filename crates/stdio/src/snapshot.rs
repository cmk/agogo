//! Decimated agogo observation snapshots.
//!
//! The audio callback writes compact fixed-point state through
//! [`RtSnapshotWriter`]. The async side reads typed [`AgogoSnapshot`]
//! values and serializes or dispatches them off the RT thread.

use std::array;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU8, AtomicU32, AtomicU64, Ordering, fence};

use agogo_core::conn::tempo::Tempo;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

pub const SNAPSHOT_SCHEMA: &str = "agogo.snapshot.v1";
pub const AGOGO_STATE_FORM_TYPE: &str = "agogo-state";
pub const AGOGO_MAIN_ID: &str = "agogo.main";
pub const MAX_SNAPSHOT_CHANNELS: usize = 16;

const SCALE_MICRO: u32 = 1_000_000;

/// Full observation state shared by the standalone TUI and stdio adapter.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AgogoSnapshot {
    pub schema: String,
    pub seq: u64,
    pub bpm: DecimalU32<SCALE_MICRO>,
    pub transport: TransportSnapshot,
    pub sync: SyncSnapshot,
    pub audio: AudioSnapshot,
    pub channels: Vec<ChannelSnapshot>,
}

impl AgogoSnapshot {
    pub fn new(seq: u64, bpm: Tempo) -> Self {
        Self {
            schema: SNAPSHOT_SCHEMA.to_owned(),
            seq,
            bpm: DecimalU32(bpm.0),
            transport: TransportSnapshot::default(),
            sync: SyncSnapshot::default(),
            audio: AudioSnapshot::default(),
            channels: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportState {
    Stopped,
    Running,
    Paused,
    Locating,
}

impl Default for TransportState {
    fn default() -> Self {
        Self::Stopped
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct TransportSnapshot {
    pub state: TransportState,
    pub bar: u32,
    pub beat: u32,
    pub tick: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncSource {
    Internal,
    Link,
    MidiClock,
    AudioPulse,
}

impl Default for SyncSource {
    fn default() -> Self {
        Self::Internal
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct SyncSnapshot {
    pub source: SyncSource,
    pub pll_locked: bool,
    pub error_ticks: DecimalI32<SCALE_MICRO>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct AudioSnapshot {
    pub sample_rate: u32,
    pub buffer_size: u32,
    pub load: DecimalU32<SCALE_MICRO>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelOutput {
    Midi,
    Cv,
    Mtc,
    Disabled,
}

impl Default for ChannelOutput {
    fn default() -> Self {
        Self::Disabled
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChannelSnapshot {
    pub index: u32,
    pub enabled: bool,
    pub grid: String,
    pub phase: DecimalU32<SCALE_MICRO>,
    pub output: ChannelOutput,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Default)]
pub struct DecimalU32<const SCALE: u32>(pub u32);

#[derive(Copy, Clone, Debug, Eq, PartialEq, Default)]
pub struct DecimalI32<const SCALE: u32>(pub i32);

impl<const SCALE: u32> Serialize for DecimalU32<SCALE> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serialize_decimal_u32::<SCALE, S>(self.0, serializer)
    }
}

impl<'de, const SCALE: u32> Deserialize<'de> for DecimalU32<SCALE> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        parse_decimal_u32::<SCALE>(&value)
            .map(Self)
            .map_err(serde::de::Error::custom)
    }
}

impl<const SCALE: u32> Serialize for DecimalI32<SCALE> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serialize_decimal_i32::<SCALE, S>(self.0, serializer)
    }
}

impl<'de, const SCALE: u32> Deserialize<'de> for DecimalI32<SCALE> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        parse_decimal_i32::<SCALE>(&value)
            .map(Self)
            .map_err(serde::de::Error::custom)
    }
}

/// Fixed-capacity RT-to-observer snapshot handoff.
#[derive(Clone, Debug)]
pub struct SnapshotSlot {
    inner: Arc<SnapshotAtomics>,
}

impl SnapshotSlot {
    pub fn new(initial_bpm: Tempo) -> Self {
        Self {
            inner: Arc::new(SnapshotAtomics::new(initial_bpm)),
        }
    }

    pub fn writer(&self) -> RtSnapshotWriter {
        RtSnapshotWriter {
            inner: Arc::clone(&self.inner),
        }
    }

    pub fn reader(&self) -> SnapshotReader {
        SnapshotReader {
            inner: Arc::clone(&self.inner),
        }
    }
}

#[derive(Clone, Debug)]
pub struct RtSnapshotWriter {
    inner: Arc<SnapshotAtomics>,
}

impl RtSnapshotWriter {
    pub fn write(&self, frame: &RtSnapshotFrame<'_>) -> u64 {
        let epoch = self.inner.write_epoch.load(Ordering::SeqCst);
        let begin_epoch = if epoch % 2 == 0 { epoch + 1 } else { epoch + 2 };
        self.inner.write_epoch.store(begin_epoch, Ordering::SeqCst);
        // Single-writer seqlock: the odd epoch must become visible
        // before any payload store can be observed, and the final even
        // epoch must not publish until every payload store is visible.
        fence(Ordering::SeqCst);

        let seq = self.inner.seq.load(Ordering::Relaxed).saturating_add(1);
        self.inner.bpm_raw.store(frame.bpm.0, Ordering::Release);
        self.inner
            .transport_state
            .store(frame.transport.state.to_u8(), Ordering::Release);
        self.inner
            .transport_bar
            .store(frame.transport.bar, Ordering::Release);
        self.inner
            .transport_beat
            .store(frame.transport.beat, Ordering::Release);
        self.inner
            .transport_tick
            .store(frame.transport.tick, Ordering::Release);
        self.inner
            .sync_source
            .store(frame.sync.source.to_u8(), Ordering::Release);
        self.inner
            .pll_locked
            .store(frame.sync.pll_locked, Ordering::Release);
        self.inner
            .error_ticks_raw
            .store(frame.sync.error_ticks_raw, Ordering::Release);
        self.inner
            .sample_rate
            .store(frame.audio.sample_rate, Ordering::Release);
        self.inner
            .buffer_size
            .store(frame.audio.buffer_size, Ordering::Release);
        self.inner
            .audio_load_raw
            .store(frame.audio.load_raw, Ordering::Release);

        let count = frame.channels.len().min(MAX_SNAPSHOT_CHANNELS);
        for (slot, channel) in self
            .inner
            .channels
            .iter()
            .zip(frame.channels.iter().take(count))
        {
            slot.index.store(channel.index, Ordering::Release);
            slot.enabled.store(channel.enabled, Ordering::Release);
            slot.grid.store(channel.grid.to_u8(), Ordering::Release);
            slot.phase_raw.store(channel.phase_raw, Ordering::Release);
            slot.output.store(channel.output.to_u8(), Ordering::Release);
        }
        self.inner
            .channel_count
            .store(count as u32, Ordering::Release);
        fence(Ordering::SeqCst);
        self.inner.seq.store(seq, Ordering::Release);
        self.inner
            .write_epoch
            .store(begin_epoch + 1, Ordering::SeqCst);
        seq
    }
}

#[derive(Clone, Debug)]
pub struct SnapshotReader {
    inner: Arc<SnapshotAtomics>,
}

impl SnapshotReader {
    pub fn snapshot(&self) -> AgogoSnapshot {
        loop {
            let begin_epoch = self.inner.write_epoch.load(Ordering::SeqCst);
            if begin_epoch % 2 != 0 {
                std::hint::spin_loop();
                continue;
            }
            // Pair with the writer fences so `begin_epoch == end_epoch`
            // means this reader saw one coherent payload generation.
            fence(Ordering::SeqCst);
            let snapshot = self.snapshot_once();
            fence(Ordering::SeqCst);
            let end_epoch = self.inner.write_epoch.load(Ordering::SeqCst);
            if begin_epoch == end_epoch {
                return snapshot;
            }
            std::hint::spin_loop();
        }
    }

    fn snapshot_once(&self) -> AgogoSnapshot {
        let count = self
            .inner
            .channel_count
            .load(Ordering::Acquire)
            .min(MAX_SNAPSHOT_CHANNELS as u32) as usize;
        let mut channels = Vec::with_capacity(count);
        for slot in self.inner.channels.iter().take(count) {
            channels.push(ChannelSnapshot {
                index: slot.index.load(Ordering::Acquire),
                enabled: slot.enabled.load(Ordering::Acquire),
                grid: grid_from_u8(slot.grid.load(Ordering::Acquire)).to_owned(),
                phase: DecimalU32(slot.phase_raw.load(Ordering::Acquire)),
                output: ChannelOutputCode::from_u8(slot.output.load(Ordering::Acquire)).into(),
            });
        }

        AgogoSnapshot {
            schema: SNAPSHOT_SCHEMA.to_owned(),
            seq: self.inner.seq.load(Ordering::Acquire),
            bpm: DecimalU32(self.inner.bpm_raw.load(Ordering::Acquire)),
            transport: TransportSnapshot {
                state: TransportStateCode::from_u8(
                    self.inner.transport_state.load(Ordering::Acquire),
                )
                .into(),
                bar: self.inner.transport_bar.load(Ordering::Acquire),
                beat: self.inner.transport_beat.load(Ordering::Acquire),
                tick: self.inner.transport_tick.load(Ordering::Acquire),
            },
            sync: SyncSnapshot {
                source: SyncSourceCode::from_u8(self.inner.sync_source.load(Ordering::Acquire))
                    .into(),
                pll_locked: self.inner.pll_locked.load(Ordering::Acquire),
                error_ticks: DecimalI32(self.inner.error_ticks_raw.load(Ordering::Acquire)),
            },
            audio: AudioSnapshot {
                sample_rate: self.inner.sample_rate.load(Ordering::Acquire),
                buffer_size: self.inner.buffer_size.load(Ordering::Acquire),
                load: DecimalU32(self.inner.audio_load_raw.load(Ordering::Acquire)),
            },
            channels,
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Default)]
pub struct RtTransportFrame {
    pub state: TransportStateCode,
    pub bar: u32,
    pub beat: u32,
    pub tick: u32,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Default)]
pub struct RtSyncFrame {
    pub source: SyncSourceCode,
    pub pll_locked: bool,
    pub error_ticks_raw: i32,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Default)]
pub struct RtAudioFrame {
    pub sample_rate: u32,
    pub buffer_size: u32,
    pub load_raw: u32,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Default)]
pub struct RtChannelFrame {
    pub index: u32,
    pub enabled: bool,
    pub grid: GridCode,
    pub phase_raw: u32,
    pub output: ChannelOutputCode,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct RtSnapshotFrame<'a> {
    pub bpm: Tempo,
    pub transport: RtTransportFrame,
    pub sync: RtSyncFrame,
    pub audio: RtAudioFrame,
    pub channels: &'a [RtChannelFrame],
}

#[derive(Debug)]
struct SnapshotAtomics {
    write_epoch: AtomicU64,
    seq: AtomicU64,
    bpm_raw: AtomicU32,
    transport_state: AtomicU8,
    transport_bar: AtomicU32,
    transport_beat: AtomicU32,
    transport_tick: AtomicU32,
    sync_source: AtomicU8,
    pll_locked: AtomicBool,
    error_ticks_raw: AtomicI32,
    sample_rate: AtomicU32,
    buffer_size: AtomicU32,
    audio_load_raw: AtomicU32,
    channel_count: AtomicU32,
    channels: [ChannelAtomics; MAX_SNAPSHOT_CHANNELS],
}

impl SnapshotAtomics {
    fn new(initial_bpm: Tempo) -> Self {
        Self {
            write_epoch: AtomicU64::new(0),
            seq: AtomicU64::new(0),
            bpm_raw: AtomicU32::new(initial_bpm.0),
            transport_state: AtomicU8::new(TransportStateCode::Stopped.to_u8()),
            transport_bar: AtomicU32::new(0),
            transport_beat: AtomicU32::new(0),
            transport_tick: AtomicU32::new(0),
            sync_source: AtomicU8::new(SyncSourceCode::Internal.to_u8()),
            pll_locked: AtomicBool::new(false),
            error_ticks_raw: AtomicI32::new(0),
            sample_rate: AtomicU32::new(0),
            buffer_size: AtomicU32::new(0),
            audio_load_raw: AtomicU32::new(0),
            channel_count: AtomicU32::new(0),
            channels: array::from_fn(|_| ChannelAtomics::default()),
        }
    }
}

#[derive(Debug, Default)]
struct ChannelAtomics {
    index: AtomicU32,
    enabled: AtomicBool,
    grid: AtomicU8,
    phase_raw: AtomicU32,
    output: AtomicU8,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Default)]
pub enum TransportStateCode {
    #[default]
    Stopped,
    Running,
    Paused,
    Locating,
}

impl TransportStateCode {
    const fn to_u8(self) -> u8 {
        match self {
            Self::Stopped => 0,
            Self::Running => 1,
            Self::Paused => 2,
            Self::Locating => 3,
        }
    }

    const fn from_u8(value: u8) -> Self {
        match value {
            1 => Self::Running,
            2 => Self::Paused,
            3 => Self::Locating,
            _ => Self::Stopped,
        }
    }
}

impl From<TransportStateCode> for TransportState {
    fn from(value: TransportStateCode) -> Self {
        match value {
            TransportStateCode::Stopped => Self::Stopped,
            TransportStateCode::Running => Self::Running,
            TransportStateCode::Paused => Self::Paused,
            TransportStateCode::Locating => Self::Locating,
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Default)]
pub enum SyncSourceCode {
    #[default]
    Internal,
    Link,
    MidiClock,
    AudioPulse,
}

impl SyncSourceCode {
    const fn to_u8(self) -> u8 {
        match self {
            Self::Internal => 0,
            Self::Link => 1,
            Self::MidiClock => 2,
            Self::AudioPulse => 3,
        }
    }

    const fn from_u8(value: u8) -> Self {
        match value {
            1 => Self::Link,
            2 => Self::MidiClock,
            3 => Self::AudioPulse,
            _ => Self::Internal,
        }
    }
}

impl From<SyncSourceCode> for SyncSource {
    fn from(value: SyncSourceCode) -> Self {
        match value {
            SyncSourceCode::Internal => Self::Internal,
            SyncSourceCode::Link => Self::Link,
            SyncSourceCode::MidiClock => Self::MidiClock,
            SyncSourceCode::AudioPulse => Self::AudioPulse,
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Default)]
pub enum ChannelOutputCode {
    Midi,
    Cv,
    Mtc,
    #[default]
    Disabled,
}

impl ChannelOutputCode {
    const fn to_u8(self) -> u8 {
        match self {
            Self::Midi => 0,
            Self::Cv => 1,
            Self::Mtc => 2,
            Self::Disabled => 3,
        }
    }

    const fn from_u8(value: u8) -> Self {
        match value {
            0 => Self::Midi,
            1 => Self::Cv,
            2 => Self::Mtc,
            _ => Self::Disabled,
        }
    }
}

impl From<ChannelOutputCode> for ChannelOutput {
    fn from(value: ChannelOutputCode) -> Self {
        match value {
            ChannelOutputCode::Midi => Self::Midi,
            ChannelOutputCode::Cv => Self::Cv,
            ChannelOutputCode::Mtc => Self::Mtc,
            ChannelOutputCode::Disabled => Self::Disabled,
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Default)]
pub enum GridCode {
    T4,
    T8,
    #[default]
    T16,
    T24,
    T32,
}

fn grid_from_u8(value: u8) -> &'static str {
    match value {
        0 => "T4",
        1 => "T8",
        3 => "T24",
        4 => "T32",
        _ => "T16",
    }
}

impl GridCode {
    const fn to_u8(self) -> u8 {
        match self {
            Self::T4 => 0,
            Self::T8 => 1,
            Self::T16 => 2,
            Self::T24 => 3,
            Self::T32 => 4,
        }
    }
}

pub mod push {
    use super::*;

    #[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
    #[serde(transparent)]
    pub struct StreamId(pub String);

    #[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "kebab-case")]
    pub enum FormType {
        Text,
        Markdown,
        Meter,
        #[serde(untagged)]
        Other(String),
    }

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    pub struct ObservationParams {
        pub stream_id: StreamId,
        pub seq: u64,
        #[serde(flatten)]
        pub op: ObservationOp,
    }

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    #[serde(tag = "op", rename_all = "snake_case")]
    pub enum ObservationOp {
        Create {
            form_id: String,
            form_type: FormType,
            data: Value,
        },
        Patch {
            form_id: String,
            patch: Value,
        },
        Destroy {
            form_id: String,
        },
    }

    pub trait ObservationSink {
        fn dispatch(&mut self, params: ObservationParams);
    }

    #[derive(Clone, Debug)]
    pub struct SnapshotPublisher {
        reader: SnapshotReader,
        stream_id: StreamId,
        form_id: String,
        created: bool,
        last_seq: u64,
    }

    impl SnapshotPublisher {
        pub fn new(reader: SnapshotReader) -> Self {
            Self {
                reader,
                stream_id: StreamId(AGOGO_MAIN_ID.to_owned()),
                form_id: AGOGO_MAIN_ID.to_owned(),
                created: false,
                last_seq: 0,
            }
        }

        pub fn publish_next<S: ObservationSink>(&mut self, sink: &mut S) -> Result<bool, String> {
            let snapshot = self.reader.snapshot();
            if self.created && snapshot.seq == self.last_seq {
                return Ok(false);
            }
            let payload = serde_json::to_value(&snapshot).map_err(|err| err.to_string())?;
            let params = if self.created {
                ObservationParams {
                    stream_id: self.stream_id.clone(),
                    seq: snapshot.seq,
                    op: ObservationOp::Patch {
                        form_id: self.form_id.clone(),
                        patch: payload,
                    },
                }
            } else {
                ObservationParams {
                    stream_id: self.stream_id.clone(),
                    seq: snapshot.seq,
                    op: ObservationOp::Create {
                        form_id: self.form_id.clone(),
                        form_type: FormType::Other(AGOGO_STATE_FORM_TYPE.to_owned()),
                        data: payload,
                    },
                }
            };
            self.created = true;
            self.last_seq = snapshot.seq;
            sink.dispatch(params);
            Ok(true)
        }

        pub fn publish_destroy<S: ObservationSink>(&mut self, sink: &mut S) {
            sink.dispatch(ObservationParams {
                stream_id: self.stream_id.clone(),
                seq: self.last_seq.saturating_add(1),
                op: ObservationOp::Destroy {
                    form_id: self.form_id.clone(),
                },
            });
        }
    }
}

fn serialize_decimal_u32<const SCALE: u32, S>(raw: u32, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    decimal_number(raw as u64, false, SCALE)
        .map_err(serde::ser::Error::custom)?
        .serialize(serializer)
}

fn serialize_decimal_i32<const SCALE: u32, S>(raw: i32, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    decimal_number(raw.unsigned_abs() as u64, raw.is_negative(), SCALE)
        .map_err(serde::ser::Error::custom)?
        .serialize(serializer)
}

fn decimal_number(raw_abs: u64, negative: bool, scale: u32) -> Result<serde_json::Number, String> {
    let mut text = decimal_text(raw_abs, scale);
    if negative && raw_abs != 0 {
        text.insert(0, '-');
    }
    serde_json::from_str(&text).map_err(|err| err.to_string())
}

fn decimal_text(raw_abs: u64, scale: u32) -> String {
    let whole = raw_abs / u64::from(scale);
    let frac = raw_abs % u64::from(scale);
    if frac == 0 {
        return whole.to_string();
    }
    let width = scale.ilog10() as usize;
    let mut frac_text = format!("{frac:0width$}");
    while frac_text.ends_with('0') {
        frac_text.pop();
    }
    format!("{whole}.{frac_text}")
}

fn parse_decimal_u32<const SCALE: u32>(value: &Value) -> Result<u32, String> {
    let raw = parse_scaled_decimal(value, SCALE)?;
    u32::try_from(raw).map_err(|_| "decimal exceeds u32 range".to_owned())
}

fn parse_decimal_i32<const SCALE: u32>(value: &Value) -> Result<i32, String> {
    let raw = parse_scaled_decimal(value, SCALE)?;
    i32::try_from(raw).map_err(|_| "decimal exceeds i32 range".to_owned())
}

fn parse_scaled_decimal(value: &Value, scale: u32) -> Result<i64, String> {
    let Value::Number(number) = value else {
        return Err("expected JSON number".to_owned());
    };
    let text = number.to_string();
    let (negative, body) = text
        .strip_prefix('-')
        .map_or((false, text.as_str()), |rest| (true, rest));
    let (whole_text, frac_text) = body.split_once('.').unwrap_or((body, ""));
    let whole = whole_text
        .parse::<i64>()
        .map_err(|_| "invalid decimal whole part".to_owned())?;
    let mut frac_digits = frac_text.to_owned();
    let width = scale.ilog10() as usize;
    if frac_digits.len() > width {
        return Err("decimal precision exceeds scale".to_owned());
    }
    while frac_digits.len() < width {
        frac_digits.push('0');
    }
    let frac = if frac_digits.is_empty() {
        0
    } else {
        frac_digits
            .parse::<i64>()
            .map_err(|_| "invalid decimal fractional part".to_owned())?
    };
    let raw_abs = whole
        .checked_mul(i64::from(scale))
        .and_then(|v| v.checked_add(frac))
        .ok_or_else(|| "decimal exceeds supported range".to_owned())?;
    Ok(if negative { -raw_abs } else { raw_abs })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use proptest::prelude::*;
    use serde_json::json;

    use super::push::{FormType, ObservationOp, ObservationParams, ObservationSink};
    use super::*;

    fn arb_channels() -> impl Strategy<Value = Vec<ChannelSnapshot>> {
        prop::collection::vec(
            (
                0_u32..64,
                any::<bool>(),
                prop_oneof![
                    Just("T4"),
                    Just("T8"),
                    Just("T16"),
                    Just("T24"),
                    Just("T32")
                ],
                any::<u32>(),
                prop_oneof![
                    Just(ChannelOutput::Midi),
                    Just(ChannelOutput::Cv),
                    Just(ChannelOutput::Mtc),
                    Just(ChannelOutput::Disabled),
                ],
            ),
            0..=8,
        )
        .prop_map(|channels| {
            let mut seen = BTreeSet::new();
            channels
                .into_iter()
                .filter_map(|(index, enabled, grid, phase_raw, output)| {
                    if !seen.insert(index) {
                        return None;
                    }
                    Some(ChannelSnapshot {
                        index,
                        enabled,
                        grid: grid.to_owned(),
                        phase: DecimalU32(phase_raw),
                        output,
                    })
                })
                .collect::<Vec<_>>()
        })
    }

    fn arb_snapshot() -> impl Strategy<Value = AgogoSnapshot> {
        let transport = (
            prop_oneof![
                Just(TransportState::Stopped),
                Just(TransportState::Running),
                Just(TransportState::Paused),
                Just(TransportState::Locating),
            ],
            any::<u32>(),
            any::<u32>(),
            any::<u32>(),
        );
        let sync = (
            prop_oneof![
                Just(SyncSource::Internal),
                Just(SyncSource::Link),
                Just(SyncSource::MidiClock),
                Just(SyncSource::AudioPulse),
            ],
            any::<bool>(),
            -1_000_000_i32..=1_000_000,
        );
        let audio = (
            prop_oneof![Just(44_100_u32), Just(48_000), Just(96_000), Just(192_000)],
            1_u32..=4_096,
            0_u32..=SCALE_MICRO,
        );
        (
            any::<u64>(),
            1_u32..=Tempo::MAX_BPM_INTEGER,
            transport,
            sync,
            audio,
            arb_channels(),
        )
            .prop_map(
                |(
                    seq,
                    bpm,
                    (state, bar, beat, tick),
                    (source, pll_locked, error_ticks_raw),
                    (sample_rate, buffer_size, load),
                    channels,
                )| AgogoSnapshot {
                    schema: SNAPSHOT_SCHEMA.to_owned(),
                    seq,
                    bpm: DecimalU32(Tempo::from_bpm_integer(bpm).0),
                    transport: TransportSnapshot {
                        state,
                        bar,
                        beat,
                        tick,
                    },
                    sync: SyncSnapshot {
                        source,
                        pll_locked,
                        error_ticks: DecimalI32(error_ticks_raw),
                    },
                    audio: AudioSnapshot {
                        sample_rate,
                        buffer_size,
                        load: DecimalU32(load),
                    },
                    channels,
                },
            )
    }

    proptest! {
        #[test]
        fn snapshot_schema_round_trips(snapshot in arb_snapshot()) {
            let value = serde_json::to_value(&snapshot).expect("serialize snapshot");
            let roundtrip: AgogoSnapshot =
                serde_json::from_value(value).expect("deserialize snapshot");
            prop_assert_eq!(roundtrip, snapshot);
        }

        #[test]
        fn snapshot_channel_indices_unique(snapshot in arb_snapshot()) {
            let mut seen = BTreeSet::new();
            for channel in snapshot.channels {
                prop_assert!(seen.insert(channel.index));
            }
        }
    }

    #[test]
    fn rt_push_has_no_allocating_storage() {
        assert!(std::mem::size_of::<RtSnapshotFrame<'static>>() <= 128);
        assert!(std::mem::size_of::<RtChannelFrame>() <= 16);
    }

    #[test]
    fn rt_writer_reader_roundtrip_snapshot() {
        let slot = SnapshotSlot::new(Tempo::from_bpm_integer(120));
        let writer = slot.writer();
        let reader = slot.reader();
        let channels = [
            RtChannelFrame {
                index: 0,
                enabled: true,
                grid: GridCode::T4,
                phase_raw: 500_000,
                output: ChannelOutputCode::Midi,
            },
            RtChannelFrame {
                index: 2,
                enabled: true,
                grid: GridCode::T16,
                phase_raw: 250_000,
                output: ChannelOutputCode::Cv,
            },
        ];
        let seq = writer.write(&RtSnapshotFrame {
            bpm: Tempo::from_bpm_integer(123),
            transport: RtTransportFrame {
                state: TransportStateCode::Running,
                bar: 12,
                beat: 3,
                tick: 480,
            },
            sync: RtSyncFrame {
                source: SyncSourceCode::Internal,
                pll_locked: true,
                error_ticks_raw: 120_000,
            },
            audio: RtAudioFrame {
                sample_rate: 48_000,
                buffer_size: 128,
                load_raw: 310_000,
            },
            channels: &channels,
        });

        let snapshot = reader.snapshot();
        assert_eq!(seq, 1);
        assert_eq!(snapshot.seq, 1);
        assert_eq!(snapshot.transport.state, TransportState::Running);
        assert_eq!(snapshot.channels.len(), 2);
        assert_eq!(snapshot.channels[0].grid, "T4");
        assert_eq!(snapshot.channels[1].output, ChannelOutput::Cv);
    }

    #[derive(Default)]
    struct RecordingSink {
        items: Vec<ObservationParams>,
    }

    impl ObservationSink for RecordingSink {
        fn dispatch(&mut self, params: ObservationParams) {
            self.items.push(params);
        }
    }

    #[test]
    fn create_uses_agogo_state_form_type() {
        let slot = SnapshotSlot::new(Tempo::from_bpm_integer(120));
        slot.writer().write(&RtSnapshotFrame {
            bpm: Tempo::from_bpm_integer(120),
            transport: RtTransportFrame::default(),
            sync: RtSyncFrame::default(),
            audio: RtAudioFrame::default(),
            channels: &[],
        });
        let mut publisher = push::SnapshotPublisher::new(slot.reader());
        let mut sink = RecordingSink::default();
        assert!(publisher.publish_next(&mut sink).expect("publish"));

        let ObservationOp::Create {
            form_id,
            form_type,
            data,
        } = &sink.items[0].op
        else {
            panic!("expected create");
        };
        assert_eq!(sink.items[0].stream_id.0, AGOGO_MAIN_ID);
        assert_eq!(form_id, AGOGO_MAIN_ID);
        assert_eq!(
            form_type,
            &FormType::Other(AGOGO_STATE_FORM_TYPE.to_owned())
        );
        assert_eq!(data["schema"], json!(SNAPSHOT_SCHEMA));
    }

    #[test]
    fn patch_carries_full_snapshot_v1() {
        let slot = SnapshotSlot::new(Tempo::from_bpm_integer(120));
        let writer = slot.writer();
        let mut publisher = push::SnapshotPublisher::new(slot.reader());
        let mut sink = RecordingSink::default();
        for bpm in [120, 121] {
            writer.write(&RtSnapshotFrame {
                bpm: Tempo::from_bpm_integer(bpm),
                transport: RtTransportFrame::default(),
                sync: RtSyncFrame::default(),
                audio: RtAudioFrame::default(),
                channels: &[],
            });
            publisher.publish_next(&mut sink).expect("publish");
        }

        let ObservationOp::Patch { form_id, patch } = &sink.items[1].op else {
            panic!("expected patch");
        };
        assert_eq!(form_id, AGOGO_MAIN_ID);
        assert_eq!(patch["schema"], json!(SNAPSHOT_SCHEMA));
        assert!(patch.get("transport").is_some());
        assert!(patch.get("sync").is_some());
        assert!(patch.get("audio").is_some());
        assert!(patch.get("channels").is_some());
    }

    #[test]
    fn seq_monotonic_under_decimation() {
        let slot = SnapshotSlot::new(Tempo::from_bpm_integer(120));
        let writer = slot.writer();
        let mut publisher = push::SnapshotPublisher::new(slot.reader());
        let mut sink = RecordingSink::default();

        for bpm in [120, 121, 122, 123] {
            writer.write(&RtSnapshotFrame {
                bpm: Tempo::from_bpm_integer(bpm),
                transport: RtTransportFrame::default(),
                sync: RtSyncFrame::default(),
                audio: RtAudioFrame::default(),
                channels: &[],
            });
            publisher.publish_next(&mut sink).expect("publish");
        }

        assert!(sink.items.windows(2).all(|pair| pair[0].seq < pair[1].seq));
    }

    #[test]
    fn agogo_snapshot_drop_is_detectable() {
        let delivered = [1_u64, 2, 4, 7];
        let gaps = delivered
            .windows(2)
            .filter(|pair| pair[1] != pair[0] + 1)
            .count();
        assert_eq!(gaps, 2);
    }

    #[test]
    fn unmount_publishes_destroy() {
        let slot = SnapshotSlot::new(Tempo::from_bpm_integer(120));
        let mut publisher = push::SnapshotPublisher::new(slot.reader());
        let mut sink = RecordingSink::default();
        publisher.publish_destroy(&mut sink);
        assert!(matches!(sink.items[0].op, ObservationOp::Destroy { .. }));
    }
}
