//! `LinkSession` — thin orchestrator composing `LinkClock`, the
//! transport FSM, and the per-session quantum.
//!
//! Stubbed in T0; filled out in T4 once tempo-push (T1) and the FSM
//! (T2) + quantum snap (T3) are in place.

use agogo_core::fxp::{Quantum, Tempo};

use crate::link::{HostTimeAnchor, LinkClock};
use crate::transport::{TransportEvent, TransportFsm, TransportOutput, TransportState};

/// Write-path configuration for `LinkSession`. Tunes which classes
/// of transport / tempo events agogo emits vs. only observes.
#[derive(Debug, Clone, Copy)]
pub struct LinkWriteConfig {
    /// When `true`, `poll_transport` drives the FSM with
    /// `LinkReports*` events that mirror the network transport.
    pub enable_start_stop_sync: bool,
    /// When `true`, `user_start` / `user_stop` publish one-shot to
    /// the Link session via `set_is_playing`. When `false`, the FSM
    /// state still flips but no network write happens.
    pub enable_start_stop: bool,
    /// Default quantum used when channels don't override it.
    pub default_quantum: Quantum,
    /// When `true`, `set_tempo` on this session pushes the new tempo
    /// to the Link network. When `false`, only the local session's
    /// view of tempo changes (useful for hold-and-preview UI flows).
    pub push_tempo_on_change: bool,
}

impl Default for LinkWriteConfig {
    fn default() -> Self {
        Self {
            enable_start_stop_sync: true,
            enable_start_stop: true,
            default_quantum: Quantum::from_bars(4),
            push_tempo_on_change: true,
        }
    }
}

/// Thin orchestrator — owns a `LinkClock`, the transport FSM, and the
/// per-session quantum. Plan 06's `Machine` eventually absorbs this.
pub struct LinkSession {
    clock: LinkClock,
    transport: TransportFsm,
    #[allow(dead_code)] // consumed in T3; placeholder in T0.
    quantum: Quantum,
    config: LinkWriteConfig,
}

impl LinkSession {
    /// Construct a session. The underlying `LinkClock` is constructed
    /// but not enabled — call `enable` explicitly.
    pub fn new(
        initial_bpm: Tempo,
        anchor: HostTimeAnchor,
        config: LinkWriteConfig,
    ) -> Self {
        Self {
            clock: LinkClock::new(initial_bpm, anchor),
            transport: rust_fsm::StateMachine::new(),
            quantum: config.default_quantum,
            config,
        }
    }

    /// Toggle peer discovery + session joining. Delegates to the
    /// underlying `LinkClock`.
    pub fn enable(&self, on: bool) {
        self.clock.enable(on);
    }

    /// Number of peers currently joined.
    pub fn num_peers(&self) -> u64 {
        self.clock.num_peers()
    }

    /// Current tempo. RT-safe (delegates to `LinkClock::tempo`).
    pub fn tempo(&mut self) -> Tempo {
        self.clock.tempo()
    }

    /// Snapshot of the current transport state.
    pub fn transport_state(&self) -> &TransportState {
        self.transport.state()
    }

    /// Whether the FSM currently reports Playing. Convenience wrapper
    /// around `transport_state`.
    pub fn is_playing(&self) -> bool {
        matches!(self.transport.state(), TransportState::Playing)
    }

    /// Drive the FSM with `UserStart`. When `enable_start_stop` is
    /// on and the transition emits `PublishPlaying`, this calls
    /// through to T1's push-path (filled out once T1 lands).
    ///
    /// Control-thread only.
    pub fn user_start(&mut self) {
        self.drive_and_publish(TransportEvent::UserStart);
    }

    /// Drive the FSM with `UserStop`.
    ///
    /// Control-thread only.
    pub fn user_stop(&mut self) {
        self.drive_and_publish(TransportEvent::UserStop);
    }

    /// Poll Link's `is_playing` flag and drive the FSM with the
    /// corresponding `LinkReports*` event. No-op when
    /// `enable_start_stop_sync` is off. Control-thread only.
    pub fn poll_transport(&mut self) {
        if !self.config.enable_start_stop_sync {
            return;
        }
        let playing = self.clock.is_playing_session();
        let ev = if playing {
            TransportEvent::LinkReportsPlaying
        } else {
            TransportEvent::LinkReportsStopped
        };
        self.drive_and_publish(ev);
    }

    fn drive_and_publish(&mut self, ev: TransportEvent) {
        let out = self
            .transport
            .consume(&ev)
            .expect("FSM declares all (state × event) combos");
        // T1 will wire PublishPlaying / PublishStopped through to the
        // LinkClock's set_is_playing path; in T0 this is a placeholder.
        if let Some(o) = out {
            if self.config.enable_start_stop {
                match o {
                    TransportOutput::PublishPlaying => self.clock.publish_is_playing(true),
                    TransportOutput::PublishStopped => self.clock.publish_is_playing(false),
                }
            }
        }
    }
}
