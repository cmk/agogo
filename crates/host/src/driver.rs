//! Driver-shaped tool routing for the host adapter.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use agogo::core::conn::tempo::Tempo;
use serde_json::{Value, json};

use crate::bridge::{
    AdmissionMetadata, AdmissionOutcome, CoalesceKey, CommandDeadline, CommandId,
    CommandTimeDomain, ControlCommand, ControlProducer, MAX_COALESCE_KEY_LEN, MAX_SOURCE_ID_LEN,
    SourceId, spsc,
};

/// Initial adapter configuration.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct AgogoDriverConfig {
    pub queue_capacity: usize,
    pub initial_tempo: Tempo,
}

impl Default for AgogoDriverConfig {
    fn default() -> Self {
        Self {
            queue_capacity: 128,
            initial_tempo: Tempo::from_bpm_integer(120),
        }
    }
}

/// Initial agogo host tools.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Tool {
    TempoSet,
    ChannelConfigure,
    Start,
    Stop,
    Locate,
}

impl Tool {
    pub const ALL: [Self; 5] = [
        Self::TempoSet,
        Self::ChannelConfigure,
        Self::Start,
        Self::Stop,
        Self::Locate,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::TempoSet => "agogo.tempo.set",
            Self::ChannelConfigure => "agogo.channel.configure",
            Self::Start => "agogo.start",
            Self::Stop => "agogo.stop",
            Self::Locate => "agogo.locate",
        }
    }

    fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|tool| tool.name() == name)
    }
}

/// agogo's host-facing driver shell.
///
/// This intentionally mirrors stdio-core's driver lifecycle without
/// importing stdio-core yet. The trait implementation lands once the
/// toolchain boundary is resolved.
#[derive(Debug)]
pub struct AgogoDriver {
    producer: Arc<ControlProducer>,
    mounted: AtomicBool,
    next_command_id: AtomicU64,
}

impl AgogoDriver {
    pub fn new(config: AgogoDriverConfig) -> (Self, crate::ControlConsumer) {
        let (producer, consumer) = spsc(config.queue_capacity, config.initial_tempo);
        (
            Self {
                producer: Arc::new(producer),
                mounted: AtomicBool::new(false),
                next_command_id: AtomicU64::new(1),
            },
            consumer,
        )
    }

    pub fn from_producer(producer: ControlProducer) -> Self {
        Self {
            producer: Arc::new(producer),
            mounted: AtomicBool::new(false),
            next_command_id: AtomicU64::new(1),
        }
    }

    pub fn name(&self) -> &str {
        "agogo"
    }

    pub fn tool_names(&self) -> Vec<String> {
        Tool::ALL
            .into_iter()
            .map(|tool| tool.name().to_owned())
            .collect()
    }

    pub fn on_mount(&self) -> Result<(), String> {
        self.mounted.store(true, Ordering::Release);
        Ok(())
    }

    pub fn on_unmount(&self) -> Result<(), String> {
        self.mounted.store(false, Ordering::Release);
        Ok(())
    }

    pub fn is_mounted(&self) -> bool {
        self.mounted.load(Ordering::Acquire)
    }

    pub fn handle_call(&self, tool: &str, args: Value) -> Result<Value, String> {
        if !self.is_mounted() {
            return Err("agogo driver is not mounted".to_owned());
        }

        let outcome =
            match Tool::parse(tool).ok_or_else(|| format!("unknown agogo tool: {tool}"))? {
                Tool::TempoSet => {
                    let tempo = parse_integer_bpm(&args)?;
                    let metadata = self.parse_metadata(&args, Some(CoalesceKey::tempo()))?;
                    self.producer.admit_tempo(tempo, metadata)
                }
                Tool::ChannelConfigure => {
                    let channel = parse_u32_field(&args, "channel")?;
                    let metadata = self.parse_metadata(&args, None)?;
                    self.producer
                        .admit_ordered(ControlCommand::ChannelConfigure { channel }, metadata)
                }
                Tool::Start => {
                    let metadata = self.parse_metadata(&args, None)?;
                    self.producer.admit_ordered(ControlCommand::Start, metadata)
                }
                Tool::Stop => {
                    let metadata = self.parse_metadata(&args, None)?;
                    self.producer.admit_ordered(ControlCommand::Stop, metadata)
                }
                Tool::Locate => {
                    let tick = parse_u32_field(&args, "tick")?;
                    let metadata = self.parse_metadata(&args, None)?;
                    self.producer
                        .admit_ordered(ControlCommand::Locate { tick }, metadata)
                }
            };

        Ok(admission_response(outcome))
    }

