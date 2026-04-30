//! Driver-shaped tool routing for the stdio adapter.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use agogo_core::conn::tempo::Tempo;
use serde_json::{Value, json};

use crate::rt_bridge::{BridgeError, ControlCommand, ControlProducer, spsc};

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

/// Initial agogo stdio tools.
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

/// agogo's stdio-facing driver shell.
///
/// This intentionally mirrors stdio-core's driver lifecycle without
/// importing stdio-core yet. The trait implementation lands once the
/// toolchain boundary is resolved.
#[derive(Debug)]
pub struct AgogoDriver {
    producer: Arc<ControlProducer>,
    mounted: AtomicBool,
}

impl AgogoDriver {
    pub fn new(config: AgogoDriverConfig) -> (Self, crate::RtControlConsumer) {
        let (producer, consumer) = spsc(config.queue_capacity, config.initial_tempo);
        (
            Self {
                producer: Arc::new(producer),
                mounted: AtomicBool::new(false),
            },
            consumer,
        )
    }

    pub fn from_producer(producer: ControlProducer) -> Self {
        Self {
            producer: Arc::new(producer),
            mounted: AtomicBool::new(false),
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

        match Tool::parse(tool).ok_or_else(|| format!("unknown agogo tool: {tool}"))? {
            Tool::TempoSet => {
                let tempo = parse_integer_bpm(&args)?;
                self.producer.set_tempo(tempo);
            }
            Tool::ChannelConfigure => {
                let channel = parse_u32_field(&args, "channel")?;
                self.push(ControlCommand::ChannelConfigure { channel })?;
            }
            Tool::Start => self.push(ControlCommand::Start)?,
            Tool::Stop => self.push(ControlCommand::Stop)?,
            Tool::Locate => {
                let tick = parse_u32_field(&args, "tick")?;
                self.push(ControlCommand::Locate { tick })?;
            }
        }

        Ok(accepted_next_buffer())
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

    fn push(&self, command: ControlCommand) -> Result<(), String> {
        self.producer.try_push(command).map_err(|err| match err {
            BridgeError::QueueFull => "agogo control queue is full".to_owned(),
            BridgeError::QueuePoisoned => "agogo control queue lock is poisoned".to_owned(),
        })
    }
}

fn accepted_next_buffer() -> Value {
    json!({
        "accepted": true,
        "applies_by": "next_buffer",
    })
}

fn parse_integer_bpm(args: &Value) -> Result<Tempo, String> {
    let bpm = parse_u32_field(args, "bpm")?;
    if bpm > Tempo::MAX_BPM_INTEGER {
        return Err(format!("field `bpm` must be <= {}", Tempo::MAX_BPM_INTEGER));
    }
    Ok(Tempo::from_bpm_integer(bpm))
}

fn parse_u32_field(args: &Value, field: &str) -> Result<u32, String> {
    let object = args
        .as_object()
        .ok_or_else(|| "tool arguments must be a JSON object".to_owned())?;
    let value = object
        .get(field)
        .ok_or_else(|| format!("missing integer field `{field}`"))?
        .as_u64()
        .ok_or_else(|| format!("field `{field}` must be an unsigned integer"))?;
    u32::try_from(value).map_err(|_| format!("field `{field}` exceeds u32"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rt_bridge::ControlCommand;

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

        assert_eq!(out, accepted_next_buffer());
        assert_eq!(consumer.snapshot().tempo, Tempo::from_bpm_integer(140));
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

        assert_eq!(consumer.try_pop(), Some(ControlCommand::Start));
        assert_eq!(
            consumer.try_pop(),
            Some(ControlCommand::Locate { tick: 960 })
        );
        assert_eq!(consumer.try_pop(), Some(ControlCommand::Stop));
    }

    #[test]
    fn queue_full_returns_tool_error() {
        let (driver, _consumer) = AgogoDriver::new(AgogoDriverConfig {
            queue_capacity: 1,
            ..AgogoDriverConfig::default()
        });
        driver.on_mount().expect("mount");
        driver
            .handle_call(Tool::Start.name(), json!({}))
            .expect("first command");
        let err = driver
            .handle_call(Tool::Stop.name(), json!({}))
            .unwrap_err();
        assert_eq!(err, "agogo control queue is full");
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
