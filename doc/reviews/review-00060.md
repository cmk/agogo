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
