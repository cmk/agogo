//! Transport FSM — minimal `{Stopped, Playing}` shim around Link's
//! `is_playing` flag with a one-shot user-publish gate.
//!
//! Declared via `rust-fsm 0.7`'s macro DSL so v0.5 Sprint 01's NEG/POS
//! one-bar forerun work can **extend** this declaration (adding
//! `PreRoll` + a forerun-aware stop state) rather than rewriting a
//! hand-rolled `match`.
//!
//! ### Events
//!
//! - `LinkReportsPlaying` / `LinkReportsStopped` — driven by a
//!   periodic `poll_transport` in `LinkSession`. These mirror Link's
//!   network-shared transport into agogo's local state. They never
//!   publish — otherwise two agogo peers would loop their publishes
//!   against each other (the `fsm_no_echo_loop` invariant below).
//! - `UserStart` / `UserStop` — driven by `LinkSession::user_start` /
//!   `user_stop`, surfaced via `agogo link transport --start /
//!   --stop-on-exit`. These publish one-shot to Link via
//!   `set_is_playing`.
//!
//! ### Self-loops
//!
//! All eight (state × event) combinations are declared explicitly so
//! `consume` never returns `TransitionImpossibleError`. Self-loop
//! transitions (e.g. `Stopped + LinkReportsStopped → Stopped`) emit
//! no output — the caller sees a state change to the same state,
//! which is a no-op.

use rust_fsm::state_machine;

state_machine! {
    #[derive(Debug, Clone, PartialEq, Eq)]
    transport_fsm(Stopped)

    Stopped => {
        LinkReportsPlaying => Playing,
        UserStart => Playing [PublishPlaying],
        LinkReportsStopped => Stopped,
        UserStop => Stopped,
    },
    Playing => {
        LinkReportsStopped => Stopped,
        UserStop => Stopped [PublishStopped],
        LinkReportsPlaying => Playing,
        UserStart => Playing,
    },
}

// Re-export the generated types under stable public names so
// downstream code doesn't have to touch the `transport_fsm::` path
// (which will gain more states in v0.5 Sprint 01).

/// The two transport states Plan 09 ships. Extended with `PreRoll`
/// (and possibly a forerun-aware stop-pending state) in v0.5
/// Sprint 01.
pub type TransportState = transport_fsm::State;

/// The four input events the FSM accepts: two Link-originated mirror
/// events + two user-originated one-shot publish events.
pub type TransportEvent = transport_fsm::Input;

/// The publish-instruction output. `PublishPlaying` /
/// `PublishStopped` fire only on `User*` events; `LinkReports*`
/// events return `None`.
pub type TransportOutput = transport_fsm::Output;

/// The generated state machine, wrapped for naming clarity.
pub type TransportFsm = transport_fsm::StateMachine;

#[cfg(test)]
mod tests {
    use super::*;
    use rust_fsm::StateMachine;

    /// Helper: drive the FSM through a sequence of events and return
    /// `(final_state, outputs, num_publishes)`.
    fn run(events: &[TransportEvent]) -> (TransportState, Vec<Option<TransportOutput>>) {
        let mut m: TransportFsm = StateMachine::new();
        let mut outs = Vec::with_capacity(events.len());
        for ev in events {
            let o = m.consume(ev).expect(
                "all (state × event) combos are declared; consume must never \
                 return TransitionImpossibleError",
            );
            outs.push(o);
        }
        (m.state().clone(), outs)
    }

    // ── Spot checks ──────────────────────────────────────────────

    #[test]
    fn initial_state_is_stopped() {
        let m: TransportFsm = StateMachine::new();
        assert!(matches!(m.state(), TransportState::Stopped));
    }

    #[test]
    fn user_start_from_stopped_publishes_playing() {
        let (state, outs) = run(&[TransportEvent::UserStart]);
        assert!(matches!(state, TransportState::Playing));
        assert!(matches!(outs[0], Some(TransportOutput::PublishPlaying)));
    }

    #[test]
    fn user_stop_from_playing_publishes_stopped() {
        let (state, outs) = run(&[TransportEvent::UserStart, TransportEvent::UserStop]);
        assert!(matches!(state, TransportState::Stopped));
        assert!(matches!(outs[1], Some(TransportOutput::PublishStopped)));
    }

    #[test]
    fn link_reports_never_publish() {
        let (state, outs) = run(&[
            TransportEvent::LinkReportsPlaying,
            TransportEvent::LinkReportsStopped,
        ]);
        assert!(matches!(state, TransportState::Stopped));
        assert!(outs.iter().all(|o| o.is_none()));
    }

