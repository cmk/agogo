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
