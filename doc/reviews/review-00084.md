# PR #84 - Port template workflow contract

## Summary

Ports the current `template-rust` workflow contract into `agogo`.

- Renames the workflow entrypoints to the current `pr_*`, `git_*`, and underscore script names, and updates AGENTS, workflow docs, Claude commands, hooks, CI, and PR templates to match.
- Keeps `agogo`-specific gates active: float discipline, layer checks, connection construction, boundary panic checks, and the audit harness.
- Adds Python workflow/audit tests and wires `python3 -m unittest` into CI.

Verification:

- `bash -n scripts/*.sh .githooks/pre-commit .githooks/pre-push`
- `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest`
- `scripts/pr_report.py path 1`
- `scripts/workflow_state.sh`
- `scripts/check_pii.sh`
- `scripts/check_layers.sh`
- `scripts/check_floats.sh`
- `scripts/check_connections.sh`
- `scripts/check_boundary_panics.sh`
- `cargo fmt -p agogo-chan -p agogo-core -p agogo-cli -- --check`
- `git diff --cached --check`

## Local review (2026-05-07)

**Branch:** plan/2026-05-07-01
**Commits:** 3 (origin/main..plan/2026-05-07-01)
**Reviewer:** Codex (`codex review --base origin/main`)

---

The patch regresses the PII helper's full-tree mode and leaves several live workflow/audit instructions pointing at deleted commands. These issues would break documented maintenance flows even though the new unit tests pass.

Full review comments:

- [P2] Preserve full-tree PII scanning — scripts/check_pii.sh:36-40
  When a release/audit run uses the existing `scripts/check_pii.sh --tree` mode after this rename, the argument is ignored and the script always scans only `git diff --cached`; with a clean index it exits 0 even if tracked files contain `/Users/...`, private-key headers, or token shapes. This regresses the full-repo PII check from the old script, so keep the `--staged|--tree` parser/tree scan or fail on unknown args instead of silently doing the staged scan.

- [P2] Update the post-push review command name — AGENTS.md:582-582
  After this patch renames `.claude/commands/pull-reviews.md` to `pr-report.md`, the Tier 2 workflow still tells agents to run `/pull-reviews <N>`; when GitHub review activity arrives, following AGENTS will invoke a missing slash command instead of mirroring comments. Update this occurrence and the later “remains available” mention to `/pr-report <N>` or `scripts/pr_report.py reviews <N>`.

- [P2] Retarget active audit docs to renamed scripts — scripts/audit_run.py:28-32
  Because this change removes `scripts/audit.py`, `scripts/audit_state.sh`, and `scripts/check-pii.sh`, the active audit docs/prompts are now stale: `doc/audits/README.md` still has cron/manual commands for the deleted audit scripts, and `doc/audits/pii.md` tells the auditor to read the deleted PII script. Anyone using the audit harness after this rename will hit missing files or give the auditor wrong context, so update those live docs/prompts to `audit_run.py`, `audit_report.sh`, and `check_pii.sh`.

- [P3] Make --check validate PR-number prediction — scripts/pr_review.sh:13-13
  `scripts/pr_review.sh --check` now uses `scripts/pr_report.py path 1`, which succeeds without contacting GitHub; in an unauthenticated or wrong-repo shell the health check passes, but the real transition later runs `scripts/pr_report.py path` with no PR number and fails in `pr_request.sh`. Use the no-arg path/pr-request check here so `--check` catches the same gh auth/repo prerequisite it is meant to preflight.

<!-- gh-id: 3206030981 -->
### Copilot on [`tests/test_workflow_state.py:25`](https://github.com/cmk/agogo/pull/84#discussion_r3206030981) (2026-05-08 03:07 UTC)

