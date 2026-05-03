# PR #65 - Apply admitted commands to Playhead

## Summary

Applies the v0.2 command admission envelope to runtime `Playhead`
state without adding a direct stdio-core dependency.

- Adds RT-safe `Playhead` hooks for admitted tempo changes and
  command-driven start/stop.
- Adds `agogo_host::apply_control_to_playhead`, which advances the RT
  bridge epoch, applies scalar tempo, drains ordered command envelopes,
  and reports applied, missed-deadline, and unsupported commands.
- Keeps `agogo.tempo.set`, `agogo.start`, and `agogo.stop` as admitted
  runtime commands, while rejecting `agogo.locate` and
  `agogo.channel.configure` with `unsupported_command_class` until
  their runtime semantics are designed.
- Adds host bridge and driver tests for tempo/start/stop application,
  missed-deadline reporting, unsupported command handling, and
  command application without any snapshot reader.

Verification:

- `cargo fmt --all -- --check`
- `scripts/check-pii.sh`
- `scripts/check-floats.sh`
- `scripts/check-layers.sh`
- `cargo test -p agogo-host --quiet`
- `cargo test --workspace --quiet`
- `cargo clippy --all-targets --quiet -- -D warnings`
- `cargo test -p agogo-cli --no-default-features --no-run`

## Local review (2026-05-03)

**Branch:** plan-2026-05-03-01
**Commits:** 3 (origin/main..plan-2026-05-03-01)
**Reviewer:** Codex (`codex review --base origin/main`)

---

The new command-start path can cancel an outstanding teardown stop request by clearing the shared stop flag, violating the existing Playhead stop contract in mixed command/teardown scenarios.

Review comment:

- [P2] Keep command start from clearing teardown stops - crates/core/src/control/transport.rs:307-309
  When an embedder uses the existing `PlayheadStopHandle` for teardown, a queued `Start` processed after `request_stop()` but before `on_buffer()` clears the same `stop_flag`. That prevents the next buffer from observing the teardown stop request, so the documented final Stop/silence contract can be lost and clock output can continue; command pause/resume state should not share the teardown latch.

## Local review (2026-05-03)

**Branch:** plan-2026-05-03-01
**Commits:** 4 (origin/main..plan-2026-05-03-01)
**Reviewer:** Codex (`codex review --base origin/main`)

---

The patch adds command application, but multiple ordered transport commands due in the same buffer can be silently collapsed because only one staged transport byte is retained before rendering. This breaks the ordered-command contract for realistic back-to-back start/stop calls.

Review comment:

- [P2] Preserve FIFO transport commands - crates/host/src/bridge.rs:988-994
  When a caller admits multiple ordered transport commands before the next RT buffer (for example, `agogo.start` followed by `agogo.stop`, both defaulting to `deadline_buffer=1`), this loop drains both before `Playhead::on_buffer` runs. Each `apply_transport_*` writes the single staged `command_transport` slot, so the later command overwrites the earlier one while `applied_commands` counts both; the first ordered command never emits its MIDI byte or affects the intermediate running state, violating the FIFO/no-silent-loss contract for ordered SPSC commands.

## Local review (2026-05-03)

**Branch:** plan-2026-05-03-01
**Commits:** 5 (origin/main..plan-2026-05-03-01)
**Reviewer:** Codex (`codex review --base origin/main`)

---

The new command transport path works for the internal policy but can wedge or desynchronize link-driven transport because it bypasses the policy state machine while mutating the shared running latch.

Review comment:

- [P2] Reject command transport outside internal policy - crates/core/src/control/transport.rs:402-407
  When the host applicator is used with a `LinkDriven` playhead (the policy used for link-sourced runs), these command paths mutate the shared `running` latch without updating the link policy state. A queued `Stop` sets `running = false`, after which `TransportState::next_byte` returns before polling the link query, so future Link play transitions can never resume clocking; conversely a queued `Start` can emit a MIDI Start unrelated to Link state. Either reject command-driven start/stop for non-`Internal` policies or keep the policy state synchronized.

## Local review (2026-05-03)

**Branch:** plan-2026-05-03-01
**Commits:** 6 (origin/main..plan-2026-05-03-01)
**Reviewer:** Codex (`codex review --base origin/main`)

---

The implementation compiles and tests pass locally, but the branch declares an RT no-reallocation property as required and does not implement or defer that verification. That leaves a repo-required shipping gate unmet.

Review comment:

- [P2] Add the no-reallocation property test - doc/plans/plan-2026-05-03-01.md:130-130
  For this sprint, rows under `Properties (must pass)` are required verification before shipping, but there is no `rt_command_application_no_realloc` or equivalent allocation check in the host tests. Because this is the RT-side apply path, future changes could grow command-application storage without any gate catching it; either add the no-reallocation test or move/defer this row in the plan's Review section.

## Local review (2026-05-03)

**Branch:** plan-2026-05-03-01
**Commits:** 7 (origin/main..plan-2026-05-03-01)
**Reviewer:** Codex (`codex review --base origin/main`)

---

No actionable correctness issues were found in the diff. The added host bridge application path and transport hooks are covered by targeted tests, and the workspace tests pass locally.
