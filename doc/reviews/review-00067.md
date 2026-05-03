# PR #67 — Remove SampleTickConn and boundary panics

## Summary

Replaces the conn-shaped runtime `SampleTickConn` bridge with fixed-PPQN,
tempo-aware sample-rate dispatch. Musical scheduling now computes
`Tick + Tempo -> S044/S048/...` directly and then uses the existing
`SxxxI064` static whole-sample conns. Decimal `Micro` / `Pico`
conversions remain only at SI-duration boundaries such as delay and
offset.

Also adds a boundary-panic discipline: user-reachable invalid values must
be rejected at CLI/config/host-command boundaries rather than by bridge
or scheduler panics. The new `scripts/check-boundary-panics.sh` gate is
wired into pre-commit, host tempo commands now reject zero BPM, and
unsupported sample-rate dispatch returns `None` instead of panicking.

Validation:

- `cargo fmt --all -- --check`
- `scripts/check-pii.sh`
- `scripts/check-floats.sh`
- `scripts/check-layers.sh`
- `scripts/check-connections.sh`
- `scripts/check-boundary-panics.sh`
- `cargo test --workspace --quiet`
- `cargo clippy --all-targets --quiet -- -D warnings`

## Local review (2026-05-03)

**Branch:** plan-2026-05-03-03
**Commits:** 3 (origin/main..plan-2026-05-03-03)
**Reviewer:** Codex (`codex review --base origin/main`)

---

I did not find any discrete, actionable regressions in the diff. The workspace test suite and the new boundary-panic check pass locally.

Post-push review superseded that conclusion: Copilot found invalid-parameter
APIs that failed silently and a high-tempo scheduler-window regression. The
round below records those findings; the follow-up commit changes the public
scheduling APIs to return errors/options and rejects tempos above the global
`T512P`-at-sample-rate envelope instead of trying to coalesce same-sample
events.


<!-- gh-id: 3178745377 -->
### Copilot on [`crates/core/src/channel/time.rs:124`](https://github.com/cmk/agogo/pull/67#discussion_r3178745377) (2026-05-03 20:46 UTC)

`transform` is also public, but these `None => Vec::new()` branches make an unsupported `sr` or zero tempo look exactly like "no events were produced". Any downstream crate that calls `channel::time::transform` directly will now get silent mis-scheduling instead of a boundary error it can surface to the user.

<!-- gh-id: 3178745389 -->
### Copilot on [`crates/core/src/time/conn.rs:254`](https://github.com/cmk/agogo/pull/67#discussion_r3178745389) (2026-05-03 20:46 UTC)

These new public `tick_to_sxxx` helpers treat `Tempo::ZERO` as "saturate to `i64::MAX`" instead of rejecting it, even though the companion `tick_to_whole_samples` / `sample_to_tick_*` APIs return `None` for the same invalid tempo. Because `Tempo::ZERO` is constructible, callers that forget to pre-validate will get a bogus far-future timestamp rather than a detectable error.


<!-- gh-id: 3178745397 -->
### Copilot on [`doc/reviews/review-00067.md:37`](https://github.com/cmk/agogo/pull/67#discussion_r3178745397) (2026-05-03 20:46 UTC)

This conclusion is no longer accurate: the new scheduler windowing drops some same-sample ticks at the top end of the supported tempo range, so the diff still has at least one actionable regression. Leaving the review record in its current state will mislead later readers about the PR's actual status.


<!-- gh-id: 3178745403 -->
### Copilot on [`crates/core/src/control/event.rs:122`](https://github.com/cmk/agogo/pull/67#discussion_r3178745403) (2026-05-03 20:46 UTC)

Because `tick_to_whole_samples` now rounds each event up to the next whole sample, `sample_to_tick_floor(swung_lo, ...)` is no longer a conservative lower bound here. At supported high tempos multiple ticks can quantize to the same sample (for example 44.1 kHz / 4294 BPM / `Grid::T512P`), and a buffer that starts on that sample will skip the earlier tick(s) because `floor(sample)` points at the last exact-time tick instead of the first tick whose emitted sample is `>= start`.

