# PR #72 — Reshape CLI module tree

## Summary

Reshapes `crates/cli` so the crate reads as a CLI shell rather than a source
root kitchen sink. `main.rs` is now only the application entrypoint, `command.rs`
is the top-level parser/dispatcher, and command implementations live under
`src/command/`.

Renames the argv-boundary helper module from `parsers.rs` to `parse.rs`, moves
the old `trace`, `time`, `link`, `demo`, and `run` modules under `command/`,
and deletes the old module roots. Adds explicit CLI command-surface integration
tests under `crates/cli/test/` with `[[test]]` targets for `demo`, `link`, and
`run`.

Extends `scripts/check-layers.sh` to cover `agogo-cli` with `command -> parse`,
and updates AGENTS plus `scripts/check-floats.sh` for the new CLI paths.

Verification run locally:

- `cargo check -p agogo-cli --no-default-features --features core --quiet`
- `cargo check -p agogo-cli --no-default-features --features cpal --quiet`
- `cargo check -p agogo-cli --no-default-features --features midi --quiet`
- `cargo check -p agogo-cli --no-default-features --features link --quiet`
- `cargo test -p agogo-cli --features demo --test demo --quiet`
- `cargo test -p agogo-cli --features link --test link --quiet`
- `cargo test -p agogo-cli --features run --quiet`
- `cargo test -p agogo-cli --all-features --quiet`
- `cargo test --workspace --quiet`
- `cargo clippy --all-targets --quiet -- -D warnings`
- `cargo fmt --all -- --check`
- `scripts/check-floats.sh`
- `scripts/check-layers.sh`
- `scripts/check-connections.sh`
- `scripts/check-boundary-panics.sh`
- `scripts/check-pii.sh`
- `git diff --check`

## Local review (2026-05-03)

**Branch:** plan/2026-05-04-01
**Commits:** 3 (origin/main..plan/2026-05-04-01)
**Reviewer:** Codex (`codex review --base origin/main`)

---

The changes are a mechanical CLI module reshaping with imports, feature gates, layer checks, and float allowlists updated consistently. I did not find any actionable regressions in the diff, and the relevant CLI checks/tests compile successfully.