    pub fn inverse_op(&self, tool: &str, args: &Value) -> Option<(String, Value)> {
        match Tool::parse(tool)? {
            Tool::TempoSet => args
                .get("prior_bpm")
                .and_then(Value::as_u64)
                .filter(|bpm| *bpm <= u64::from(Tempo::MAX_BPM_INTEGER))
                .map(|bpm| (Tool::TempoSet.name().to_owned(), json!({ "bpm": bpm }))),
            Tool::Start => Some((Tool::Stop.name().to_owned(), json!({}))),
            Tool::Stop => Some((Tool::Start.name().to_owned(), json!({}))),
            Tool::ChannelConfigure | Tool::Locate => None,
        }
    }

    fn parse_metadata(
        &self,
        args: &Value,
        default_coalesce_key: Option<CoalesceKey>,
    ) -> Result<AdmissionMetadata, String> {
        let object = json_object(args)?;
        let command_id = match object.get("command_id") {
            Some(value) => {
                let id = value
                    .as_u64()
                    .ok_or_else(|| "field `command_id` must be an unsigned integer".to_owned())?;
                if id == u64::MAX {
                    return Err("field `command_id` must be less than u64::MAX".to_owned());
                }
                let command_id = CommandId(id);
                self.reserve_generated_ids_through(command_id);
                command_id
            }
            None => self.next_generated_command_id()?,
        };
        let source_id = match object.get("source_id") {
            Some(value) => {
                let source = value
                    .as_str()
                    .ok_or_else(|| "field `source_id` must be a string".to_owned())?;
                SourceId::new(source).ok_or_else(|| {
                    format!("field `source_id` must be <= {MAX_SOURCE_ID_LEN} bytes")
                })?
            }
            None => SourceId::default(),
        };
        let time_domain = match object.get("time_domain") {
            Some(value) => {
                let domain = value
                    .as_str()
                    .ok_or_else(|| "field `time_domain` must be a string".to_owned())?;
                CommandTimeDomain::parse(domain)
            }
            None => CommandTimeDomain::RtBuffer,
        };
        let deadline_buffer = match object.get("deadline_buffer") {
            Some(value) => value
                .as_u64()
                .ok_or_else(|| "field `deadline_buffer` must be an unsigned integer".to_owned())?,
            None => self.producer.default_deadline_buffer(),
        };
        let coalesce_key = match object.get("coalesce_key") {
            Some(Value::Null) => None,
            Some(value) => {
                let key = value
                    .as_str()
                    .ok_or_else(|| "field `coalesce_key` must be a string".to_owned())?;
                Some(CoalesceKey::new(key).ok_or_else(|| {
                    format!("field `coalesce_key` must be <= {MAX_COALESCE_KEY_LEN} bytes")
                })?)
            }
            None => default_coalesce_key,
        };

        Ok(AdmissionMetadata {
            command_id,
            source_id,
            deadline: CommandDeadline {
                time_domain,
                buffer: deadline_buffer,
            },
            coalesce_key,
        })
    }

    fn next_generated_command_id(&self) -> Result<CommandId, String> {
        self.next_command_id
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                current.checked_add(1)
            })
            .map(CommandId)
            .map_err(|_| "generated command id range exhausted".to_owned())
    }

    fn reserve_generated_ids_through(&self, command_id: CommandId) {
        let next = command_id
            .get()
            .checked_add(1)
            .expect("caller rejects u64::MAX command id");
        let _ =
            self.next_command_id
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                    (current < next).then_some(next)
                });
    }
}

fn admission_response(outcome: AdmissionOutcome) -> Value {
    let mut value = json!({
        "status": outcome.status.as_str(),
        "accepted": outcome.is_accepted(),
        "command_id": outcome.metadata.command_id.get(),
        "source_id": outcome.metadata.source_id.as_str(),
        "time_domain": outcome.metadata.deadline.time_domain.as_str(),
        "deadline_buffer": outcome.metadata.deadline.buffer,
    });
    let object = value
        .as_object_mut()
        .expect("admission response is an object");
    if let Some(key) = outcome.metadata.coalesce_key {
        object.insert("coalesce_key".to_owned(), json!(key.as_str()));
    }
    if let Some(reason) = outcome.reason {
        object.insert("reason".to_owned(), json!(reason.as_str()));
    }
    value
}