    #[test]
    fn user_start_then_link_reports_stopped_ends_stopped_with_one_publish() {
        // Plan 09 spot check: `Stopped + UserStart + LinkReportsStopped`
        // ends Stopped; the FSM publishes exactly one
        // `set_is_playing(true, _)` and zero `set_is_playing(false, _)`
        // (LinkReportsStopped doesn't publish).
        let (state, outs) = run(&[
            TransportEvent::UserStart,
            TransportEvent::LinkReportsStopped,
        ]);
        assert!(matches!(state, TransportState::Stopped));
        let play = outs
            .iter()
            .filter(|o| matches!(o, Some(TransportOutput::PublishPlaying)))
            .count();
        let stop = outs
            .iter()
            .filter(|o| matches!(o, Some(TransportOutput::PublishStopped)))
            .count();
        assert_eq!(play, 1);
        assert_eq!(stop, 0);
    }

    #[test]
    fn self_loops_are_noops() {
        // Stopped + LinkReportsStopped → Stopped (no output).
        // Playing + LinkReportsPlaying → Playing (no output).
        // Stopped + UserStop → Stopped (no output).
        // Playing + UserStart → Playing (no output).
        let cases: &[(&[TransportEvent], TransportState)] = &[
            (
                &[TransportEvent::LinkReportsStopped],
                TransportState::Stopped,
            ),
            (&[TransportEvent::UserStop], TransportState::Stopped),
            (
                &[
                    TransportEvent::UserStart,
                    TransportEvent::LinkReportsPlaying,
                ],
                TransportState::Playing,
            ),
            (
                &[TransportEvent::UserStart, TransportEvent::UserStart],
                TransportState::Playing,
            ),
        ];
        for (events, expected) in cases {
            let (state, _) = run(events);
            assert!(
                std::mem::discriminant(&state) == std::mem::discriminant(expected),
                "events {events:?} left FSM in {state:?}, expected {expected:?}"
            );
        }
    }

    // ── Properties ───────────────────────────────────────────────

    use proptest::prelude::*;

    fn arb_event() -> impl Strategy<Value = TransportEvent> {
        prop_oneof![
            Just(TransportEvent::LinkReportsPlaying),
            Just(TransportEvent::LinkReportsStopped),
            Just(TransportEvent::UserStart),
            Just(TransportEvent::UserStop),
        ]
    }

    fn is_user_event(e: &TransportEvent) -> bool {
        matches!(e, TransportEvent::UserStart | TransportEvent::UserStop)
    }

    fn is_publish(o: &Option<TransportOutput>) -> bool {
        matches!(
            o,
            Some(TransportOutput::PublishPlaying) | Some(TransportOutput::PublishStopped)
        )
    }

    proptest! {
        /// `transport_fsm_deterministic` — the FSM state + emitted
        /// publishes are a pure function of the event sequence. Two
        /// parallel runs over the same sequence must agree on every
        /// step.
        #[test]
        fn transport_fsm_deterministic(events in proptest::collection::vec(arb_event(), 0..64)) {
            let (s1, o1) = run(&events);
            let (s2, o2) = run(&events);
            prop_assert_eq!(
                std::mem::discriminant(&s1),
                std::mem::discriminant(&s2),
                "state diverged on identical input"
            );
            prop_assert_eq!(o1.len(), o2.len());
            for (a, b) in o1.iter().zip(o2.iter()) {
                prop_assert_eq!(
                    std::mem::discriminant(a),
                    std::mem::discriminant(b),
                    "output diverged on identical input"
                );
            }
        }

        /// `transport_fsm_no_spurious_publishes` — every publish in
        /// the output stream is paired one-to-one with a `User*`
        /// event in the input stream. The count of publishes must
        /// equal the count of `User*` events that left the FSM in a
        /// state opposite to its starting state for that transition.
        ///
        /// Simpler: since Stopped+UserStart publishes and Playing+UserStop
        /// publishes, and the other six (state × event) combos don't,
        /// the total publish count equals the count of "productive"
        /// User events — no `LinkReports*` event ever produces a
        /// publish.
        #[test]
        fn transport_fsm_no_spurious_publishes(
            events in proptest::collection::vec(arb_event(), 0..64),
        ) {
            let (_, outs) = run(&events);
            for (ev, out) in events.iter().zip(outs.iter()) {
                if is_publish(out) {
                    prop_assert!(
                        is_user_event(ev),
                        "publish {:?} was triggered by non-user event {:?}",
                        out, ev
                    );
                }
            }
        }

        /// `fsm_no_echo_loop` — a sequence containing only
        /// `LinkReports*` events must never emit a publish. This is
        /// the critical invariant preventing two agogo peers from
        /// looping publishes against each other over the network.
        #[test]
        fn fsm_no_echo_loop(n_events in 0usize..64) {
            let events: Vec<TransportEvent> = (0..n_events)
                .map(|i| if i % 2 == 0 {
                    TransportEvent::LinkReportsPlaying
                } else {
                    TransportEvent::LinkReportsStopped
                })
                .collect();
            let (_, outs) = run(&events);
            prop_assert!(
                outs.iter().all(|o| o.is_none()),
                "LinkReports-only sequence emitted a publish: {:?}",
                outs,
            );
        }
    }
}
