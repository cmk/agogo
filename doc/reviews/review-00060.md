# PR #60 — Split transport playhead module

## Summary

Implements Plan 2026-05-02-05's structural transport-runtime split.

- Move the N-channel transport runtime from `crates/core/src/control.rs`
  into `crates/core/src/control/transport.rs`, leaving `control.rs` as
  the facade for `event`, `sync`, and `transport`.
- Hard-rename `Machine<R>` to `Playhead<R>` and
  `MachineStopHandle` to `PlayheadStopHandle`; keep `TransportPolicy`
  and `TransportState` names, re-exported from `agogo::core::control`.
- Update CLI, host-cpal, core comments/tests, and docs under `doc/` to
  use the new runtime name and current module paths.
- Move the float allowlist entry for the PCM ABI test locals from
  `control.rs` to `control/transport.rs`, with AGENTS prose kept in
  sync.

Verification:

- `cargo fmt --all -- --check`
- `scripts/check-pii.sh`
- `scripts/check-floats.sh`
- `scripts/check-layers.sh`
- `cargo test --workspace --quiet`
- `cargo clippy --all-targets --quiet -- -D warnings`
- Targeted stale-reference scans for old `Machine` API names and paths.

## Local review (2026-05-02)

**Branch:** plan-2026-05-02-05
**Commits:** 3 (origin/main..plan-2026-05-02-05)
**Reviewer:** Codex (`codex review --base origin/main`)

---

The Rust code builds and tests pass, but the documentation sweep corrupts historical migration instructions by changing source paths into current paths. That leaves misleading self-referential plans that should be corrected before merging the docs/reference repair.

Review comment:

- [P2] Preserve source paths in historical migration steps — doc/plans/plan-2026-04-29-01.md:241-242
  Because this section documents the 2026-04-29 migration, replacing the source side turns the task into a no-op (`channel/spec.rs` → `channel/spec.rs`) and similarly makes nearby regression/allowlist bullets self-referential. Future cleanup/audit work using this plan loses the fact that the original source was `machine/spec.rs`; keep the historical source paths intact or rewrite the section as a current-state note instead of a migration plan.

Resolution: restored the historical source-side paths in
`doc/plans/plan-2026-04-29-01.md` and its mirrored PR #48 review
summary, while keeping current API/runtime references on `Playhead`.

<!-- gh-id: 3176400832 -->
### Copilot on [`doc/plans/plan-2026-04-27-02.md:59`](https://github.com/cmk/agogo/pull/60#discussion_r3176400832) (2026-05-02 08:41 UTC)

This affected-files list now names `control/transport.rs` and `channel/spec.rs`, but this plan is dated 2026-04-27, before either later file move happened. Rewriting the list to current paths loses which files Q1b actually touched at the time, which makes the historical audit trail inaccurate.

<!-- gh-id: 3176400841 -->
### Copilot on [`doc/plans/plan-2026-04-24-03.md:113`](https://github.com/cmk/agogo/pull/60#discussion_r3176400841) (2026-05-02 08:41 UTC)

This historical plan now rewrites the source module as `control::transport`, but the unchanged task list immediately below still describes adding `pub mod machine;`. Because Plan 14 predates the 2026-05-02 split, changing the module path here makes the archived plan internally inconsistent and obscures what file the sprint originally introduced.

<!-- gh-id: 3176400845 -->
### Copilot on [`doc/reviews/review-00038.md:1`](https://github.com/cmk/agogo/pull/60#discussion_r3176400845) (2026-05-02 08:41 UTC)

This review record is for PR #38, which happened before the later move under `channel/`. Renaming the reviewed file here from `machine/spec.rs` to `channel/spec.rs` rewrites the historical source path and loses what that PR actually split, so the archived review becomes misleading.

<!-- gh-id: 3176400847 -->
### Copilot on [`doc/plans/plan-2026-04-27-04.md:26`](https://github.com/cmk/agogo/pull/60#discussion_r3176400847) (2026-05-02 08:41 UTC)

These citations no longer point at the code the audit actually inspected: `crates/core/src/channel/spec.rs` is now only a 39-line re-export shell, so lines 74/272/388/572/984 cannot refer to the original `delay_ms` implementation. Please preserve the original source path here instead of rewriting the audit references to the current module layout.


<!-- gh-id: 3176400849 -->
### Copilot on [`doc/plans/plan-2026-04-27-03.md:128`](https://github.com/cmk/agogo/pull/60#discussion_r3176400849) (2026-05-02 08:41 UTC)

This reference is now factually wrong: `crates/core/src/channel/spec.rs` is only 39 lines today, so `:507-512` cannot be the `micro_from_ms` implementation discussed in this historical plan. Rewriting the citation to the current path breaks the audit trail; keep the original source location instead.


<!-- gh-id: 3176400856 -->
### Copilot on [`doc/plans/plan-2026-04-26-03.md:91`](https://github.com/cmk/agogo/pull/60#discussion_r3176400856) (2026-05-02 08:41 UTC)

