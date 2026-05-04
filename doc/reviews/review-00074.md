# PR #74 - GitHub open-source readiness

## Summary

Prepares the repository for public GitHub reading while keeping crates.io
publishing explicitly out of scope.

Rewrites the root README as a public project entry point with current status,
source-build instructions, hardware-free command examples, hardware-backed
`agogo run` examples, timing caveats, workspace layout, feature flags, roadmap
links, and a publishing-status section that keeps `publish = false`
intentional.

Removes tracked contributor-local home-directory paths from historical plan and
review records without deleting the underlying context. Adds `scripts/check-pii.sh
--tree` so release checks can scan every tracked file while preserving the fast
staged-diff mode used by hooks. Updates `doc/todo.md` to track GitHub
open-sourcing and crates.io publishing as separate milestones.

Verification run locally:

- `scripts/check-pii.sh --tree`
- `scripts/check-pii.sh --staged`
- `bash -n scripts/check-pii.sh`
- `cargo run -p agogo-cli --bin agogo -- --help`
- `cargo run -p agogo-cli --bin agogo -- channel trace --help`
- `cargo run -p agogo-cli --bin agogo -- midi trace --help`
- `cargo run -p agogo-cli --bin agogo -- sync trace --help`
- `cargo run -p agogo-cli --features run --bin agogo -- run --help`
- `cargo run -p agogo-cli --bin agogo -- channel trace --bpm 120 --sr 48000 --grid t4 --buffers 2 --frames 256`
- `cargo run -p agogo-cli --bin agogo -- midi trace --bpm 120 --sr 48000 --grid t32t --buffers 2 --frames 256`
- `cargo run -p agogo-cli --bin agogo -- sync trace --bpm 120 --sr 48000 --ppq 4 --pulses 4`
- `scripts/check-layers.sh`
- `scripts/check-floats.sh`
- `scripts/check-connections.sh`
- `scripts/check-boundary-panics.sh`
- `cargo fmt --all -- --check`
- `cargo test --workspace`
- `cargo clippy --all-targets -- -D warnings`