<!-- gh-id: 4216816634 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-05-03 20:46 UTC](https://github.com/cmk/agogo/pull/67#pullrequestreview-4216816634))

## Pull request overview

This PR removes the runtime `SampleTickConn` bridge from the scheduling path and replaces it with fixed-PPQN, tempo-aware sample-rate dispatch, while also introducing a repository rule that user-reachable invalid values must be rejected at boundaries instead of by bridge/scheduler panics.

**Changes:**
- Reworked tick/sample scheduling to use rate-specific helpers and static whole-sample connections instead of `SampleTickConn`.
- Hardened boundary handling by rejecting zero BPM at the host-command boundary and adding a panic-audit script to the pre-commit checks.
- Updated CLI/runtime call sites and project docs to reflect the new scheduling model and boundary-panic discipline.

### Reviewed changes

Copilot reviewed 22 out of 22 changed files in this pull request and generated 5 comments.

<details>
<summary>Show a summary per file</summary>

| File | Description |
| ---- | ----------- |
| scripts/check-boundary-panics.sh | Adds the panic-audit script for selected bridge/scheduler modules. |
| doc/todo.md | Updates the historical TODO entry for the removed runtime shim. |
| doc/reviews/review-00067.md | Adds the PR review record for this change set. |
| doc/plans/plan-2026-05-03-03.md | Captures the implementation plan and verification targets for the refactor. |
| doc/designs/integer-math.md | Updates design prose to describe direct tick-to-sample conversion. |
| doc/agogo.md | Revises architecture docs to remove the old `SampleTickConn` workaround. |
| crates/host/src/driver.rs | Rejects zero BPM in host commands and adjusts command-id handling/annotations. |
| crates/host/src/bridge.rs | Adds boundary-panic annotations and updates tests for the new playhead shape. |
| crates/host-link/src/session.rs | Refreshes session docs to describe the new conversion boundaries. |
| crates/host-cpal/src/cpal/callback.rs | Updates callback tests for the simplified `Playhead::new` signature. |
| crates/core/src/time/conn.rs | Replaces `SampleTickConn` with rate-specific tick/sample helpers and runtime dispatch. |
| crates/core/src/time.rs | Updates module docs to describe fixed-PPQN sample scheduling. |
| crates/core/src/sink/midi.rs | Adjusts MIDI tests to call the new scheduler API. |
| crates/core/src/control/transport.rs | Removes stored `SampleTickConn` state from `Playhead` and threads `sr`/`bpm` directly. |
| crates/core/src/control/event.rs | Rewrites scheduler windowing to use the new tick/sample helpers. |
| crates/core/src/channel/time.rs | Rewrites the transform pipeline to use direct tick-to-whole-sample dispatch. |
| crates/cli/src/trace/midi.rs | Drops `SampleTickConn` construction in MIDI trace mode. |
| crates/cli/src/trace/channel.rs | Drops `SampleTickConn` construction in channel trace mode. |
| crates/cli/src/run.rs | Updates runtime setup to the new `Playhead::new` API. |
| crates/cli/src/demo.rs | Updates demo setup to the new `Playhead::new` API. |
| AGENTS.md | Documents the new boundary-validation / panic-annotation rule. |
| .githooks/pre-commit | Wires the new boundary-panic audit into the pre-commit hook. |
</details>






<!-- gh-id: 3178745410 -->
### Copilot on [`crates/core/src/control/event.rs:98`](https://github.com/cmk/agogo/pull/67#discussion_r3178745410) (2026-05-03 20:46 UTC)

`tick_stream` is a public API, but this early return now turns an unsupported sample rate or `Tempo::ZERO` into an ordinary empty schedule. That makes "invalid scheduling parameters" indistinguishable from "this buffer legitimately has no events", so downstream callers lose the precise boundary error this PR is trying to enforce and will just fail silently.
