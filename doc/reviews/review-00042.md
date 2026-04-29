# PR #42 — Restructure review-round FSM: drop replies_amended, single atomic round commit

## Summary

PR #41 closed three documented gaps in the review-round workflow but
left the underlying FSM unchanged: `gh_review → items_pulled →
fix_unpushed → replies_amended → gh_review`. Two transient pre-push
states meant two opportunities for a merge to silently drop work.
PR-A (PR #41) papered the trap with a runtime guard
(`scripts/safe_merge.sh`) and clearer reporting. PR-B (this one) is
the structural fix: collapse the two states into one, remove the
`--amend` step entirely, and make the single round commit atomic by
construction.

### New FSM

Was:
```
gh_review → items_pulled → fix_unpushed → replies_amended → gh_review
                              ↑                ↑
                        local fix commit   /reply-reviews amends
```

Now:
```
gh_review → items_pulled → round_unpushed → gh_review
                              ↑
        edit working tree + /reply-reviews (post + mirror + atomic commit)
```

`replies_amended` is gone. `fix_unpushed` is gone. The sole pre-push
state is `round_unpushed`, which is a fully-formed atomic commit
containing **both** the code fix and the mirrored reply doc — built
in one shot by the new `/reply-reviews` flow.

### New `/reply-reviews` order

1. Apply fix edits to the working tree (uncommitted).
2. Step 0 preconditions (PR resolves; branch at `gh_review`, i.e. no
   unpushed commits).
3. Step 1: refresh the review doc.
4. Steps 2–3: identify unreplied threads, compose replies.
5. Step 4: post replies via `reply_review.py`. Failures abort the
   run before mirror+commit.
6. Step 5: mirror via `pull_reviews.py`.
7. Step 6: `git add -A && git commit` — one commit captures the
   whole round.

The pre-commit hook still runs at Step 6; failures leave the working
tree dirty and replies on GitHub. Recovery is a re-run of
`/reply-reviews` (Step 1 mirrors the already-posted replies; Step 2's
filter dedupes; Step 6 retries the commit).

### `/watch-pr` follows the same shape

- Step 3 applies auto-fix edits but no longer commits.
- Step 4 posts replies, mirrors, and commits atomically (the single
  round commit, prefixed `fix:` or `doc:` depending on whether code
  changed).
- Step 5 report names the new state (`round_unpushed`).

### Atomicity guarantee

The trap that bit PRs #38 and #40 — local `replies_amended` commit
silently dropped on merge — is now structurally impossible. There is
exactly one pre-push commit per round; either it's pushed (state
`gh_review`) or it isn't (state `round_unpushed`). `safe_merge.sh`
still guards the boundary (kept as defense-in-depth), but the FSM
itself no longer has a state where the merge can desynchronize from
the local round.

### Files changed

- `doc/workflow.md` — FSM diagram, legend, and recovery sections
  rewritten for the new state shape.
- `.claude/commands/reply-reviews.md` — Step ordering rewritten
  (fix-edits-first instead of fix-commit-first; atomic Step 6 commit;
  no `--amend`).
- `.claude/commands/watch-pr.md` — Step 3 no longer commits; Step 4
  does atomic commit; Step 5 report uses new state name.
