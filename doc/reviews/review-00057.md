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