fn parse_integer_bpm(args: &Value) -> Result<Tempo, String> {
    let bpm = parse_u32_field(args, "bpm")?;
    if bpm > Tempo::MAX_BPM_INTEGER {
        return Err(format!("field `bpm` must be <= {}", Tempo::MAX_BPM_INTEGER));
    }
    Ok(Tempo::from_bpm_integer(bpm))
}

fn parse_u32_field(args: &Value, field: &str) -> Result<u32, String> {
    let object = json_object(args)?;
    let value = object
        .get(field)
        .ok_or_else(|| format!("missing integer field `{field}`"))?
        .as_u64()
        .ok_or_else(|| format!("field `{field}` must be an unsigned integer"))?;
    u32::try_from(value).map_err(|_| format!("field `{field}` exceeds u32"))
}

fn json_object(args: &Value) -> Result<&serde_json::Map<String, Value>, String> {
    args.as_object()
        .ok_or_else(|| "tool arguments must be a JSON object".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::ControlCommand;

    #[test]
    fn driver_advertises_initial_tool_set() {
        let (driver, _consumer) = AgogoDriver::new(AgogoDriverConfig::default());
        assert_eq!(driver.name(), "agogo");
        assert_eq!(
            driver.tool_names(),
            vec![
                "agogo.tempo.set",
                "agogo.channel.configure",
                "agogo.start",
                "agogo.stop",
                "agogo.locate",
            ]
        );
    }

    #[test]
    fn unmounted_driver_rejects_calls() {
        let (driver, _consumer) = AgogoDriver::new(AgogoDriverConfig::default());
        let err = driver
            .handle_call(Tool::Start.name(), json!({}))
            .unwrap_err();
        assert_eq!(err, "agogo driver is not mounted");
    }

    #[test]
    fn tempo_set_response_names_next_buffer() {
        let (driver, consumer) = AgogoDriver::new(AgogoDriverConfig::default());
        driver.on_mount().expect("mount");

        let out = driver
            .handle_call(Tool::TempoSet.name(), json!({ "bpm": 140 }))
            .expect("tempo set");

        assert_eq!(
            out,
            json!({
                "status": "accepted",
                "accepted": true,
                "command_id": 1,
                "source_id": "agogo.driver",
                "time_domain": "rt_buffer",
                "deadline_buffer": 1,
                "coalesce_key": "tempo",
            })
        );
        assert_eq!(consumer.begin_buffer().tempo, Tempo::from_bpm_integer(140));
    }

    #[test]
    fn tempo_set_rejects_non_tempo_coalesce_key() {
        let (driver, consumer) = AgogoDriver::new(AgogoDriverConfig::default());
        driver.on_mount().expect("mount");

        let out = driver
            .handle_call(
                Tool::TempoSet.name(),
                json!({ "bpm": 140, "coalesce_key": "transport" }),
            )
            .expect("rejected admission response");

        assert_eq!(
            out,
            json!({
                "status": "rejected",
                "accepted": false,
                "command_id": 1,
                "source_id": "agogo.driver",
                "time_domain": "rt_buffer",
                "deadline_buffer": 1,
                "coalesce_key": "transport",
                "reason": "unsupported_command_class",
            })
        );
        assert_eq!(consumer.begin_buffer().tempo, Tempo::from_bpm_integer(120));
    }

    #[test]
    fn tempo_set_rejects_bpm_above_tempo_range() {
        let (driver, _consumer) = AgogoDriver::new(AgogoDriverConfig::default());
        driver.on_mount().expect("mount");

        let err = driver
            .handle_call(
                Tool::TempoSet.name(),
                json!({ "bpm": Tempo::MAX_BPM_INTEGER + 1 }),
            )
            .unwrap_err();

        assert_eq!(
            err,
            format!("field `bpm` must be <= {}", Tempo::MAX_BPM_INTEGER)
        );
    }

    #[test]
    fn tempo_set_rejects_non_integer_bpm_with_precise_error() {
        let (driver, _consumer) = AgogoDriver::new(AgogoDriverConfig::default());
        driver.on_mount().expect("mount");

        let err = driver
            .handle_call(Tool::TempoSet.name(), json!({ "bpm": "140" }))
            .unwrap_err();

        assert_eq!(err, "field `bpm` must be an unsigned integer");
    }

    #[test]
    fn tempo_set_rejects_non_object_args_with_precise_error() {
        let (driver, _consumer) = AgogoDriver::new(AgogoDriverConfig::default());
        driver.on_mount().expect("mount");

        let err = driver
            .handle_call(Tool::TempoSet.name(), Value::Null)
            .unwrap_err();

        assert_eq!(err, "tool arguments must be a JSON object");
    }

    #[test]
    fn ordered_tools_reach_rt_consumer() {
        let (driver, mut consumer) = AgogoDriver::new(AgogoDriverConfig::default());
        driver.on_mount().expect("mount");

        driver
            .handle_call(Tool::Start.name(), json!({}))
            .expect("start");
        driver
            .handle_call(Tool::Locate.name(), json!({ "tick": 960 }))
            .expect("locate");
        driver
            .handle_call(Tool::Stop.name(), json!({}))
            .expect("stop");

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
    }

    #[test]
    fn queue_full_returns_rejected_admission() {
        let (driver, _consumer) = AgogoDriver::new(AgogoDriverConfig {
            queue_capacity: 1,
            ..AgogoDriverConfig::default()
        });
        driver.on_mount().expect("mount");
        driver
            .handle_call(Tool::Start.name(), json!({}))
            .expect("first command");
        let out = driver
            .handle_call(Tool::Stop.name(), json!({}))
            .expect("rejected admission response");
        assert_eq!(
            out,
            json!({
                "status": "rejected",
                "accepted": false,
                "command_id": 2,
                "source_id": "agogo.driver",
                "time_domain": "rt_buffer",
                "deadline_buffer": 1,
                "reason": "queue_full",
            })
        );
    }

    #[test]
    fn ordered_tool_with_coalesce_key_rejected() {
        let (driver, mut consumer) = AgogoDriver::new(AgogoDriverConfig::default());
        driver.on_mount().expect("mount");

        let out = driver
            .handle_call(Tool::Start.name(), json!({ "coalesce_key": "transport" }))
            .expect("rejected admission response");

        assert_eq!(
            out,
            json!({
                "status": "rejected",
                "accepted": false,
                "command_id": 1,
                "source_id": "agogo.driver",
                "time_domain": "rt_buffer",
                "deadline_buffer": 1,
                "coalesce_key": "transport",
                "reason": "unsupported_command_class",
            })
        );
        assert_eq!(consumer.try_pop(), None);
    }

    #[test]
    fn locate_with_stale_deadline_returns_late() {
        let (driver, mut consumer) = AgogoDriver::new(AgogoDriverConfig::default());
        driver.on_mount().expect("mount");
        consumer.begin_buffer();

        let out = driver
            .handle_call(
                Tool::Locate.name(),
                json!({ "tick": 960, "deadline_buffer": 1 }),
            )
            .expect("late admission response");

        assert_eq!(
            out,
            json!({
                "status": "late",
                "accepted": false,
                "command_id": 1,
                "source_id": "agogo.driver",
                "time_domain": "rt_buffer",
                "deadline_buffer": 1,
                "reason": "late_deadline",
            })
        );
        assert_eq!(consumer.try_pop(), None);
    }

    #[test]
    fn unsupported_time_domain_rejected_before_queue() {
        let (driver, mut consumer) = AgogoDriver::new(AgogoDriverConfig::default());
        driver.on_mount().expect("mount");

        let out = driver
            .handle_call(Tool::Start.name(), json!({ "time_domain": "host_time" }))
            .expect("rejected admission response");

        assert_eq!(
            out,
            json!({
                "status": "rejected",
                "accepted": false,
                "command_id": 1,
                "source_id": "agogo.driver",
                "time_domain": "host_time",
                "deadline_buffer": 1,
                "reason": "unsupported_time_domain",
            })
        );
        assert_eq!(consumer.try_pop(), None);
    }

    #[test]
    fn malformed_metadata_returns_tool_error() {
        let (driver, _consumer) = AgogoDriver::new(AgogoDriverConfig::default());
        driver.on_mount().expect("mount");

        let err = driver
            .handle_call(Tool::Start.name(), json!({ "source_id": 7 }))
            .unwrap_err();

        assert_eq!(err, "field `source_id` must be a string");
    }

    #[test]
    fn metadata_fields_are_echoed() {
        let (driver, mut consumer) = AgogoDriver::new(AgogoDriverConfig::default());
        driver.on_mount().expect("mount");

        let out = driver
            .handle_call(
                Tool::Start.name(),
                json!({
                    "command_id": 42,
                    "source_id": "stdio-core",
                    "deadline_buffer": 3,
                }),
            )
            .expect("accepted admission response");

        assert_eq!(
            out,
            json!({
                "status": "accepted",
                "accepted": true,
                "command_id": 42,
                "source_id": "stdio-core",
                "time_domain": "rt_buffer",
                "deadline_buffer": 3,
            })
        );
        let envelope = consumer.try_pop().expect("enqueued command");
        assert_eq!(envelope.command, ControlCommand::Start);
        assert_eq!(envelope.metadata.command_id.get(), 42);
        assert_eq!(envelope.metadata.source_id.as_str(), "stdio-core");
    }

    #[test]
    fn explicit_command_id_advances_generated_ids() {
        let (driver, _consumer) = AgogoDriver::new(AgogoDriverConfig::default());
        driver.on_mount().expect("mount");

        let explicit = driver
            .handle_call(Tool::Start.name(), json!({ "command_id": 42 }))
            .expect("accepted explicit id response");
        let generated = driver
            .handle_call(Tool::Stop.name(), json!({}))
            .expect("accepted generated id response");

        assert_eq!(explicit["command_id"], json!(42));
        assert_eq!(generated["command_id"], json!(43));
    }

    #[test]
    fn explicit_command_id_rejects_u64_max() {
        let (driver, _consumer) = AgogoDriver::new(AgogoDriverConfig::default());
        driver.on_mount().expect("mount");

        let err = driver
            .handle_call(Tool::Start.name(), json!({ "command_id": u64::MAX }))
            .unwrap_err();
        let generated = driver
            .handle_call(Tool::Stop.name(), json!({}))
            .expect("generated id still available");

        assert_eq!(err, "field `command_id` must be less than u64::MAX");
        assert_eq!(generated["command_id"], json!(1));
    }

    #[test]
    fn generated_command_id_rejects_exhausted_range() {
        let (driver, _consumer) = AgogoDriver::new(AgogoDriverConfig::default());
        driver.on_mount().expect("mount");
        driver.next_command_id.store(u64::MAX, Ordering::Relaxed);

        let err = driver
            .handle_call(Tool::Start.name(), json!({}))
            .unwrap_err();

        assert_eq!(err, "generated command id range exhausted");
    }

    #[test]
    fn inverse_op_is_round_trip_for_tempo_arg_with_prior() {
        let (driver, _consumer) = AgogoDriver::new(AgogoDriverConfig::default());
        assert_eq!(
            driver.inverse_op(
                Tool::TempoSet.name(),
                &json!({ "bpm": 140, "prior_bpm": 120 })
            ),
            Some((Tool::TempoSet.name().to_owned(), json!({ "bpm": 120 })))
        );
    }

    #[test]
    fn inverse_op_rejects_out_of_range_prior_tempo() {
        let (driver, _consumer) = AgogoDriver::new(AgogoDriverConfig::default());
        assert_eq!(
            driver.inverse_op(
                Tool::TempoSet.name(),
                &json!({ "bpm": 140, "prior_bpm": u64::from(Tempo::MAX_BPM_INTEGER) + 1 })
            ),
            None
        );
    }

    #[test]
    fn mount_unmount_idempotent() {
        let (driver, _consumer) = AgogoDriver::new(AgogoDriverConfig::default());
        driver.on_mount().expect("mount 1");
        driver.on_mount().expect("mount 2");
        assert!(driver.is_mounted());
        driver.on_unmount().expect("unmount 1");
        driver.on_unmount().expect("unmount 2");
        assert!(!driver.is_mounted());
    }
}