This re-export citation is no longer valid after the rename/split: `crates/core/src/control/transport.rs:35` is a `use` line, not the `pub use spec::ChannelDev` site this plan is describing. Repointing historical line references at the new module layout makes the cleanup steps inaccurate; the plan should keep the original source location.

<!-- gh-id: 4214750626 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-05-02 08:41 UTC](https://github.com/cmk/agogo/pull/60#pullrequestreview-4214750626))

## Pull request overview

This PR splits the control-layer transport runtime into a dedicated `control::transport` module, hard-renames the runtime from `Machine` to `Playhead`, and updates callers/docs to the new structure while keeping `agogo::core::control` as the public facade.

**Changes:**
- Extract the transport runtime from `crates/core/src/control.rs` into `crates/core/src/control/transport.rs` and re-export the public transport types from the facade.
- Rename runtime-facing symbols from `Machine`/`MachineStopHandle` to `Playhead`/`PlayheadStopHandle` across core, CLI, host-cpal, and comments/tests.
- Sweep the float allowlist and repository docs/reviews/plans for the new runtime/module names and paths.

### Reviewed changes

Copilot reviewed 56 out of 56 changed files in this pull request and generated 7 comments.

<details>
<summary>Show a summary per file</summary>

| File | Description |
| ---- | ----------- |
| scripts/check-floats.sh | Moves the PCM-ABI allowlist entry from `control.rs` to `control/transport.rs`. |
| doc/versions/version-0.4.md | Renames the runtime reference in v0.4 acceptance text to `Playhead`. |
| doc/versions/version-0.2.md | Renames `Machine::on_buffer` references to `Playhead::on_buffer`. |
| doc/versions/version-0.1.md | Renames the hard-time callback reference to `Playhead::on_buffer`. |
| doc/todo.md | Updates backlog/path references from old `machine/spec.rs` naming. |
| doc/reviews/review-00060.md | Adds the new review record for this PR and its local findings. |
| doc/reviews/review-00055.md | Renames the `control.rs` review summary entry to `Playhead`. |
| doc/reviews/review-00048.md | Updates layer-check examples to use `Playhead`. |
| doc/reviews/review-00040.md | Rewrites a historical regression-seed path reference. |
| doc/reviews/review-00038.md | Rewrites the historical PR #38 title/summary paths. |
| doc/reviews/review-00037.md | Updates a deferred backlog item to the newer spec path. |
| doc/reviews/review-00036.md | Updates a deferred backlog item to the newer spec path. |
| doc/reviews/review-00035.md | Updates a deferred backlog item to the newer spec path. |
| doc/reviews/review-00030.md | Renames a test-path reference to `control/transport.rs`. |
| doc/reviews/review-00026.md | Renames historical runtime references to `Playhead`. |
| doc/reviews/review-00023.md | Rewrites a helper-path reference to `control/transport.rs`. |
| doc/reviews/review-00021.md | Renames historical runtime references to `Playhead`. |
| doc/reviews/review-00017.md | Updates historical float-allowlist prose to current paths. |
| doc/reviews/review-00016.md | Renames the historical PR #16 review to `Playhead`. |
| doc/reviews/review-00015.md | Renames historical Plan 14 runtime references to `Playhead`. |
| doc/reviews/review-00014.md | Renames a transport-FSM mapping reference to `Playhead`. |
| doc/reviews/review-00012.md | Renames a future-absorption note to `Playhead`. |
| doc/plans/plan-2026-05-02-05.md | Adds the new plan describing the transport split and rename. |
| doc/plans/plan-2026-04-28-10.md | Renames historical orchestrator references to `Playhead`/`control/transport.rs`. |
| doc/plans/plan-2026-04-28-06.md | Rewrites the historical spec-split plan to newer paths. |
| doc/plans/plan-2026-04-28-05.md | Updates a deferred backlog item to the newer spec path. |
| doc/plans/plan-2026-04-28-04.md | Updates a deferred backlog item to the newer spec path. |
| doc/plans/plan-2026-04-28-03.md | Rewrites multiple historical `machine/spec.rs` references to newer paths. |
| doc/plans/plan-2026-04-27-04.md | Rewrites audit citations for `ChannelSpec.delay_ms` to newer paths. |
| doc/plans/plan-2026-04-27-03.md | Rewrites a historical `micro_from_ms` citation to newer paths. |
| doc/plans/plan-2026-04-27-02.md | Updates the affected-files list to newer transport/spec paths. |
| doc/plans/plan-2026-04-27-01.md | Renames a generic call-site reference from `Machine` to `Playhead`. |
| doc/plans/plan-2026-04-26-03.md | Rewrites historical `ChannelDev` re-export paths to newer transport paths. |
| doc/plans/plan-2026-04-26-02.md | Renames historical dispatch references from `Machine` to `Playhead`. |
| doc/plans/plan-2026-04-26-01.md | Rewrites historical affected-file references to newer transport/spec paths. |
| doc/plans/plan-2026-04-25-05.md | Rewrites historical parser/helper path references to newer paths. |
| doc/plans/plan-2026-04-25-03.md | Renames historical runtime references to `Playhead` and `control/transport.rs`. |
| doc/plans/plan-2026-04-25-02.md | Rewrites a historical parser path reference to `channel/spec.rs`. |
| doc/plans/plan-2026-04-24-03.md | Rewrites the historical Plan 14 module/runtime paths to newer names. |
| doc/plans/plan-2026-04-24-02.md | Renames historical Plan 14 references from `Machine` to `Playhead`. |
| doc/plans/plan-2026-04-24-01.md | Renames historical Plan 14 callback references to `Playhead`. |
| doc/plans/plan-2026-04-23-06.md | Renames historical future transport references to `Playhead`. |
| doc/plans/plan-2026-04-23-01.md | Rewrites a historical transport-stub path reference to `control/transport.rs`. |
| crates/host-link/src/session.rs | Updates the doc comment that says the runtime later absorbs `LinkSession`. |
| crates/host-cpal/src/cpal/callback.rs | Renames the callback wrapper to hold `Playhead` and updates tests/docs accordingly. |
| crates/core/src/sink/midi.rs | Updates typed-renderer docs and panic text to refer to `Playhead`. |
| crates/core/src/sink.rs | Updates sink module docs to say audio I/O is consumed by `Playhead`. |
| crates/core/src/control/transport.rs | New home for the extracted transport runtime, policy/state, stop handle, and tests. |
| crates/core/src/control/event.rs | Updates scheduler docs to refer to `Playhead`. |
| crates/core/src/control.rs | Shrinks `control.rs` into a facade that exports `event`, `sync`, and `transport`. |
| crates/core/src/channel/role.rs | Updates `bar_multiplier` docs to say filtering happens in `Playhead::on_buffer`. |
| crates/cli/src/run.rs | Renames CLI runtime construction and Ctrl-C plumbing to `Playhead`. |
| crates/cli/src/demo.rs | Renames demo runtime construction/comments to `Playhead`. |
| crates/cli/src/command.rs | Renames the `run` subcommand help text to `Playhead`. |
| crates/cli/Cargo.toml | Renames the `run` feature comment to `Playhead`. |
| AGENTS.md | Updates float-allowlist prose for the moved PCM-ABI test locals. |
</details>



<details>
<summary>Comments suppressed due to low confidence (1)</summary>

**doc/plans/plan-2026-04-28-06.md:5**
* This plan predates the later move from `machine/spec.rs` to `channel/spec.rs`, so rewriting the title and opening description to the current path changes the historical migration target. That makes the archived sprint misleading for future audits and grep-based cleanup work; the source-side paths in this plan should stay as they were at the time.
</details>



<!-- gh-id: 3176400867 -->
### Copilot on [`doc/reviews/review-00040.md:40`](https://github.com/cmk/agogo/pull/60#discussion_r3176400867) (2026-05-02 08:41 UTC)

This review predates the later move from `machine/spec/display.txt` to `channel/spec/display.txt`, so rewriting the regression-seed path here changes the historical file location that PR #40 actually added. Keeping the current path makes the archived review misleading for anyone trying to trace when the seed moved.

<!-- gh-id: 3176430340 -->
#### ↳ cmk ([2026-05-02 09:09 UTC](https://github.com/cmk/agogo/pull/60#discussion_r3176430340))

Restored the affected-files list to the historical machine.rs and machine/spec.rs entries while keeping the Playhead naming update.

<!-- gh-id: 3176434976 -->
#### ↳ cmk ([2026-05-02 09:14 UTC](https://github.com/cmk/agogo/pull/60#discussion_r3176434976))

Restored Plan 14's historical source module and file paths to agogo_core::machine / machine.rs / machine/spec.rs, while leaving the runtime identity updated to Playhead.

<!-- gh-id: 3176435296 -->
#### ↳ cmk ([2026-05-02 09:14 UTC](https://github.com/cmk/agogo/pull/60#discussion_r3176435296))

Restored the PR #38 review title and related summary paths to machine/spec.rs so the archived review keeps the source path that PR actually split.

<!-- gh-id: 3176435438 -->
#### ↳ cmk ([2026-05-02 09:15 UTC](https://github.com/cmk/agogo/pull/60#discussion_r3176435438))

Restored those audit citations to the historical machine/spec.rs line references, including the T3 source-file reference later in the plan.

<!-- gh-id: 3176435698 -->
#### ↳ cmk ([2026-05-02 09:15 UTC](https://github.com/cmk/agogo/pull/60#discussion_r3176435698))

Restored this micro_from_ms citation to the historical machine/spec.rs location, and matched the later grep-scope reference to that same historical file.

<!-- gh-id: 3176436060 -->
#### ↳ cmk ([2026-05-02 09:15 UTC](https://github.com/cmk/agogo/pull/60#discussion_r3176436060))

Restored the ChannelDev re-export discussion to the original agogo_core::machine / machine.rs / machine/spec.rs locations for this archived cleanup plan.

<!-- gh-id: 3176436274 -->
#### ↳ cmk ([2026-05-02 09:15 UTC](https://github.com/cmk/agogo/pull/60#discussion_r3176436274))

Restored the PR #40 regression-seed references to proptest-regressions/machine/spec/display.txt so the review reflects the file path that existed when that PR landed.
