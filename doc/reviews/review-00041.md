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
   "fold the stranded fix into the next plan branch" pattern (it
   was lore in `feedback_bundle_pr_nits.md` until now). `/reply-reviews`
   Step 7 and `CLAUDE.md`'s Two-tier section pick up matching
   safe_merge.sh pointers so the convention surfaces wherever the
   merge step appears.

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
