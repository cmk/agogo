# PR #71 — Draw chan/core/host boundary

## Summary

Renames the pure library crate from `agogo-core` to `agogo-chan` and renames
the runtime orchestration crate from `agogo-host` to `agogo-core`. The new
runtime crate re-exports the pure channel/time/control/sink surface from
`agogo-chan`, preserving downstream `agogo::core::*` call sites while making
the internal dependency direction explicit.

Moves the per-buffer scheduler (`event.rs`) and `Playhead` transport runtime
(`transport.rs`) into the new `agogo-core`, and flattens the old
`control::sync` modules into `agogo-chan::control::{detect, pll, pulse,
source}` so `control` now means pure control-loop logic.

Extends `scripts/check-layers.sh` from the old core-only rule to enforce
layer sentinels across `agogo-chan`, `agogo-core`, and the detached host
adapter crates. Updates AGENTS, float/boundary-panic gates, pre-commit fmt
scope, and host adapter module sentinels to match the new crate boundary.

Verification run locally:

- `cargo metadata --format-version 1 --no-deps`
- `cargo test -p agogo-chan --quiet`
- `cargo test -p agogo-core --quiet`
- `cargo test --workspace --quiet`
- `cargo clippy --all-targets --quiet -- -D warnings`
- `cargo test --manifest-path crates/host-link/Cargo.toml --quiet`
- `cargo test --manifest-path crates/host-midi/Cargo.toml --quiet`
- `cargo test --manifest-path crates/host-cpal/Cargo.toml --quiet`
- pre-commit gates: fmt, PII, floats, layers, connections, boundary panics

## Local review (2026-05-03)

**Branch:** plan/2026-05-03-06
**Commits:** 3 (origin/main..plan/2026-05-03-06)
**Reviewer:** Codex (`codex review --base origin/main`)

---

The runtime code and default workspace tests appear to pass, but the module move leaves an existing proptest regression seed on the old path, weakening regression coverage. This should be corrected with the rename.

Review comment:

- [P3] Move the PLL proptest regression seed — `crates/chan/src/control.rs:14`
  Because the PLL module is flattened from `control::sync::pll` to `control::pll` here, proptest's default persistence path changes to `proptest-regressions/control/pll.txt`. The checked-in seed is still under `crates/chan/proptest-regressions/control/sync/pll.txt`, so that historical `bpm = 185.18518` regression will no longer be replayed; move the regression file with the module path.
