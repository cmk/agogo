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
1. `.githooks/pre-commit` line 49: change `cargo fmt --all -- --check` to `cargo fmt -p agogo-core -p agogo-cli -- --check` per MEMORY.md.

**Follow-up (not blocking push):**
2. `remote_sha` unused in pre-push loop. Cosmetic.
3. Spot-check table omits `git commit --amend`. Low priority; standard git behavior.
