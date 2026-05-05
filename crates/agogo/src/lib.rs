#![forbid(unsafe_code)]
#![cfg_attr(docsrs, feature(doc_cfg))]

//! Public facade namespace for the agogo workspace.
//!
//! The implementation crates stay split for build hygiene and optional
//! backend dependencies, while this crate presents the project as a
//! nested API:
//!
//! - [`chan`] for pure channel, control, sink, connection, and time primitives.
//! - [`core`] for runtime orchestration.
//! - [`host`] for host-side drivers and optional backends.

#[cfg(feature = "core")]
#[cfg_attr(docsrs, doc(cfg(feature = "core")))]
pub mod chan {
    //! Pure channel, control, sink, connection, and time primitives.

    pub use chan_impl::*;
}

#[cfg(feature = "core")]
#[cfg_attr(docsrs, doc(cfg(feature = "core")))]
pub mod core {
    //! Runtime orchestration primitives.

    pub use core_impl::{
        AdmissionMetadata, AdmissionOutcome, AdmissionRejectReason, AdmissionStatus, AgogoDriver,
        AgogoDriverConfig, AgogoSnapshot, BridgeError, CoalesceKey, CommandApplyReport,
        CommandDeadline, CommandEnvelope, CommandId, CommandTimeDomain, ControlCommand,
        ControlConsumer, ControlParams, ControlProducer, MAX_OUTPUT_CHANNELS, OfflineMidiRecord,
        OfflinePcm, OfflineRenderConfig, OfflineRenderError, OfflineRenderReport, Playhead,
        PlayheadStopHandle, RtCommandDrain, Runtime, RuntimeStepReport, RuntimeSurface,
        RuntimeToolMetadata, SnapshotSlot, SourceId, Tool, TransportCommandApply, TransportPolicy,
        TransportState, apply_control_to_playhead, max_events_for_buffer, render_offline,
        render_offline_capture, spsc, tick_stream, tick_stream_into, validate_audio_lanes,
    };
}

#[cfg(feature = "host")]
#[cfg_attr(docsrs, doc(cfg(feature = "host")))]
pub mod host {
    //! Host-side drivers and optional host backends.

    pub use core_impl::*;

    #[cfg(feature = "cpal")]
    #[cfg_attr(docsrs, doc(cfg(feature = "cpal")))]
    pub mod cpal {
        //! cpal audio backend.

        pub use cpal_impl::cpal::*;
    }

    #[cfg(feature = "link")]
    #[cfg_attr(docsrs, doc(cfg(feature = "link")))]
    pub mod link {
        //! Ableton Link backend.

        pub use link_impl::*;
    }

    #[cfg(feature = "midi")]
    #[cfg_attr(docsrs, doc(cfg(feature = "midi")))]
    pub mod midi {
        //! midir MIDI backend.

        pub use midi_impl::*;
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "core")]
    #[test]
    fn chan_namespace_paths_compile() {
        let tempo = crate::chan::conn::tempo::Tempo::from_bpm_integer(120);
        assert_eq!(
            tempo,
            crate::chan::conn::tempo::Tempo::from_bpm_integer(120)
        );
    }

    #[cfg(feature = "host")]
    #[test]
    fn core_and_host_namespace_paths_compile() {
        let tempo = crate::chan::conn::tempo::Tempo::from_bpm_integer(120);
        let _driver = crate::host::AgogoDriver::new(crate::host::AgogoDriverConfig::default());
        assert_eq!(
            tempo,
            crate::chan::conn::tempo::Tempo::from_bpm_integer(120)
        );
    }

    #[cfg(feature = "cpal")]
    #[test]
    fn cpal_namespace_paths_compile() {
        let _list_inputs: fn() -> Vec<String> = crate::host::cpal::CpalHost::list_input_devices;
        let _ = core::mem::size_of::<
            crate::host::cpal::callback::CallbackState<crate::chan::conn::rate::R048>,
        >();
    }

    #[cfg(feature = "link")]
    #[test]
    fn link_namespace_paths_compile() {
        let _ = core::mem::size_of::<crate::host::link::session::LinkWriteConfig>();
    }

    #[cfg(feature = "midi")]
    #[test]
    fn midi_namespace_paths_compile() {
        let _list_outputs: fn() -> Result<Vec<String>, crate::host::midi::MidirSinkError> =
            crate::host::midi::MidirSink::list_output_ports;
    }
}
