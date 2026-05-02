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
`agogo-cli` now consumes the facade paths instead of importing
`agogo_core`, `agogo_host_cpal`, `agogo_host_link`, or
`agogo_host_midi` directly. Backend crates continue to depend on
`agogo-core` directly to avoid dependency cycles.

Verification:

- `cargo fmt --all -- --check`
- `scripts/check-pii.sh`
- `scripts/check-floats.sh`
- `scripts/check-layers.sh`
- `cargo test --workspace --quiet`
- `cargo clippy --all-targets --quiet -- -D warnings`
- `cargo test -p agogo-cli --no-default-features --no-run`
- `cargo test -p agogo --features cpal,link,midi --quiet`
- `cargo test -p agogo-cli --features run --no-run`
- `cargo test -p agogo-cli --no-default-features --features core,cpal,midi --no-run`
