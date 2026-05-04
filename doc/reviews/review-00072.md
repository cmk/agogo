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

The changes are a mechanical CLI module reshaping with imports, feature gates, layer checks, and float allowlists updated consistently. The relevant CLI checks/tests compile successfully.

Residual caveat: `crates/cli/src/command/link/probe.rs` still contains the pre-existing LAN-sensitive `peers == 0` assertion. This refactor only moved that test; a later Link test-isolation plan should replace it with a fixture/skip gate or non-networked seam.

<!-- gh-id: 3179294998 -->
### Copilot on [`doc/reviews/review-00072.md:47`](https://github.com/cmk/agogo/pull/72#discussion_r3179294998) (2026-05-04 03:48 UTC)

This local-review conclusion is no longer accurate for the current diff. `crates/cli/src/command/link/probe.rs` still contains the known LAN-sensitive `peers == 0` assertion, so saying there are no actionable regressions will mislead future readers reviewing this record.

<!-- gh-id: 3179312423 -->
#### ↳ cmk (2026-05-04 03:54 UTC)

Fixed. I revised the local-review record to avoid claiming no actionable regressions and added the residual caveat that `command/link/probe.rs` still has the pre-existing LAN-sensitive `peers == 0` assertion. This refactor only moved that test; a later Link test-isolation plan should replace it with a fixture/skip gate or non-networked seam.

<!-- gh-id: 3179295003 -->
### Copilot on [`doc/plans/plan-2026-05-04-01.md:36`](https://github.com/cmk/agogo/pull/72#discussion_r3179295003) (2026-05-04 03:48 UTC)

The target-tree diagram is now factually out of date: the PR actually introduces a `command/link/` subdirectory with `commands.rs` and `probe.rs`, but this section still presents `link.rs` as a single leaf file under `command/`. Because this file is meant to capture the intended post-refactor layout, readers will get the wrong module tree unless the nested Link files are shown here too.

<!-- gh-id: 3179312167 -->
#### ↳ cmk (2026-05-04 03:54 UTC)

Fixed. The target tree now shows the nested `command/link/` directory with `commands.rs` and `probe.rs`, matching the implemented layout.
