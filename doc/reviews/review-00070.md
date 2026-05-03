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
and collapsible `if` cases.

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
