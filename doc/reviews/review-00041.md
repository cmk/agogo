# PR #41 — Workflow gaps: merge guard, FSM-named round report, recovery section

## Summary

The two-tier review workflow has an asymmetry: `/reply-reviews`
enforces "fix commit must be unpushed" before mirroring (so the
amend-into-fix-commit step is safe), but there is no symmetric
enforcement at the merge boundary. The FSM in `doc/workflow.md`
shows merging starts from `gh_review` (push complete) — there is no
`replies_amended → merged` edge — but `gh pr merge` is GitHub-side
and doesn't see local state. PRs #38 and #40 both lost their
round-2 fix commits this way: the amend happened, the user merged,
the unpushed local commit silently went into the reflog.

This PR closes three documented-gap fixes (FSM unchanged):

1. **`scripts/safe_merge.sh`** — local-side merge guard. Refuses to
   invoke `gh pr merge` when the current branch is ahead of its
   remote tracking ref. Forwards all arguments to `gh pr merge` once
   the guard passes. The single line of defense against the
   `replies_amended → merged` trap.

2. **State-named `/watch-pr` Step 5 report.** The previous template
   said `round complete / next step: review the commit, then git
   push` which read as "merge-ready" while the FSM state was
   actually `replies_amended` (mid-cycle). New template names the
   state explicitly: `paused at replies_amended (fix unpushed) /
   next step: git push to advance to gh_review (mergeable). DO NOT
   merge from replies_amended`. The "no commit needed" branch is
   broken out separately and reports `gh_review` (mergeable).

3. **`doc/workflow.md` legend + recovery section.** Adds a "Never
   merge from `replies_amended`" bullet to the legend pointing at
   `safe_merge.sh`, and a short Recovery section that documents the
   "fold the stranded fix into the next plan branch" pattern.
   `/reply-reviews` Step 7 and `CLAUDE.md`'s Two-tier section pick
   up matching safe_merge.sh pointers so the convention surfaces
   wherever the merge step appears.

A follow-up PR (separate from this one) will restructure the FSM to
remove `replies_amended` entirely: post-and-mirror replies before
making the fix commit, so the round produces a single
all-inclusive commit and the trap can't exist by construction.
That's a structural change; this PR is the immediate documented-gap
fix.

### Verification

| Check | Result |
|---|---|
| `scripts/safe_merge.sh` no args | usage exit 64 |
| `scripts/safe_merge.sh 999999` (no remote ref) | refuses, exit 1, points to `git push` |
| `scripts/safe_merge.sh` (branch behind / equal) | forwards to `gh pr merge` |
| `cargo test --workspace --all-features` | unchanged (no Rust touched) |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | unchanged |
| `scripts/check-floats.sh` | unchanged |

### Files changed

- `scripts/safe_merge.sh` (new, executable)
- `.claude/commands/watch-pr.md` (Step 5 report template)
- `.claude/commands/reply-reviews.md` (Step 7 hand-off)
- `doc/workflow.md` (legend + Recovery section)
- `CLAUDE.md` (Two-tier review workflow merge-guard note)

<!-- gh-id: 3157817955 -->
### Copilot on [`scripts/safe_merge.sh:33`](https://github.com/cmk/agogo/pull/41#discussion_r3157817955) (2026-04-28 23:32 UTC)

The guard compares HEAD against `origin/$branch`, but the script forwards arbitrary `gh pr merge` args (e.g., a PR number). If someone runs `scripts/safe_merge.sh 17` while *not* on the PR’s head branch, the check will validate the wrong branch and can still allow a merge that strands an unpushed fix commit on the actual PR branch. Consider either (a) allowing the no-arg form and encouraging merges only from the checked-out PR branch, or (b) resolving the PR head ref via `gh pr view` and comparing that ref to the current branch before merging.

<!-- gh-id: 4193179990 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-04-28 23:32 UTC](https://github.com/cmk/agogo/pull/41#pullrequestreview-4193179990))

## Pull request overview

Closes workflow/documentation gaps in the two-tier review process by adding a local merge guard, clarifying state/mergeability reporting, and documenting recovery when a merge happens from the wrong FSM state.

**Changes:**
- Add `scripts/safe_merge.sh` to refuse `gh pr merge` when the current branch is ahead of `origin/<branch>`.
- Update `/watch-pr` and `/reply-reviews` templates to explicitly name FSM states and warn against merging from `replies_amended`.
- Extend `doc/workflow.md` (legend + Recovery section) and `CLAUDE.md` to document the merge guard and the stranded-commit recovery pattern.

### Reviewed changes

Copilot reviewed 6 out of 6 changed files in this pull request and generated 2 comments.

<details>
<summary>Show a summary per file</summary>

| File | Description |
| ---- | ----------- |
| scripts/safe_merge.sh | New merge wrapper that blocks merges when local branch is ahead of origin. |
| doc/workflow.md | Adds “Never merge from replies_amended” guidance and a recovery recipe. |
| doc/reviews/review-00041.md | New PR review record capturing motivation, verification, and file list. |
| .claude/commands/watch-pr.md | Step 5 report template updated to name FSM states and mergeability. |
| .claude/commands/reply-reviews.md | Step 7 handoff updated to name FSM state and point to safe merge wrapper. |
| CLAUDE.md | Two-tier workflow section updated with merge-guard warning and recovery note. |
</details>






<!-- gh-id: 3157817980 -->
### Copilot on [`doc/reviews/review-00041.md:39`](https://github.com/cmk/agogo/pull/41#discussion_r3157817980) (2026-04-28 23:32 UTC)

This summary mentions `feedback_bundle_pr_nits.md`, but that file doesn’t appear to exist in the repo (no matches). If the lore lived in a different doc, update the reference/path; otherwise consider removing this claim to avoid a dead reference in the review record.
```suggestion
   "fold the stranded fix into the next plan branch" pattern
   explicitly. `/reply-reviews` Step 7 and `CLAUDE.md`'s Two-tier
   section pick up matching safe_merge.sh pointers so the
   convention surfaces wherever the merge step appears.
```

<!-- gh-id: 3157831017 -->
#### ↳ cmk ([2026-04-28 23:36 UTC](https://github.com/cmk/agogo/pull/41#discussion_r3157831017))

Fixed in a6d6e73 — guard now resolves the PR head ref via `gh pr view --json headRefName --jq .headRefName` and compares the local copy of THAT branch (not the currently-checked-out branch) to origin. So `safe_merge.sh 41` from main correctly checks `plan/2026-04-28-09`. Verified end-to-end: ran `safe_merge.sh 41` while ahead by one commit, got the REFUSING TO MERGE message naming the right branch.

<!-- gh-id: 3157831517 -->
#### ↳ cmk ([2026-04-28 23:36 UTC](https://github.com/cmk/agogo/pull/41#discussion_r3157831517))

Fixed in a6d6e73 — dropped the dead `feedback_bundle_pr_nits.md` parenthetical from the review summary. The pattern itself is documented in `doc/workflow.md`'s new Recovery section, which is the authoritative repo home for it.
