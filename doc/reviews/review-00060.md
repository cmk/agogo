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
