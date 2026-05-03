# PR #70 — Two-repo agogo/std-core runtime helper

## Summary

Adds the agogo side of the two-repo `stdio-core <-> agogo` integration slice
without introducing a stdio-core dependency into agogo.

The new `agogo_host::runtime` module packages `AgogoDriver`,
`ControlConsumer`, `Playhead`, `SnapshotSlot`, and `SnapshotPublisher` behind a
small runtime helper that stdio-core test/dev code can wrap later. It exposes
mount/unmount, tool-call handling, buffer advancement, snapshot publishing,
structured step reports, and agogo-owned metadata/surface facts for mapping into
stdio-core types.

The branch also aligns agogo's Rust pin with stdio-core's Rust 1.92 across the
workspace, detached host crates, toolchain file, CI/docs workflows, and active
repo workflow prose. The Rust 1.92 bump required mechanical clippy cleanup for
new lints, including `is_multiple_of`, derived defaults, doc-list indentation,
and collapsible `if` cases. It also updates `time` to the non-advisory release
and drops the now-stale cargo-deny ignore.

Validation:

- `cargo fmt --all -- --check`
- `scripts/check-pii.sh`
- `scripts/check-floats.sh`
- `scripts/check-layers.sh`
- `scripts/check-connections.sh`
- `scripts/check-boundary-panics.sh`
- `cargo test --workspace --quiet`
- `cargo clippy --all-targets --quiet -- -D warnings`
- `git diff --check`

## Local review (2026-05-03)

**Branch:** plan-2026-05-03-05
**Commits:** 5 (origin/main..plan-2026-05-03-05)
**Reviewer:** Codex (`codex review --base origin/main`)

---

The runtime helper can publish incorrect transport state for stop commands, and the MSRV bump leaves a now-actionable security advisory suppressed. These are actionable correctness/security issues introduced by the patch.

Full review comments:

- [P2] Report stopped transport after stop commands — crates/host/src/runtime.rs:176-177
  When `agogo.stop` is exercised through `run_command_step`, `apply_control_to_playhead` only stages the Stop for `Playhead::on_buffer`, but `advance_buffer` writes the snapshot without ever rendering that buffer. Because the snapshot state is derived from `is_running()` here, the report can say the stop command applied while the emitted `agogo-state` observation still says `running`, which breaks adapter tests that treat snapshots as the resulting transport truth.

- [P2] Drop the stale RUSTSEC ignore after the MSRV bump — deny.toml:13-13
  With this patch agogo's MSRV is 1.92, but the advisory ignore still suppresses RUSTSEC-2026-0009 even though the comment says the fixed `time` release only needed Rust 1.88 and should be re-evaluated once MSRV reached 1.88+. CI will keep allowing the locked vulnerable `time 0.3.45` instead of forcing the now-compatible 0.3.47 update, so the security gate remains blind to a fixable advisory.

## Local review (2026-05-03)

**Branch:** plan-2026-05-03-05
**Commits:** 6 (origin/main..plan-2026-05-03-05)
**Reviewer:** Codex (`codex review --base origin/main`)

---

The new runtime helper can expose an incorrect initial observation state before any buffer advancement, which affects the adapter-facing surface it is intended to support.

Review comment:

- [P2] Seed the initial runtime snapshot from the playhead — crates/host/src/runtime.rs:74-79
  When an adapter reads or publishes the runtime surface immediately after `mount()` but before the first `advance_buffer()`, this slot still contains `SnapshotSlot`'s default stopped/zero-audio frame even though the `Playhead` constructed above is already running (`TransportState::new` starts with `running = true`). That makes the initial `agogo-state` observation falsely report `stopped` until some later buffer write happens; initialize the slot from the runtime/playhead state or make the playhead start stopped.
