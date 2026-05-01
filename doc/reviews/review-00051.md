# PR #51 — [codex] clean up cli module layout

## Summary

- split the CLI entrypoint into command dispatch and parser modules
- move trace, time, and link handlers under grouped module directories
- fix CLI feature dependencies so no-default-features combinations compile
- update the float allowlist, cleanup plan, and todo tracking docs

## Validation

- pre-commit: cargo fmt --all -- --check
- pre-commit: scripts/check-pii.sh
- pre-commit: scripts/check-floats.sh
- pre-commit: scripts/check-layers.sh
- pre-commit: cargo test --workspace --quiet
- pre-commit: cargo clippy --all-targets --quiet -- -D warnings
- cargo test -p agogo-cli --no-default-features --no-run
- cargo test -p agogo-cli --no-default-features --features core --no-run
- cargo test -p agogo-cli --no-default-features --features link --no-run
- cargo test -p agogo-cli --no-default-features --features demo --no-run
- cargo test -p agogo-cli --no-default-features --features run --no-run
- cargo test -p agogo-cli --all-features --no-run

## Local Review

No must-fix findings in the local review.

Checked the refactor for command-surface preservation, feature-gate
coverage, moved-file float allowlist updates, and stale references to
the old flat CLI module names. The staged diff keeps existing command
names, flag names, CSV headers, and unit tests intact while fixing the
previous `--no-default-features` compile break.

<!-- gh-id: 4209262605 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-05-01 00:11 UTC](https://github.com/cmk/agogo/pull/51#pullrequestreview-4209262605))

## Pull request overview

This PR restructures the `agogo-cli` crate into clearer submodules (parser/dispatch vs. command handlers), groups trace/time/link functionality under dedicated directories, and adjusts feature dependencies so `--no-default-features` builds compile cleanly across feature combinations.

**Changes:**
- Split CLI parsing/dispatch out of `main.rs` into `command.rs`, and centralized bpaf parsing helpers into `parsers.rs`.
- Moved/organized handlers into `trace/`, `time/`, and `link/` modules; added shared trace helper functions to reduce duplication.
- Updated CLI feature dependencies and refreshed float allowlist + docs/plans/todo tracking to match the new layout.

### Reviewed changes

Copilot reviewed 16 out of 19 changed files in this pull request and generated 1 comment.

<details>
<summary>Show a summary per file</summary>

| File | Description |
| ---- | ----------- |
| scripts/check-floats.sh | Updates float allowlist paths for moved CLI modules/parsers. |
| doc/todo.md | Updates module-path references and closes the no-default-features build-break TODO. |
| doc/plans/plan-2026-04-30-03.md | Adds a plan document describing the CLI cleanup and validation steps. |
| crates/cli/src/trace/sync.rs | Adds the `sync trace` implementation and an E2E convergence test in its new module location. |
| crates/cli/src/trace/mod.rs | Introduces trace subcommand types, dispatchers, and shared trace utilities (grid parsing, SR validation, overflow checks). |
| crates/cli/src/trace/midi.rs | Refactors MIDI trace to use shared trace helpers and centralized parsers. |
| crates/cli/src/trace/channel.rs | Refactors channel trace to use shared trace helpers and centralized parsers. |
| crates/cli/src/time/schedule.rs | Adds the time schedule handler module (swing config + tick scheduling) with unit tests. |
| crates/cli/src/time/mod.rs | Adds time subcommand wiring and dispatch for `time schedule`. |
| crates/cli/src/run.rs | Switches to shared parser helpers from `parsers.rs` after the module split. |
| crates/cli/src/parsers.rs | New shared bpaf parser adapters, feature-gated for `core`/`link`. |
| crates/cli/src/main.rs | Becomes a thin entrypoint delegating to `command::cli()` + `command::dispatch()`. |
| crates/cli/src/link/probe.rs | Adds a `print_csv` helper to consolidate Link probe printing logic. |
| crates/cli/src/link/mod.rs | Introduces Link subcommand types and dispatch in a dedicated module. |
| crates/cli/src/link/commands.rs | Moves Link subcommand implementations (`push-tempo`, `transport`, `diag`) into a dedicated module. |
| crates/cli/src/demo.rs | Adds `DemoSub` parsing + dispatch in-module and updates parser imports. |
| crates/cli/src/command.rs | New top-level CLI parser/dispatcher with feature-gated command availability. |
| crates/cli/Cargo.toml | Fixes feature dependency graph so `link`/`demo`/`run` imply `core`. |
| CLAUDE.md | Updates float allowlist documentation and references to reflect new CLI file paths. |
</details>






<!-- gh-id: 3171479043 -->
### Copilot on [`crates/cli/src/time/mod.rs:12`](https://github.com/cmk/agogo/pull/51#discussion_r3171479043) (2026-05-01 00:11 UTC)

The help text says swing makes odd (off-beat) steps come out earlier than nominal, but the actual swing implementation moves positive swing later (e.g., `effective_tick(T16, amount=80)` maps 240→320 and `schedule_ticks_swing_054_shifts_off_beats` expects `+19`). Please update this description (e.g., “later/delayed” for swing > 0.5) so CLI docs match behavior.


<!-- gh-id: 3171490538 -->
#### ↳ cmk ([2026-05-01 00:14 UTC](https://github.com/cmk/agogo/pull/51#discussion_r3171490538))

Fixed in the follow-up commit: the `time schedule` help now says positive swing delays odd steps relative to the nominal grid position, matching `schedule_ticks_swing_054_shifts_off_beats` and the `effective_tick` behavior.
