# PR #57 — feat: Add agogo facade namespace

## Summary

Adds a new `agogo` facade crate that presents the workspace through
nested public modules instead of fused crate roots:

- `agogo::core`
- `agogo::host`
- `agogo::host::cpal`
- `agogo::host::link`
- `agogo::host::midi`

The facade keeps backend crates optional and feature-gated, so the
default workspace build still avoids platform and Link dependencies.
`agogo-cli` now consumes the facade paths instead of importing fused
crate roots directly. Backend implementation crates continue to depend
on `agogo-core` at the Cargo layer to avoid dependency cycles, but
their Rust source routes through private `agogo::core` namespace shims.

Verification:

- `cargo fmt --all -- --check`
- `scripts/check-pii.sh`
- `scripts/check-floats.sh`
- `scripts/check-layers.sh`
- `cargo test --workspace --quiet`
- `cargo clippy --all-targets --quiet -- -D warnings`
- `cargo test -p agogo-cli --no-default-features --no-run`
- `cargo test -p agogo --no-default-features --quiet`
- `cargo test -p agogo --no-default-features --features core --quiet`
- `cargo test -p agogo --no-default-features --features host --quiet`
- `cargo test -p agogo --features cpal,link,midi --quiet`
- `cargo test -p agogo-cli --features run --no-run`
- `cargo test -p agogo-cli --no-default-features --features core,cpal,midi --no-run`
- `cargo test --manifest-path crates/host-cpal/Cargo.toml --offline`
- `cargo test --manifest-path crates/host-midi/Cargo.toml --offline`
- `cargo test --manifest-path crates/host-link/Cargo.toml --features rusty-link --no-run --offline`

## Local review (2026-05-01)

**Branch:** plan-2026-05-02-02
**Commits:** 4 (origin/main..plan-2026-05-02-02)
**Reviewer:** Codex (`codex review --base origin/main`)

---

The facade works for default and backend feature combinations, but the standalone `host` feature exposes host APIs while hiding the matching `agogo::core` types needed to use them.

Review comment:

- [P2] Make the host feature enable the core facade — `crates/agogo/Cargo.toml:12`
  When a downstream user disables defaults and enables only `features = ["host"]`, `agogo::host` exposes public APIs that use core types such as `AgogoDriverConfig::initial_tempo: Tempo`, but `agogo::core` is cfg'd out, so the facade cannot be used to construct those APIs without adding the old fused core crate directly. Since `host_impl` already depends on core, make the `host` feature include `core` so the public namespace stays self-contained.

Resolution: fixed by making the facade `host` feature imply `core` and
verifying `cargo test -p agogo --no-default-features --features host --quiet`.

