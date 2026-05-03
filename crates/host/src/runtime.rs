//! agogo-owned runtime helper for stdio-core adapter tests.
//!
//! This module deliberately stays free of stdio-core types. It packages
//! the driver, control bridge, playhead, and snapshot publisher behind a
//! small host-facing API that a sibling adapter can wrap.

use agogo::core::conn::sample::S048;
use agogo::core::control::{PhaseSource, Playhead, TransportPolicy};
use serde_json::Value;

use crate::bridge::{CommandApplyReport, ControlConsumer, apply_control_to_playhead};
use crate::driver::{AgogoDriver, AgogoDriverConfig, Tool};
use crate::snapshot::push::{ObservationParams, ObservationSink, SnapshotPublisher};
use crate::snapshot::{
    AGOGO_MAIN_ID, AGOGO_STATE_FORM_TYPE, AgogoSnapshot, RtAudioFrame, RtSnapshotFrame,
    RtSyncFrame, RtTransportFrame, SnapshotSlot, TransportStateCode,
};

const RUNTIME_SAMPLE_RATE: u32 = 48_000;
const RUNTIME_BUFFER_FRAMES: usize = 4_096;

/// agogo-native tool metadata for adapter mapping.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct RuntimeToolMetadata {
    pub tool: Tool,
    pub timing: &'static str,
    pub safety: &'static str,
}

/// agogo-native observation surface declaration for adapter mapping.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct RuntimeSurface {
    pub stream_id: &'static str,
    pub form_id: &'static str,
    pub form_type: &'static str,
}

/// Structured result of one runtime command step.
#[derive(Clone, Debug, PartialEq)]
pub struct RuntimeStepReport {
    pub admission: Value,
    pub apply: CommandApplyReport,
    pub snapshot_seq: u64,
    pub observation: Option<ObservationParams>,
}

/// Runtime bundle used by adapter and integration tests.
pub struct Runtime {
    driver: AgogoDriver,
    consumer: ControlConsumer,
    playhead: Playhead<S048>,
    snapshot_slot: SnapshotSlot,
    publisher: SnapshotPublisher,
}

impl Runtime {
    /// Build the default runtime at 120 BPM.
    pub fn new() -> Self {
        Self::with_config(AgogoDriverConfig::default())
    }

    /// Build a runtime with caller-selected driver settings.
    pub fn with_config(config: AgogoDriverConfig) -> Self {
        let (driver, consumer) = AgogoDriver::new(config);
        let playhead = Playhead::<S048>::new(
            Vec::new(),
            PhaseSource::Internal {
                bpm: config.initial_tempo,
            },
            RUNTIME_SAMPLE_RATE,
            config.initial_tempo,
            TransportPolicy::Internal {
                start_emitted: true,
            },
            RUNTIME_BUFFER_FRAMES,
        );
        let snapshot_slot = SnapshotSlot::new(config.initial_tempo);
        let publisher = SnapshotPublisher::new(snapshot_slot.reader());
        Self {
            driver,
            consumer,
            playhead,
            snapshot_slot,
            publisher,
        }
    }

    pub fn mount(&self) -> Result<(), String> {
        self.driver.on_mount()
    }

    pub fn unmount(&self) -> Result<(), String> {
        self.driver.on_unmount()
    }

    pub fn handle_call(&self, tool: &str, args: Value) -> Result<Value, String> {
        self.driver.handle_call(tool, args)
    }

    /// Advance one RT buffer and publish the resulting snapshot frame.
    pub fn advance_buffer(&mut self) -> CommandApplyReport {
        let report = apply_control_to_playhead(&mut self.consumer, &mut self.playhead);
        self.write_snapshot(report);
        report
    }

    pub fn publish_snapshot<S: ObservationSink>(&mut self, sink: &mut S) -> Result<bool, String> {
        self.publisher.publish_next(sink)
    }

    pub fn snapshot(&self) -> AgogoSnapshot {
        self.snapshot_slot.reader().snapshot()
    }

    /// Run the scoped command path and capture the observation, if one is emitted.
    pub fn run_command_step(
        &mut self,
        tool: &str,
        args: Value,
    ) -> Result<RuntimeStepReport, String> {
        let admission = self.handle_call(tool, args)?;
        let apply = self.advance_buffer();
        let snapshot_seq = self.snapshot().seq;
        let mut sink = CaptureSink::default();
        let observation = self
            .publish_snapshot(&mut sink)?
            .then_some(sink.item)
            .flatten();
        Ok(RuntimeStepReport {
            admission,
            apply,
            snapshot_seq,
            observation,
        })
    }

