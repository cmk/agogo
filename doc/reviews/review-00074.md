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

Also updates the drafted next plan so its deterministic render work is preceded
by mechanical public-API cleanup: `agogo::chan` facade migration,
`conn::boundary` merger into `conn::float`, `conn::sample` rename to
`conn::rate`, and `SXYZ` sample-rate names becoming `RXYZ`.

Screens out additional public-tree naming/path leaks by replacing Recologic's
former product name in tracked prose and removing two local product-family path
references while preserving the surrounding design context.

Verification run locally:

- `scripts/check-pii.sh --tree`
- `scripts/check-pii.sh --staged`
- targeted scan for the additional screened terms
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

## Local review (2026-05-04)

**Branch:** plan/2026-05-04-02
**Commits:** 3 (origin/main..plan/2026-05-04-02)
**Reviewer:** Codex (`codex review --base origin/main`)

---

The README/documentation updates and the new check-pii tree mode appear consistent with the intended open-source readiness changes. I did not identify any discrete correctness issue introduced by the diff.

<!-- gh-id: 3179680017 -->
### Copilot on [`scripts/check-pii.sh:61`](https://github.com/cmk/agogo/pull/74#discussion_r3179680017) (2026-05-04 06:18 UTC)

`filter_allowed` uses `grep -vE '^\s*(#|$)'` to strip blank/comment lines from `.pii-allow`, but `\s` is not a POSIX ERE escape in `grep -E` (it’s treated as a literal `s` on many platforms). This can cause allow-list parsing to behave incorrectly for lines with leading whitespace. Prefer `[[:space:]]*` (or `[ \t]*`) instead of `\s*`.


<!-- gh-id: 3179680059 -->
### Copilot on [`scripts/check-pii.sh:82`](https://github.com/cmk/agogo/pull/74#discussion_r3179680059) (2026-05-04 06:18 UTC)

In `--tree` mode, `matches` comes from `grep -nE`, so each line is prefixed with `N:`. `filter_allowed` then applies `.pii-allow` patterns to that prefixed string, which means allow-list regexes written to match the actual offending line content won’t match in tree mode (but will in staged mode). Consider stripping the `^[0-9]+:` prefix before applying allow patterns (while still reporting line numbers), or avoid `-n` during the allow-list filtering step.

<!-- gh-id: 4217776473 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-05-04 06:18 UTC](https://github.com/cmk/agogo/pull/74#pullrequestreview-4217776473))

## Pull request overview

Prepares the repository for public GitHub reading (docs + hygiene tooling) while explicitly keeping crates.io publishing out of scope.

**Changes:**
- Add `--tree` mode to `scripts/check-pii.sh` to scan the full tracked tree (while keeping the staged-diff hook mode).
- Rewrite `README.md` as a public-facing entry point with build instructions, hardware-free examples, and feature-flag guidance.
- Remove contributor-local absolute paths from historical plan/review docs and track the GitHub-vs-crates.io milestone split in `doc/todo.md`.

### Reviewed changes

Copilot reviewed 20 out of 20 changed files in this pull request and generated 2 comments.

<details>
<summary>Show a summary per file</summary>

| File | Description |
| ---- | ----------- |
| scripts/check-pii.sh | Adds `--tree` mode and refactors allow-list filtering. |
| README.md | Rewrites README for public consumption; adds commands, layout, flags, and publishing status. |
| doc/todo.md | Adds “Release readiness” section separating GitHub open-sourcing from crates.io publishing. |
| doc/reviews/review-00074.md | Adds review record for this PR. |
| doc/reviews/review-00023.md | Removes contributor-local absolute path reference from review prose. |
| doc/reviews/review-00006.md | Removes contributor-local absolute path reference from review prose. |
| doc/plans/plan-2026-05-04-03.md | Adds next plan draft (chan facade/rate renames/deterministic render). |
| doc/plans/plan-2026-05-04-02.md | Adds plan documenting GitHub open-source readiness scope and tasks. |
| doc/plans/plan-2026-04-27-03.md | Removes contributor-local absolute path reference from plan prose. |
| doc/plans/plan-2026-04-25-05.md | Removes contributor-local absolute path reference; replaces with upstream module description. |
</details>






<!-- gh-id: 3179696427 -->
#### ↳ cmk ([2026-05-04 06:22 UTC](https://github.com/cmk/agogo/pull/74#discussion_r3179696427))

Fixed in the checker: allow-list parsing now uses POSIX `[[:space:]]*` for blank/comment lines instead of `\s*`, so leading-whitespace comments are handled portably by `grep -E`.

<!-- gh-id: 3179697803 -->
#### ↳ cmk ([2026-05-04 06:23 UTC](https://github.com/cmk/agogo/pull/74#discussion_r3179697803))

Fixed in tree mode: numbered `grep -nE` matches are now filtered by applying `.pii-allow` patterns to the original line content with the line-number prefix stripped, while still reporting the numbered line when a match remains.
