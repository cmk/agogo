#![forbid(unsafe_code)]

//! Public facade namespace for the agogo workspace.
//!
//! The implementation crates stay split for build hygiene and optional
//! backend dependencies, while this crate presents the project as a
//! nested API:
//!
//! - [`core`] for clock, control, channel, sink, and time primitives.
//! - [`host`] for host-side drivers and optional backends.

#[cfg(feature = "core")]
pub mod core {
    //! Core scheduling, control, sink, and time primitives.

    pub use core_impl::*;
}

#[cfg(feature = "host")]
pub mod host {
    //! Host-side drivers and optional host backends.

    pub use host_impl::*;

    #[cfg(feature = "cpal")]
    pub mod cpal {
        //! cpal audio backend.

        pub use cpal_impl::cpal::*;
    }

    #[cfg(feature = "link")]
    pub mod link {
        //! Ableton Link backend.

        pub use link_impl::*;
    }

    #[cfg(feature = "midi")]
    pub mod midi {
        //! midir MIDI backend.

        pub use midi_impl::*;
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn core_and_host_namespace_paths_compile() {
        let tempo = crate::core::conn::tempo::Tempo::from_bpm_integer(120);
        let _driver = crate::host::AgogoDriver::new(crate::host::AgogoDriverConfig::default());
        assert_eq!(
            tempo,
            crate::core::conn::tempo::Tempo::from_bpm_integer(120)
        );
    }

    #[cfg(feature = "cpal")]
    #[test]
    fn cpal_namespace_paths_compile() {
        let _list_inputs: fn() -> Vec<String> = crate::host::cpal::CpalHost::list_input_devices;
        let _ = core::mem::size_of::<
            crate::host::cpal::callback::CallbackState<crate::core::conn::sample::S048>,
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