`run_state()` unconditionally removes `WORKFLOW_REVIEW_FILE` and `WORKFLOW_STATE_ALLOW_REVIEW_PATH_FALLBACK` from the environment copy, so the `mock.patch.dict(os.environ, ...)` in `test_clean_pushed_branch_without_gh...` never affects the script under test. This makes the test misleading (it doesn't actually validate behavior in the presence of those env vars). Either drop the patch block, or stop stripping these vars in `run_state()` for tests that intend to cover them.

<!-- gh-id: 3206030994 -->
### Copilot on [`AGENTS.md:773`](https://github.com/cmk/agogo/pull/84#discussion_r3206030994) (2026-05-08 03:07 UTC)

The layering rule description is now incomplete relative to `scripts/check_layers.sh`: the script also matches facade-module roots like `use agogo::core::...` / `use agogo::chan::...` (and host module roots), not just `use crate::...`, `use agogo_chan::...`, and `use agogo_core::...`. Consider updating this sentence to enumerate (or at least mention) the additional roots so readers can predict what the gate will flag.


<!-- gh-id: 4249231190 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-05-08 03:07 UTC](https://github.com/cmk/agogo/pull/84#pullrequestreview-4249231190))

## Pull request overview

Ports the `template-rust` workflow contract into `agogo` by renaming/aligning workflow entrypoints (`pr_*`, `git_*`, underscore script names), updating the documented review-round FSM and hooks/CI to match, and adding Python unit tests for the workflow/audit harness.

**Changes:**
- Replace legacy PR helpers (`review_path.sh`, `extract_pr_body.sh`, etc.) with `scripts/pr_report.py` subcommands and updated shell entrypoints (`pr_review.sh`, `pr_request.sh`, `git_merge.sh`, `git_squash.sh`).
- Rename/refresh workflow gates and docs to underscore script names; extend `check_layers.sh` parsing to cover grouped/multiline `use` statements and additional import roots.
- Add Python unit tests and run them in CI (`python3 -m unittest`), including coverage for `workflow_state.sh` and `audit_run.py`.

### Reviewed changes

Copilot reviewed 35 out of 36 changed files in this pull request and generated 2 comments.

<details>
<summary>Show a summary per file</summary>

| File | Description |
| ---- | ----------- |
| tests/test_workflow_state.py | Adds unit tests for `workflow_state.sh` behavior in ephemeral git repos. |
| tests/test_audit_run.py | Adds unit tests for `audit_run.py` (force mode, `git ls-files` validation, cron failure aggregation). |
| tests/__init__.py | Makes `tests/` a Python package for discovery/import consistency. |
| scripts/workflow_state.sh | Switches to `pr_report.py path`, adds local-only review-file inference, tweaks FSM classification. |
| scripts/review_path.sh | Removes legacy review-path helper (superseded by `pr_report.py path`). |
| scripts/pr_review.sh | Updates local-review transition script to use `pr_report.py`, requires `gh`, and commits appended local review. |
| scripts/pr_request.sh | New/updated `gh`-based “next PR number” predictor with improved diagnostics. |
| scripts/pr_report.py | Consolidates PR workflow reporting into subcommands: `path`, `body`, `reviews`. |
| scripts/pr_reply.py | Renamed review-reply CLI; updates shared GitHub helper import. |
| scripts/github_client.py | Shared `gh` repo resolution / PR verification helper for `pr_report.py` + `pr_reply.py`. |
| scripts/git_squash.sh | Renames/standardizes autosquash wrapper naming and usage text. |
| scripts/git_merge.sh | Renames safe-merge wrapper; adds a dirty-working-tree refusal to avoid dropping uncommitted review mirroring. |
| scripts/extract_pr_body.sh | Removes legacy PR-body extractor (superseded by `pr_report.py body`). |
| scripts/check_pii.sh | Renames script and refactors allowlist filtering while preserving `--staged/--tree` behavior. |
| scripts/check_layers.sh | Renames script; expands import-root matching and improves grouped `use` parsing. |
| scripts/check_floats.sh | Renames script references/messages to underscore style. |
| scripts/check_connections.sh | Renames script references/messages to underscore style. |
| scripts/check_boundary_panics.sh | Renames script references/messages to underscore style. |
| scripts/audit_run.py | Renames audit harness entrypoint; adds `tracked_files_for` and cron failure aggregation. |
| scripts/audit_report.sh | Renames audit early-exit gate script and updates messaging/usage text. |
| doc/workflow.md | Updates mermaid diagrams and prose to new `/pr-*` commands and renamed scripts. |
| doc/reviews/review-00084.md | Adds/updates PR review record for this change set. |
| doc/plans/plan-2026-05-07-01.md | Adds/updates the implementation plan + verification list for the workflow port. |
| doc/audits/README.md | Updates audit harness documentation to `audit_run.py` / `audit_report.sh`. |
| doc/audits/pii.md | Updates audit prompt references to `check_pii.sh`. |
| AGENTS.md | Updates canonical workflow/tooling references to new script/command names. |
| .gitignore | Renames watch-loop state dir to `/.pr-watch/`. |
| .github/workflows/ci.yml | Adds `python3 -m unittest` and updates gate script names. |
| .github/pull_request_template.md | Updates checklist command name to `/pr-review`. |
| .githooks/pre-push | Updates comments/style; keeps pre-push test+clippy gate. |
| .githooks/pre-commit | Updates gate commands to underscore script names and simplifies step labeling. |
| .claude/settings.json | Updates agent hook wiring to run the same gate chain on `git commit*`. |
| .claude/commands/pr-watch.md | Updates the polling workflow template to use `pr_report.py`/`pr_reply.py` and `.pr-watch` state. |
| .claude/commands/pr-review.md | Updates Tier-1 review command template to new script names and workflow wording. |
| .claude/commands/pr-report.md | Updates GitHub review-mirroring command template to `pr_report.py reviews`. |
| .claude/commands/pr-reply.md | Updates reply+mirror+atomic-commit command template to new scripts and commit-prefix logic. |
</details>
<details>
<summary>Comments suppressed due to low confidence (1)</summary>

**scripts/audit_run.py:306**
* `cmd_run` calls `changed_files_since_last(audit)` even when `--force` is set, but the docstring/CLI help say `--force` should skip the early-exit gate. As written, a broken/missing `scripts/audit_report.sh` (or any error in the gate) will still prevent forced runs. Consider branching first: if `--force`, compute `changed = tracked_files_for(audit)` without invoking the state script; otherwise call `changed_files_since_last` and early-exit on empty.
</details>

<!-- gh-id: 3206055690 -->
#### ↳ cmk ([2026-05-08 03:16 UTC](https://github.com/cmk/agogo/pull/84#discussion_r3206055690))

Fixed: removed the ineffective environment patch from the workflow-state test, so it now directly reflects run_state()'s default environment-clearing behavior.

<!-- gh-id: 3206056258 -->
#### ↳ cmk ([2026-05-08 03:17 UTC](https://github.com/cmk/agogo/pull/84#discussion_r3206056258))

Fixed: AGENTS.md now names the crate-root and facade-root imports checked by scripts/check_layers.sh, including agogo::chan, agogo::core, and the host facade roots.
