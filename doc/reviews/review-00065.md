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

<!-- gh-id: 3177849670 -->
### Copilot on [`crates/host/src/bridge.rs:999`](https://github.com/cmk/agogo/pull/65#discussion_r3177849670) (2026-05-03 08:33 UTC)

`apply_control_to_playhead` treats any `playhead.apply_transport_{start,stop}()` `false` as `unsupported_commands`, but those methods also return `false` when the Playhead’s fixed-capacity command transport queue is full. That would misclassify a supported command as “unsupported” (and can drop admitted commands under load if the bridge queue capacity exceeds the Playhead queue capacity). Consider returning a richer result from the Playhead apply methods (e.g., unsupported-policy vs queue-full/overflow vs teardown-stop-pending) and add a separate counter/fault in `CommandApplyReport` so overflow is reported accurately and doesn’t look like an unsupported command class.


<!-- gh-id: 3177849678 -->
### Copilot on [`crates/core/src/control/transport.rs:375`](https://github.com/cmk/agogo/pull/65#discussion_r3177849678) (2026-05-03 08:33 UTC)

`apply_transport_start` / `apply_transport_stop` can return `true` (command staged) even when a teardown stop has already been requested via `stop_flag`. In that state, `on_buffer` always takes the `stop_pending` branch and clears `command_transport`, so any staged Start/Stop is guaranteed to be dropped. Consider checking `stop_flag` inside these methods and rejecting staging once teardown has been requested (ideally with a distinct error/result so the host can report a fault rather than counting the command as applied).

<!-- gh-id: 3177849681 -->
### Copilot on [`crates/core/src/control/transport.rs:415`](https://github.com/cmk/agogo/pull/65#discussion_r3177849681) (2026-05-03 08:33 UTC)

The command-transport handling mutates `self.transport.running` in response to command Start/Stop (e.g., setting it `false` on command Stop and `true` on command Start). However, `TransportState::running` is documented earlier in this file as a teardown latch that flips to `false` only when a `PlayheadStopHandle::request_stop` is observed, and that policy-driven Stop bytes do not clear it. Please update the documentation and/or split this into separate fields (teardown latch vs command-controlled running) so future changes don’t accidentally break the teardown-silence contract.

