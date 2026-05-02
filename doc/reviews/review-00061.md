# PR #61 — Split pre-commit / pre-push by cost

## Summary

The git-side pre-commit chain was charging every `git commit` ~50s
of `cargo test --workspace` + `cargo clippy --all-targets`, even
though only the pushed state needs to be green for CI / bisect
purposes. On a 5-commit feature branch that's 4+ minutes of dead
wall time per branch.

This PR splits the chain by event:

- **`.githooks/pre-commit`** keeps the cheap, deterministic checks:
  `cargo fmt --check`, `scripts/check-pii.sh`,
  `scripts/check-floats.sh`, `scripts/check-layers.sh`. Sub-second
  combined.
- **`.githooks/pre-push`** (new) runs `cargo test --workspace` +
  `cargo clippy --all-targets -- -D warnings` once per push. Reads
  git's stdin contract (`<local-ref> <local-sha> <remote-ref>
  <remote-sha>`) and short-circuits with `exit 0` if no refs are
  being pushed (delete-only / no-op pushes don't pay the cost).

Measured locally on this branch: pre-commit dropped from ~52s to
**1.83s**; pre-push runs the full suite in **3.83s** on a warm
cache. Cold-cache pre-push is the historical ~50s — paid once per
push instead of once per commit.

`AGENTS.md` is updated to describe the three-layer hook split
(`PreToolUse` agent-side + `pre-commit` cheap + `pre-push`
expensive) and to weaken the per-commit-green invariant to a
per-push-green invariant. The autosquash workflow already
accommodates intra-branch commits that weren't green at the moment
of recording, so this matches existing reality.

Activation is unchanged: `git config core.hooksPath .githooks`
covers both hooks.

Stdio-core has the same hook chain and will get the same split in
a follow-up sprint.

## Local review (2026-05-02)

**Branch:** plan/2026-05-02-07
**Commits:** 3 (origin/main..plan/2026-05-02-07)
**Reviewer:** Claude (sonnet, independent)

---

### Commit Hygiene

Three commits: `plan:` opener, a single `feat:` implementing T1+T2+T3 together, and the `doc:` finalizer. The plan's dependency graph explicitly serializes T1 → T2 → T3, and they landed in one commit rather than three — this is fine; the plan itself notes it. Subjects are under 72 chars, present-tense imperative, correct prefixes. No issues.

### Code Quality

**`set -euo pipefail` + `while read` loop in pre-push.** Verified safe. Bash exempts the `while read` loop-condition expression from `set -e`; `read` initializes all named variables (assigning empty when fields are missing), so `set -u` is not triggered by the trailing `remote_sha`. The `(delete) 0000…0000 …` form correctly falls through the `local_sha != zero_sha` guard.

**`cargo fmt --all -- --check` scope drift.** `.githooks/pre-commit` line 49 runs `cargo fmt --all -- --check`. Per MEMORY.md `[cargo fmt scope]`, this reformats (or, in --check mode, fails on) the sibling `connections` path-dep. The correct scope is `cargo fmt -p agogo-core -p agogo-cli -- --check`. Pre-existing drift, but the gardener rule applies because this PR edits the file. **Fixed in the round commit below.**

**`remote_sha` is read but never used** in the pre-push loop body. Cosmetic; the trailing variable acts as a "discard remainder" idiom for `read`. Deferred.

### Test Coverage

Shell-script sprint; proptests do not apply. The plan's spot-check table is adequate. The one explicitly un-exercised case (delete-only push) is acknowledged with a brief justification, which is acceptable.

### Plan Conformance

T1, T2, and T3 are fully implemented. AGENTS.md's three-layer description matches the hook files and the plan's task specs. The conventions bullet at AGENTS.md:151 is updated correctly. No drift between plan and implementation.

### Risks

The weakened invariant (intra-branch transient red OK) accurately describes the autosquash workflow. The structural commit-atomicity rule ("don't commit a library module without the tests that cover it") is preserved verbatim. `git push --force-with-lease` and `git push origin :branch` (delete via colon) both fire the hook normally and correctly fall through the zero-SHA guard.

### Recommendations

**Must fix before push:**
1. ~~`.githooks/pre-commit` line 49: change `cargo fmt --all -- --check` to `cargo fmt -p agogo-core -p agogo-cli -- --check` per MEMORY.md.~~ **Resolved** in commit `3b6240b` (round commit on this branch).

**Follow-up (not blocking push):**
2. `remote_sha` unused in pre-push loop. Cosmetic.
3. Spot-check table omits `git commit --amend`. Low priority; standard git behavior.

<!-- gh-id: 3176477592 -->
### Copilot on [`AGENTS.md:659`](https://github.com/cmk/agogo/pull/61#discussion_r3176477592) (2026-05-02 09:59 UTC)

The documentation for Layer 2 is now stale: this list still says pre-commit runs `cargo fmt --all`, but the hook in `.githooks/pre-commit` was changed to the narrower `cargo fmt -p agogo-core -p agogo-cli -- --check`. Leaving the old command here will send contributors back to the repo-wide fmt scope that this PR is explicitly trying to avoid.

<!-- gh-id: 4214815489 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-05-02 09:59 UTC](https://github.com/cmk/agogo/pull/61#pullrequestreview-4214815489))

## Pull request overview

This PR splits the local Git hook pipeline so cheap checks still run on every commit while the expensive workspace test/lint suite runs once per push. In this codebase, that change is meant to preserve the repo’s “green before publication” workflow while reducing the per-commit cost during normal branch iteration.

**Changes:**
- Trim `.githooks/pre-commit` down to the fast deterministic checks and scope the fmt check to the workspace crates.
- Add a new `.githooks/pre-push` hook that runs `cargo test --workspace` and `cargo clippy --all-targets -- -D warnings`, skipping delete-only/no-op pushes.
- Update project documentation and sprint/review records to describe the new pre-commit / pre-push split and the revised “each pushed commit is green” convention.

### Reviewed changes

Copilot reviewed 5 out of 5 changed files in this pull request and generated 3 comments.

<details>
<summary>Show a summary per file</summary>

| File | Description |
| ---- | ----------- |
| `doc/reviews/review-00061.md` | Adds the PR review record summarizing the hook split and local review outcome. |
| `doc/plans/plan-2026-05-02-07.md` | Adds the sprint plan describing the motivation, tasks, verification, and review notes for the hook split. |
| `AGENTS.md` | Updates contributor guidance and repository conventions to reflect the new hook layering and per-push green invariant. |
| `.githooks/pre-push` | Introduces the new expensive push-time gate for tests and clippy. |
| `.githooks/pre-commit` | Removes the expensive checks, keeps the fast checks, and narrows the fmt scope. |
</details>






<!-- gh-id: 3176477602 -->
### Copilot on [`doc/plans/plan-2026-05-02-07.md:39`](https://github.com/cmk/agogo/pull/61#discussion_r3176477602) (2026-05-02 09:59 UTC)

This plan still records `cargo fmt --all -- --check` as both the intended pre-commit command and a passing build gate, but the implementation was updated in this PR to use the narrower package-scoped fmt command. Because the Review section later says that drift was fixed, these earlier sections now contradict the final state of the branch.


<!-- gh-id: 3176477607 -->
### Copilot on [`doc/reviews/review-00061.md:58`](https://github.com/cmk/agogo/pull/61#discussion_r3176477607) (2026-05-02 09:59 UTC)

This review record contradicts the final code: it says `.githooks/pre-commit` still runs `cargo fmt --all -- --check` and even lists that as a must-fix item, but the hook in this PR has already been updated to the package-scoped fmt command. Since this file is meant to capture the completed review outcome, it should be updated to reflect that the issue was resolved instead of leaving a stale blocker in the final document.

<!-- gh-id: 3176491423 -->
#### ↳ cmk ([2026-05-02 10:16 UTC](https://github.com/cmk/agogo/pull/61#discussion_r3176491423))

Fixed — AGENTS.md:659 now lists the scoped command and explains the reason inline. Same edit applied to the run-it-yourself instruction one paragraph below.

<!-- gh-id: 3176491512 -->
#### ↳ cmk ([2026-05-02 10:16 UTC](https://github.com/cmk/agogo/pull/61#discussion_r3176491512))

Fixed — T1's 'Keep' list and the build-gates entry both now show the scoped command. The Drift entry in the Review section already documents the original-vs-fixed transition.

<!-- gh-id: 3176491616 -->
#### ↳ cmk ([2026-05-02 10:16 UTC](https://github.com/cmk/agogo/pull/61#discussion_r3176491616))

Fixed — Recommendations item 1 is now struck-through and annotated **Resolved** with the round-commit SHA (3b6240b). The body of the Code Quality finding already noted the fix; the contradiction was in the trailing list.
