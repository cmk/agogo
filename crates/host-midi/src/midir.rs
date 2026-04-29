//! `MidirSink` — midir-backed [`MidiSink`] implementation.

use agogo_core::out::midi::MidiSink;
use thiserror::Error;

/// midir-backed [`MidiSink`]. Opens a single output port at
/// construction time; every `send_at` flushes the bytes
/// immediately to the OS MIDI driver. The `at_sample` argument is
/// preserved as call-site metadata but ignored — midir has no
/// scheduler, so dispatch jitter inherits the platform's USB-bus
/// limit (~1 ms typical per `doc/agogo.md` §4).
///
/// `Mutex` interior wraps `MidiOutputConnection` because midir's
/// connection type is `!Sync`. The lock is only acquired on the
/// drain thread Plan 13 T3 will wire (`crates/host-cpal/src/cpal/
/// control.rs`); the audio thread never touches the sink.
pub struct MidirSink {
    conn: std::sync::Mutex<::midir::MidiOutputConnection>,
    port_name: String,
}

impl MidirSink {
    /// Open a midir output connection by port name. Names are those
    /// returned by [`Self::list_output_ports`]. Matching is
    /// case-sensitive.
    pub fn open(port_name: &str) -> Result<Self, MidirSinkError> {
        use ::midir::MidiOutput;

        let output =
            MidiOutput::new("agogo-host-midi").map_err(|e| MidirSinkError::Init(e.to_string()))?;
        let port = output
            .ports()
            .into_iter()
            .find(|p| matches!(output.port_name(p), Ok(ref n) if n == port_name))
            .ok_or_else(|| MidirSinkError::PortNotFound(port_name.to_owned()))?;
        let conn = output
            .connect(&port, "agogo-out")
            .map_err(|e| MidirSinkError::Connect(e.to_string()))?;

        Ok(Self {
            conn: std::sync::Mutex::new(conn),
            port_name: port_name.to_owned(),
        })
    }

    /// Enumerate the names of MIDI output ports visible to midir.
    ///
    /// A port whose name can't be fetched (e.g. a transient OS
    /// error mid-enumeration) is silently dropped from the result —
    /// this is best-effort discovery, not a hard error contract.
    pub fn list_output_ports() -> Result<Vec<String>, MidirSinkError> {
        use ::midir::MidiOutput;

        let output =
            MidiOutput::new("agogo-host-midi").map_err(|e| MidirSinkError::Init(e.to_string()))?;
        let names = output
            .ports()
            .into_iter()
            .filter_map(|p| output.port_name(&p).ok())
            .collect();
        Ok(names)
    }

    /// The name of the port this sink is connected to. Useful for
    /// diagnostic logging (`tracing` spans, CLI output).
    pub fn port_name(&self) -> &str {
        &self.port_name
    }
}

impl MidiSink for MidirSink {
    /// Forwards `msg` to the underlying `MidiOutputConnection`
    /// **immediately** — `at_sample` is metadata only. A failed
    /// send (typically a disconnected port) is logged via `tracing`
    /// and dropped: the alternative would be to surface the error
    /// up through `MidiSink::send_at`'s signature, which the
    /// trait deliberately doesn't carry. Plan 13's drain thread
    /// can poll the underlying `MidirSink` for diagnostic state if
    /// needed.
    fn send_at(&self, msg: &[u8], _at_sample: u64) {
        match self.conn.lock() {
            Ok(mut conn) => {
                if let Err(e) = conn.send(msg) {
                    tracing::warn!(?e, port = %self.port_name, "midir send failed");
                }
            }
            Err(poisoned) => {
                // Mutex poisoning means a previous panic interrupted a
                // `send_at` call. Try the send anyway — the connection
                // itself is still valid.
                let mut conn = poisoned.into_inner();
                if let Err(e) = conn.send(msg) {
                    tracing::warn!(?e, port = %self.port_name, "midir send failed (poisoned mutex)");
                }
            }
        }
    }
}

#[derive(Debug, Error)]
pub enum MidirSinkError {
    #[error("midir init: {0}")]
    Init(String),
    #[error("no MIDI output port: {0}")]
    PortNotFound(String),
    #[error("connect: {0}")]
    Connect(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `list_output_ports` should not panic. It may return an empty
    /// `Vec` on a host with no MIDI ports (typical for CI runners
    /// without a virtual MIDI bus); that's acceptable — Plan 14's
    /// `agogo run` acceptance path is where a real loopback
    /// fixture lands (Plan 13 T6 is deferred per the plan's
    /// Review section).
    #[test]
    fn list_output_ports_does_not_panic() {
        // Behaviour: returns Ok(...) even when empty. Surfaces
        // platform init errors (the Init variant) only when the
        // host's MIDI subsystem is fundamentally unavailable, which
        // would also fail any subsequent open call.
        let _result = MidirSink::list_output_ports();
    }

    /// Opening a bogus port name surfaces `PortNotFound` (or
    /// `Init` on hosts where the MIDI subsystem can't initialise at
    /// all — both are valid failure modes from the user's
    /// perspective).
    #[test]
    fn open_rejects_bogus_name() {
        let result = MidirSink::open("definitely-not-a-real-midi-port-\u{00A0}\u{2603}");
        match result {
            Err(MidirSinkError::PortNotFound(_))
            | Err(MidirSinkError::Init(_))
            | Err(MidirSinkError::Connect(_)) => {}
            Ok(_) => panic!("bogus name must not match a real port"),
        }
    }
}