- `.claude/commands/pull-reviews.md` — phrasing updated ("rides with
  the next round commit" instead of "next fix commit").
- `CLAUDE.md` — Two-tier review section rewritten for the new flow.
- `scripts/safe_merge.sh` — comments updated to reference
  `round_unpushed` instead of the now-removed `replies_amended`.

### Verification

| Check | Result |
|---|---|
| `cargo test --workspace --all-features` | unchanged (no Rust touched) |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | unchanged |
| `scripts/check-floats.sh` | unchanged |
| `grep -rn 'replies_amended\|fix_unpushed' --include='*.md' --include='*.sh'` | only in PR #41's review file (historical) and explanatory backreferences in the new `/reply-reviews.md` and `/watch-pr.md` |

### Migration notes for in-flight PRs

Any open PR currently at the old `replies_amended` state (i.e., a
local amended fix commit awaiting push) should:

1. Push the existing commit. That advances to `gh_review` under the
   new FSM (which is the same edge as the old FSM — push is push).
2. From there, the next review round uses the new flow: edits in
   working tree, run `/reply-reviews`, single atomic commit.

No history rewrites needed. The FSM change is forward-compatible.

## Companion change: `/sprint-review` autonomy

The original `/sprint-review` Step 5 ended with **"Stop. Do not push.
Do not offer to fix the issues. The user reads the review and
decides what to do next."** This forced the user back into the
agent loop after every reviewer pass, even for items that fit the
established `/watch-pr` auto-fix bucket (one file, <~20 lines, no
API changes, no cross-module reasoning).

This PR removes that gate. New `/sprint-review` Step 6 triages the
reviewer's must-fix and small-scope follow-up items into:

- **auto** — applied autonomously, committed as
  `fix:`/`debt:`/`test:`/`doc:` (whichever matches the predominant
  nature of the fixes).
- **needs-user** — surfaced in the Step 7 report; not auto-applied.

When in doubt, classify as **needs-user**. One pass of auto-fixes is
the contract — the agent doesn't recursively re-run `/sprint-review`
against the new tip. The user can re-run for another pass if they
want one.

Files in this companion change:
- `.claude/commands/sprint-review.md` — Step 5 hand-off replaced
  with Steps 6 (triage + apply) + 7 (state-named report).

<!-- gh-id: 3157887501 -->
### Copilot on [`.claude/commands/reply-reviews.md:180`](https://github.com/cmk/agogo/pull/42#discussion_r3157887501) (2026-04-28 23:54 UTC)

In the doc-only round path, the snippet switches to a `doc:` commit message but no longer explicitly includes the staging step. Since Step 6’s instruction is to stage everything (`git add -A`) first, consider keeping that explicit in the doc-only snippet too, otherwise it reads like `git commit` may run with nothing staged.

<!-- gh-id: 3157887514 -->
### Copilot on [`.claude/commands/reply-reviews.md:206`](https://github.com/cmk/agogo/pull/42#discussion_r3157887514) (2026-04-28 23:54 UTC)

The Step 7 report bullet mentions "no commit" while also saying the mirror is "staged for next round". If the working tree is clean post-mirror, there’s nothing to stage and the FSM state would remain `gh_review` (not `round_unpushed`). Clarify the intended no-op behavior (e.g., when there were no unreplied threads / no new mirror output) and what state the user should consider themselves in.

<!-- gh-id: 3157887531 -->
### Copilot on [`.claude/commands/watch-pr.md:132`](https://github.com/cmk/agogo/pull/42#discussion_r3157887531) (2026-04-28 23:54 UTC)

Step 4’s command block always runs `git add -A` and `git commit`, but the Step 5 report includes a "no commit" branch (all items were `ask`). If there are no replies to post and `pull_reviews.py` produced no new doc delta, `git commit` will fail with "nothing to commit". Consider making the commit conditional on there being staged changes, and aligning the Step 4 instructions with the Step 5 reporting branches.
```suggestion
# Single atomic commit: code edits (if any) + mirrored doc.
# If there is no code/doc delta this round, do not create a commit.
git add -A
if ! git diff --cached --quiet; then
  git commit -m "fix: Address review feedback on PR #<N>"   # or doc: if no code edits
else
  echo "No staged changes; no commit for this round."
fi
```

<!-- gh-id: 4193269298 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-04-28 23:54 UTC](https://github.com/cmk/agogo/pull/42#pullrequestreview-4193269298))

## Pull request overview

Updates the repo’s documented review-round workflow to structurally remove the pre-push `fix_unpushed → replies_amended` split and replace it with a single `round_unpushed` “atomic round commit” state, aligning `/reply-reviews` and `/watch-pr` with the new FSM shape.

**Changes:**
- Rewrite the FSM in `doc/workflow.md` to collapse the two transient pre-push states into `round_unpushed`, and add updated recovery guidance.
- Reorder and re-spec `/reply-reviews` and `/watch-pr` to post replies → mirror → commit exactly once (no `--amend`), then push.
- Refresh supporting docs/comments (CLAUDE workflow section, pull-reviews phrasing, safe-merge commentary) and add the PR review record.

### Reviewed changes

Copilot reviewed 7 out of 7 changed files in this pull request and generated 4 comments.

<details>
<summary>Show a summary per file</summary>

| File | Description |
| ---- | ----------- |
| `scripts/safe_merge.sh` | Updates commentary to reference `round_unpushed` instead of `replies_amended`. |
| `doc/workflow.md` | Updates FSM diagram + legend and adds recovery notes for the new single pre-push state. |
| `doc/reviews/review-00042.md` | Adds the PR’s review record describing the workflow/FSM restructure. |
| `CLAUDE.md` | Updates the Tier 2 workflow prose to match “edits first, then /reply-reviews makes the atomic commit.” |
| `.claude/commands/watch-pr.md` | Updates `/watch-pr` steps to avoid intermediate commits and to produce one atomic round commit in Step 4. |
| `.claude/commands/reply-reviews.md` | Updates `/reply-reviews` contract to start from `gh_review` and create a single atomic commit (no amend). |
| `.claude/commands/pull-reviews.md` | Adjusts wording so the review file “rides with the next round commit” (atomic). |
</details>



<details>
<summary>Comments suppressed due to low confidence (1)</summary>

**scripts/safe_merge.sh:10**
* The comment still says "a fix commit" can be unpushed, but the rest of the file and the updated FSM terminology refer to a single unpushed *round commit* (`round_unpushed`). Consider renaming this phrasing to avoid mixing the old and new state model.
```
# merged`. But `gh pr merge` is a GitHub-side operation; it doesn't
# know about local state. Merging while a fix commit sits unpushed on
# the local branch silently drops it on the floor — the merge takes
# the remote head, and the local commit stays orphaned in the reflog.
```
</details>



<!-- gh-id: 3157887551 -->
### Copilot on [`doc/workflow.md:41`](https://github.com/cmk/agogo/pull/42#discussion_r3157887551) (2026-04-28 23:54 UTC)

nit: In the `round_unpushed` legend / recovery text, references to `pull_reviews.py` are a bit ambiguous given the actual script path is `scripts/pull_reviews.py` elsewhere in the repo, and the "single shot" description omits the initial refresh (`scripts/pull_reviews.py` run before composing replies) that the later recovery section depends on. Consider tightening the wording to match the documented `/reply-reviews` step order and script name.
```suggestion
  unpushed on the local branch. `/reply-reviews` produces it in one
  flow: refresh via `scripts/pull_reviews.py` → post replies →
  refresh again to mirror via `scripts/pull_reviews.py` → `git add
  -A && git commit`. There is no `--amend` step and no prior fix
```

<!-- gh-id: 3157907771 -->
#### ↳ cmk ([2026-04-29 00:00 UTC](https://github.com/cmk/agogo/pull/42#discussion_r3157907771))

Fixed — doc-only branch now lives inside the same `git add -A` + conditional commit block. Step 6 has one staging step that fires regardless of whether the round is code+doc or doc-only.

<!-- gh-id: 3157908157 -->
#### ↳ cmk ([2026-04-29 00:00 UTC](https://github.com/cmk/agogo/pull/42#discussion_r3157908157))

Fixed — Step 7 now has two explicit terminal shapes. The "no commit" path explicitly states the branch stays at `gh_review` (no state change), not `round_unpushed`. The misleading "staged for next round" wording is gone.

<!-- gh-id: 3157908572 -->
#### ↳ cmk ([2026-04-29 00:00 UTC](https://github.com/cmk/agogo/pull/42#discussion_r3157908572))

Fixed — `/watch-pr` Step 4 now uses `if git diff --cached --quiet` to skip the commit when nothing is staged (same shape as your suggestion, slightly fewer lines). Step 5 report keeps the two-branch shape; the Step 4 commit branch is now consistent with it.

<!-- gh-id: 3157909038 -->
#### ↳ cmk ([2026-04-29 00:00 UTC](https://github.com/cmk/agogo/pull/42#discussion_r3157909038))

Fixed — `round_unpushed` legend now uses full `scripts/pull_reviews.py` / `scripts/reply_review.py` paths and spells out the full flow (refresh → identify → post → refresh → commit) so the legend matches `/reply-reviews`'s actual step order. Adopted your suggested phrasing.