<!-- gh-id: 4214631598 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-05-02 06:55 UTC](https://github.com/cmk/agogo/pull/57#pullrequestreview-4214631598))

## Pull request overview

Adds a new `agogo` facade crate that exposes the workspace as a nested public namespace (`agogo::{core, host, host::{cpal, link, midi}}`), keeping backend crates optional/feature-gated while migrating in-repo consumers to the new paths.

**Changes:**
- Introduces `crates/agogo` facade crate with feature-gated module re-exports and compile-smoke tests.
- Migrates CLI + host/backend crates from fused crate roots (`agogo_core`, `agogo_host_*`) to `agogo::...` paths (with internal `agogo::core` shims in backend crates to avoid Cargo cycles).
- Updates docs/plans/reviews to reference the new public namespace.

### Reviewed changes

Copilot reviewed 45 out of 46 changed files in this pull request and generated 1 comment.

<details>
<summary>Show a summary per file</summary>

| File | Description |
| ---- | ----------- |
| doc/todo.md | Updates a referenced symbol path to the new `agogo::host::link` namespace. |
| doc/reviews/review-00057.md | Adds PR review record capturing facade behavior and resolution. |
| doc/plans/plan-2026-05-02-02.md | Adds implementation plan/verification steps for the facade namespace work. |
| doc/designs/snapshot.md | Updates design doc type references to `agogo::core` paths. |
| crates/host/src/snapshot.rs | Switches host crate imports to `agogo::core` shim path. |
| crates/host/src/lib.rs | Adds internal `agogo::core` shim (`extern crate self as agogo` + `core_impl`). |
| crates/host/src/driver.rs | Switches imports to `agogo::core` shim path. |
| crates/host/src/bridge.rs | Switches imports to `agogo::core` shim path. |
| crates/host/Cargo.toml | Renames `agogo-core` dep to `core_impl` alias for shim routing. |
| crates/host-midi/src/midir.rs | Switches imports to `agogo::core` shim path. |
| crates/host-midi/src/lib.rs | Adds internal `agogo::core` shim and updates crate docs to facade paths. |
| crates/host-midi/README.md | Updates README API references to `agogo::core`. |
| crates/host-midi/Cargo.toml | Renames `agogo-core` dep to `core_impl` alias for shim routing. |
| crates/host-link/tests/bidirectional.rs | Adds a test-local `agogo::{core, host::link}` shim to exercise facade-like paths. |
| crates/host-link/src/source.rs | Switches imports/docs to `agogo::core` shim path. |
| crates/host-link/src/session.rs | Switches imports/docs to `agogo::core` shim path. |
| crates/host-link/src/quantum.rs | Switches imports/docs to `agogo::core` shim path. |
| crates/host-link/src/link.rs | Switches imports/docs to `agogo::core` shim path. |
| crates/host-link/src/lib.rs | Adds internal `agogo::core` shim and updates docs to facade paths. |
| crates/host-link/Cargo.toml | Renames `agogo-core` dep to `core_impl` alias for shim routing. |
| crates/host-cpal/src/lib.rs | Adds internal `agogo::core` shim and updates crate docs to facade paths. |
| crates/host-cpal/src/cpal/control.rs | Switches imports/tests to `agogo::core` shim path. |
| crates/host-cpal/src/cpal/callback.rs | Switches imports/docs to `agogo::core` shim path. |
| crates/host-cpal/src/cpal.rs | Switches imports to `agogo::core` shim path. |
| crates/host-cpal/README.md | Updates README API references to `agogo::core`. |
| crates/host-cpal/Cargo.toml | Renames `agogo-core` dep to `core_impl` alias for shim routing. |
| crates/core/src/test.rs | Updates internal documentation to prefer the facade path. |
| crates/core/src/conn/float.rs | Updates internal comment to prefer the facade path. |
| crates/core/src/channel/spec/types.rs | Updates doc comment to reference `agogo::host::link::Quantum`. |
| crates/core/src/channel/dsl.rs | Updates doc example to `agogo::core` and marks it `ignore` to avoid cycles. |
| crates/cli/src/trace/sync.rs | Migrates trace module imports to `agogo::core`. |
| crates/cli/src/trace/mod.rs | Migrates CLI types/consts to `agogo::core`. |
| crates/cli/src/trace/midi.rs | Migrates trace-midi imports/tests to `agogo::core`. |
| crates/cli/src/trace/channel.rs | Migrates trace-channel imports to `agogo::core`. |
| crates/cli/src/time/schedule.rs | Migrates scheduling imports/consts to `agogo::core`. |
| crates/cli/src/run.rs | Migrates run pipeline imports to `agogo::{core, host::*}` and updates Link helper path. |
| crates/cli/src/parsers.rs | Migrates CLI parsers to `agogo::core` and Link parser re-export to `agogo::host::link`. |
| crates/cli/src/link/probe.rs | Migrates Link probe command to `agogo::{core, host::link}` paths. |
| crates/cli/src/link/mod.rs | Migrates Link subcommand types/defaults to facade paths. |
| crates/cli/src/link/commands.rs | Migrates Link commands module to facade paths. |
| crates/cli/src/demo.rs | Migrates demo pipeline imports to `agogo::{core, host::*}`. |
| crates/cli/Cargo.toml | Replaces direct fused-root deps with optional `agogo` facade dep + feature forwarding. |
| crates/agogo/src/lib.rs | Adds facade namespace modules and compile-smoke tests. |
| crates/agogo/Cargo.toml | Defines facade features and optional deps for core/host/backends. |
| Cargo.toml | Adds `crates/agogo` to workspace members. |
| Cargo.lock | Records the new `agogo` package and updates `agogo-cli` deps accordingly. |
</details>






<!-- gh-id: 3176283164 -->
### Copilot on [`crates/agogo/src/lib.rs:56`](https://github.com/cmk/agogo/pull/57#discussion_r3176283164) (2026-05-02 06:55 UTC)

The `core_and_host_namespace_paths_compile` test is compiled whenever `cfg(test)` is active, but it unconditionally references `crate::host::*`. If someone runs `cargo test -p agogo --no-default-features --features core`, the `host` module is cfg’d out and this test won’t compile. Gate this test with `#[cfg(feature = "host")]` (or `all(feature = "core", feature = "host")`), and/or add a separate core-only compile smoke test that doesn’t touch `crate::host`.

<!-- gh-id: 3176285510 -->
#### ↳ cmk ([2026-05-02 06:57 UTC](https://github.com/cmk/agogo/pull/57#discussion_r3176285510))

Fixed. `core_and_host_namespace_paths_compile` is now gated behind `feature = "host"`, and I added a separate core-only smoke test. Verified `cargo test -p agogo --no-default-features --features core --quiet` and `cargo test -p agogo --no-default-features --features host --quiet`.
