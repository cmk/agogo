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