<!-- gh-id: 4216078913 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-05-03 08:33 UTC](https://github.com/cmk/agogo/pull/65#pullrequestreview-4216078913))

## Pull request overview

Applies the v0.2 command-admission envelope to runtime `Playhead` state, enabling admitted tempo and transport commands to take effect at RT buffer boundaries without introducing a direct stdio-core dependency.

**Changes:**
- Added RT-safe `Playhead` APIs for applying tempo changes and command-driven start/stop at buffer boundaries.
- Introduced `agogo_host::apply_control_to_playhead` to advance the RT epoch, apply tempo, drain ordered commands, and return an application report.
- Updated driver behavior and tests to reject unsupported ordered command classes (`locate`, `channel.configure`) before queue admission.

### Reviewed changes

Copilot reviewed 6 out of 6 changed files in this pull request and generated 3 comments.

<details>
<summary>Show a summary per file</summary>

| File | Description |
| ---- | ----------- |
| doc/reviews/review-00065.md | Adds the PR review record and local-review findings/fixes summary. |
| doc/plans/plan-2026-05-03-01.md | Adds the sprint plan, verification properties, and review notes for the implementation. |
| crates/host/src/lib.rs | Re-exports the new host bridge applicator and reporting types. |
| crates/host/src/driver.rs | Rejects unsupported ordered tools before queue admission; updates tests accordingly. |
| crates/host/src/bridge.rs | Adds `CommandApplyReport` and `apply_control_to_playhead`; adds targeted bridge/applicator tests. |
| crates/core/src/control/transport.rs | Adds command-transport staging FIFO and `Playhead` hooks for tempo/start/stop; updates callback transport emission logic and tests. |
</details>

<!-- gh-id: 3177867120 -->
#### ↳ cmk ([2026-05-03 08:47 UTC](https://github.com/cmk/agogo/pull/65#discussion_r3177867120))

Fixed in the review-round commit by replacing the boolean Playhead transport apply result with TransportCommandApply and recording queue-full separately from unsupported policy in CommandApplyReport. Added transport_queue_full_reports_separately to cover the fixed-capacity FIFO case.

<!-- gh-id: 3177868526 -->
#### ↳ cmk ([2026-05-03 08:48 UTC](https://github.com/cmk/agogo/pull/65#discussion_r3177868526))

Fixed in the review-round commit by checking the teardown stop flag inside apply_transport_start/apply_transport_stop and returning TransportCommandApply::TeardownRequested instead of staging a command that on_buffer would drop. Added teardown_requested_reports_without_staging_command for the host-side report behavior.

<!-- gh-id: 3177868556 -->
#### ↳ cmk ([2026-05-03 08:48 UTC](https://github.com/cmk/agogo/pull/65#discussion_r3177868556))

Fixed in the review-round commit by updating the TransportState::running documentation to describe it as the local clock gate rather than a teardown-only latch, including the stronger teardown behavior that rejects further command staging.

<!-- gh-id: 3177872019 -->
### Copilot on [`crates/core/src/control/transport.rs:366`](https://github.com/cmk/agogo/pull/65#discussion_r3177872019) (2026-05-03 08:52 UTC)

The docstring implies the next `on_buffer` will emit Start/resume clocking, but this method can return `UnsupportedPolicy`, `QueueFull`, or `TeardownRequested` (and even after `Applied`, a teardown stop could be requested before `on_buffer`, causing the staged command to be cleared). Please qualify the guarantee to the `Applied` case (and/or document the teardown race) so callers don’t treat any successful call as “will definitely start next buffer.”


<!-- gh-id: 3177872026 -->
### Copilot on [`crates/core/src/control/transport.rs:388`](https://github.com/cmk/agogo/pull/65#discussion_r3177872026) (2026-05-03 08:52 UTC)

Similar to `apply_transport_start`: this docstring states the next `on_buffer` call emits Stop and suppresses clock output, but the method can return non-`Applied` results and even an `Applied` Stop may not emit a byte if teardown is requested before `on_buffer` runs. Clarifying the exact semantics (what is guaranteed for each `TransportCommandApply` variant) would make the API contract unambiguous.

<!-- gh-id: 4216101887 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-05-03 08:52 UTC](https://github.com/cmk/agogo/pull/65#pullrequestreview-4216101887))

## Pull request overview

Copilot reviewed 7 out of 7 changed files in this pull request and generated 2 comments.





<!-- gh-id: 4216104133 -->
### chatgpt-codex-connector[bot] — COMMENTED ([2026-05-03 08:55 UTC](https://github.com/cmk/agogo/pull/65#pullrequestreview-4216104133))


### 💡 Codex Review

Here are some automated review suggestions for this pull request.

**Reviewed commit:** `a12de3c7f9`


<details> <summary>ℹ️ About Codex in GitHub</summary>
<br/>

[Your team has set up Codex to review pull requests in this repo](https://chatgpt.com/codex/cloud/settings/general). Reviews are triggered when you
- Open a pull request for review
- Mark a draft as ready
- Comment "@codex review".

If Codex has suggestions, it will comment; otherwise it will react with 👍.




Codex can also answer questions or update the PR. Try commenting "@codex address that feedback".

</details>

<!-- gh-id: 3177874949 -->
### chatgpt-codex-connector[bot] on [`crates/host/src/bridge.rs:987`](https://github.com/cmk/agogo/pull/65#discussion_r3177874949) (2026-05-03 08:55 UTC)

**<sub><sub>![P1 Badge](https://img.shields.io/badge/P1-orange?style=flat)</sub></sub>  Defer draining commands until their deadline buffer**

`apply_control_to_playhead` now drains and applies every queued ordered command each buffer, but `drain_due_command` only marks commands as missed when `deadline < current_epoch`; commands with `deadline > current_epoch` are still returned as `Command` and get applied immediately. This means a caller that sets `deadline_buffer` several buffers ahead (for example, `agogo.start` with `deadline_buffer: 10`) will execute on the next callback instead of at/before its intended buffer boundary, violating the deadline contract for scheduled commands.

Useful? React with 👍 / 👎.

<!-- gh-id: 3177878531 -->
#### ↳ cmk ([2026-05-03 08:58 UTC](https://github.com/cmk/agogo/pull/65#discussion_r3177878531))

Fixed by qualifying apply_transport_start to the TransportCommandApply::Applied case and documenting that teardown requested before on_buffer wins and clears staged commands. Non-Applied results now explicitly mean no command was staged.

<!-- gh-id: 3177878561 -->
#### ↳ cmk ([2026-05-03 08:58 UTC](https://github.com/cmk/agogo/pull/65#discussion_r3177878561))

Fixed by retaining a future-deadline FIFO head inside ControlConsumer instead of returning it as due. drain_due_command now returns None for commands whose RT-buffer deadline is still ahead, then reports the command once the buffer epoch reaches the declared deadline or missed-deadline if it is observed late. Added future_deadline_waits_until_declared_buffer.

<!-- gh-id: 3177878569 -->
#### ↳ cmk ([2026-05-03 08:58 UTC](https://github.com/cmk/agogo/pull/65#discussion_r3177878569))

Fixed by qualifying apply_transport_stop to the TransportCommandApply::Applied case, documenting that other variants stage nothing, and calling out that teardown before rendering clears staged commands.
