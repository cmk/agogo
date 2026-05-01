[![CI](https://github.com/cmk/agogo/actions/workflows/ci.yml/badge.svg)](https://github.com/cmk/agogo/actions/workflows/ci.yml)

# agogo

Rust workspace for agogo.

Agent workflow instructions live in `AGENTS.md`. `CLAUDE.md` is a
compatibility symlink for Claude Code, and Claude-specific commands remain
under `.claude/`.

Before pushing a sprint branch, run the local review transition:

- Claude Code: `/sprint-review`
- Codex or shell: `scripts/local_review.sh` with the `codex` CLI and
  authenticated `gh` CLI available.

Use `scripts/workflow_state.sh` to inspect the current workflow state
before committing, reviewing, pushing, replying to review comments, or
merging.