    pub fn tool_metadata() -> &'static [RuntimeToolMetadata] {
        &[
            RuntimeToolMetadata {
                tool: Tool::TempoSet,
                timing: "rt_buffer_deadline",
                safety: "last_value_tempo",
            },
            RuntimeToolMetadata {
                tool: Tool::Start,
                timing: "rt_buffer_deadline",
                safety: "ordered_transport",
            },
            RuntimeToolMetadata {
                tool: Tool::Stop,
                timing: "rt_buffer_deadline",
                safety: "ordered_transport",
            },
            RuntimeToolMetadata {
                tool: Tool::Locate,
                timing: "rt_buffer_deadline",
                safety: "unsupported_v0_2",
            },
            RuntimeToolMetadata {
                tool: Tool::ChannelConfigure,
                timing: "rt_buffer_deadline",
                safety: "unsupported_v0_2",
            },
        ]
    }

    pub const fn surface() -> RuntimeSurface {
        RuntimeSurface {
            stream_id: AGOGO_MAIN_ID,
            form_id: AGOGO_MAIN_ID,
            form_type: AGOGO_STATE_FORM_TYPE,
        }
    }

    fn write_snapshot(&self, report: CommandApplyReport) -> u64 {
        let state = if self.playhead.is_running() {
            TransportStateCode::Running
        } else {
            TransportStateCode::Stopped
        };
        self.snapshot_slot.writer().write(&RtSnapshotFrame {
            bpm: report.params.tempo,
            transport: RtTransportFrame {
                state,
                bar: 0,
                beat: 0,
                tick: report.params.buffer_epoch as u32,
            },
            sync: RtSyncFrame::default(),
            audio: RtAudioFrame {
                sample_rate: RUNTIME_SAMPLE_RATE,
                buffer_size: RUNTIME_BUFFER_FRAMES as u32,
                load_raw: 0,
            },
            channels: &[],
        })
    }
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Default)]
struct CaptureSink {
    item: Option<ObservationParams>,
}

impl ObservationSink for CaptureSink {
    fn dispatch(&mut self, params: ObservationParams) {
        self.item = Some(params);
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::bridge::CommandTimeDomain;
    use crate::snapshot::push::{FormType, ObservationOp, StreamId};
    use agogo::core::conn::tempo::Tempo;

    #[test]
    fn runtime_tempo_set_applies_and_snapshots() {
        let mut runtime = Runtime::new();
        runtime.mount().expect("mount");

        let report = runtime
            .run_command_step(
                Tool::TempoSet.name(),
                json!({
                    "bpm": 132,
                    "source_id": "test",
                    "command_id": 7,
                    "time_domain": "rt_buffer",
                    "deadline_buffer": 1,
                }),
            )
            .expect("tempo command");

        assert_eq!(report.admission["status"], json!("accepted"));
        assert_eq!(report.admission["command_id"], json!(7));
        assert_eq!(report.admission["source_id"], json!("test"));
        assert_eq!(
            report.admission["time_domain"],
            json!(CommandTimeDomain::RtBuffer.as_str())
        );
        assert_eq!(report.apply.params.tempo, Tempo::from_bpm_integer(132));
        assert!(report.apply.tempo_updated);
        assert_eq!(report.snapshot_seq, 1);

        let snapshot = runtime.snapshot();
        assert_eq!(snapshot.seq, 1);
        assert_eq!(snapshot.bpm.0, Tempo::from_bpm_integer(132).0);
        assert_eq!(
            snapshot.transport.state,
            crate::snapshot::TransportState::Running
        );
        assert!(matches!(
            report.observation.map(|params| params.op),
            Some(ObservationOp::Create {
                form_id,
                form_type: FormType::Other(form_type),
                ..
            }) if form_id == AGOGO_MAIN_ID && form_type == AGOGO_STATE_FORM_TYPE
        ));
    }

    #[test]
    fn runtime_start_applies_and_snapshots() {
        let mut runtime = Runtime::new();
        runtime.mount().expect("mount");

        let report = runtime
            .run_command_step(
                Tool::Start.name(),
                json!({
                    "source_id": "test",
                    "command_id": 8,
                    "time_domain": "rt_buffer",
                    "deadline_buffer": 1,
                }),
            )
            .expect("start command");

        assert_eq!(report.admission["status"], json!("accepted"));
        assert_eq!(report.apply.applied_commands, 1);
        assert_eq!(report.apply.missed_deadlines, 0);
        assert_eq!(
            runtime.snapshot().transport.state,
            crate::snapshot::TransportState::Running
        );
    }

    #[test]
    fn runtime_control_without_observation_sink() {
        let mut runtime = Runtime::new();
        runtime.mount().expect("mount");

        let admission = runtime
            .handle_call(
                Tool::TempoSet.name(),
                json!({ "bpm": 140, "deadline_buffer": 1 }),
            )
            .expect("admit without observer");
        let apply = runtime.advance_buffer();

        assert_eq!(admission["status"], json!("accepted"));
        assert_eq!(apply.params.tempo, Tempo::from_bpm_integer(140));
        assert_eq!(runtime.snapshot().bpm.0, Tempo::from_bpm_integer(140).0);
    }

    #[test]
    fn runtime_metadata_matches_stdio_core_contract() {
        let names: Vec<_> = Runtime::tool_metadata()
            .iter()
            .map(|metadata| metadata.tool.name())
            .collect();
        assert_eq!(
            names,
            vec![
                "agogo.tempo.set",
                "agogo.start",
                "agogo.stop",
                "agogo.locate",
                "agogo.channel.configure",
            ]
        );

        let surface = Runtime::surface();
        assert_eq!(surface.stream_id, AGOGO_MAIN_ID);
        assert_eq!(surface.form_id, AGOGO_MAIN_ID);
        assert_eq!(surface.form_type, AGOGO_STATE_FORM_TYPE);
        assert_eq!(StreamId(surface.stream_id.to_owned()).0, "agogo.main");
    }
}
