# PR #52 — doc: Add multi-agent workflow instructions

<!-- gh-id: 3171642841 -->
### Copilot on [`scripts/workflow_state.sh:66`](https://github.com/cmk/agogo/pull/52#discussion_r3171642841) (2026-05-01 01:16 UTC)

The state classifier doesn’t currently model the `pushed` state from `doc/workflow.md`: a branch that has been pushed (remote tracking ref exists and local is not ahead) but has no PR yet will still report `local_reviewed`. Also, on `main` the script reports `main_clean`/`main_dirty` without considering whether `main` is ahead of `origin/main`, which can mask an unpushed-main situation. Consider adding explicit checks for “branch pushed & in sync with origin/<branch>” and for “main ahead of origin/main” so the reported FSM state matches the documented diagram.

<!-- gh-id: 3171642853 -->
### Copilot on [`scripts/workflow_state.sh:25`](https://github.com/cmk/agogo/pull/52#discussion_r3171642853) (2026-05-01 01:16 UTC)

`review_file=$WORKFLOW_REVIEW_FILE` should be quoted to avoid word-splitting/globbing if the override path contains spaces or glob characters.


<!-- gh-id: 3171642862 -->
### Copilot on [`scripts/local_review.sh:12`](https://github.com/cmk/agogo/pull/52#discussion_r3171642862) (2026-05-01 01:16 UTC)

`--check` verifies `codex` but the script later depends on `scripts/review_path.sh` (which, when called with no args, requires an authenticated `gh` CLI via `scripts/next_pr_number.sh`). It would be less surprising if `--check` also validated `gh` availability/auth (or that `scripts/review_path.sh` succeeds), so failures happen early with a clear message.


<!-- gh-id: 4209447263 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-05-01 01:16 UTC](https://github.com/cmk/agogo/pull/52#pullrequestreview-4209447263))

## Pull request overview

This PR introduces a shared, tool-agnostic agent/workflow instruction set (via `AGENTS.md`) and adds supporting scripts so the documented FSM workflow can be followed from Codex/shell as well as Claude Code.

**Changes:**
- Add `AGENTS.md` as the canonical agent instruction doc (with `CLAUDE.md` treated as compatibility) and update docs to point to it.
- Add workflow helper scripts: a best-effort workflow state inspector and a Codex-backed local review transition.
- Update existing scripts/docs to reference the renamed workflow doc and the new local review transition.

### Reviewed changes

Copilot reviewed 13 out of 14 changed files in this pull request and generated 5 comments.

<details>
<summary>Show a summary per file</summary>

| File | Description |
| ---- | ----------- |
| scripts/workflow_state.sh | New best-effort FSM state reporter for repo workflow. |
| scripts/next_pr_number.sh | Updates comments to reference the new local review transition naming behavior. |
| scripts/local_review.sh | New Codex-backed `plan_finalized -> local_reviewed` transition script. |
| scripts/extract_pr_body.sh | Updates references to `AGENTS.md` and clarifies review-marker semantics. |
| scripts/check-layers.sh | Updates guidance to document rationale in `AGENTS.md`. |
| scripts/check-floats.sh | Updates references from `CLAUDE.md` to `AGENTS.md`. |
| doc/workflow.md | Updates diagrams/text to include `scripts/local_review.sh` as an alternative to `/sprint-review`. |
| doc/reviews/review-00000.md | Updates the sentinel explanation to reference the local review transition. |
| README.md | Adds repo intro + points to `AGENTS.md`, and documents local review/state scripts. |
| AGENTS.md | New canonical, multi-agent workflow/conventions document. |
| .claude/commands/watch-pr.md | Updates conventions reference to `AGENTS.md`. |
| .claude/commands/sprint-review.md | Clarifies `/sprint-review` as the Claude implementation of the same transition and updates references to `AGENTS.md`. |
</details>






<!-- gh-id: 3171642871 -->
### Copilot on [`scripts/extract_pr_body.sh:53`](https://github.com/cmk/agogo/pull/52#discussion_r3171642871) (2026-05-01 01:16 UTC)

The extraction logic below stops at review markers (`## Local review (` / `<!-- gh-id:`), but the header comment earlier in this file still says it stops at the next `##` heading. Please reconcile the top-level description with the actual stopping condition to avoid misleading users about what content can safely appear in the PR body.

<!-- gh-id: 3171642880 -->
### Copilot on [`README.md:14`](https://github.com/cmk/agogo/pull/52#discussion_r3171642880) (2026-05-01 01:16 UTC)

README suggests “Codex or shell: scripts/local_review.sh”, but the transition script hard-depends on the `codex` CLI (and indirectly on authenticated `gh` for review-path prediction). Consider calling out those prerequisites here so a plain shell user doesn’t discover it only after running the command.


<!-- gh-id: 3172382559 -->
#### ↳ cmk ([2026-05-01 06:50 UTC](https://github.com/cmk/agogo/pull/52#discussion_r3172382559))

Fixed in this round — `workflow_state.sh` now reports `pushed` for a synced reviewed branch with no PR, and distinguishes clean `main` from `main_unpushed` when local main is ahead of origin.

<!-- gh-id: 3172383422 -->
#### ↳ cmk ([2026-05-01 06:51 UTC](https://github.com/cmk/agogo/pull/52#discussion_r3172383422))

Fixed in this round — the `WORKFLOW_REVIEW_FILE` override assignment is now quoted to avoid word splitting or glob expansion.

<!-- gh-id: 3172384172 -->
#### ↳ cmk ([2026-05-01 06:51 UTC](https://github.com/cmk/agogo/pull/52#discussion_r3172384172))

Fixed in this round — `local_review.sh --check` now verifies `gh` is available and that `scripts/review_path.sh` succeeds, so auth/path failures surface early.

<!-- gh-id: 3172384653 -->
#### ↳ cmk ([2026-05-01 06:51 UTC](https://github.com/cmk/agogo/pull/52#discussion_r3172384653))

Fixed in this round — the extractor header now says the PR body stops at the first review-round marker, matching the implementation.

<!-- gh-id: 3172385409 -->
#### ↳ cmk ([2026-05-01 06:52 UTC](https://github.com/cmk/agogo/pull/52#discussion_r3172385409))

Fixed in this round — README now calls out that `scripts/local_review.sh` requires both the `codex` CLI and authenticated `gh` CLI.
