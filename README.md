[![CI](https://github.com/cmk/agogo/actions/workflows/ci.yml/badge.svg)](https://github.com/cmk/agogo/actions/workflows/ci.yml)
[![Docs](https://github.com/cmk/agogo/actions/workflows/docs.yml/badge.svg)](https://cmk.github.io/agogo/)

# agogo

Rust workspace for agogo. API docs: <https://cmk.github.io/agogo/>.

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

```bash
cargo run -p agogo-cli --features run --bin agogo -- run \
  --source internal \
  --bpm 120 \
  --sr 48000 \
  --ch 'id=three,dev=audio,mode=click,grid=t2t,out=default' \
  --ch 'id=two,dev=audio,mode=click,grid=t2,out=default'
```

That gives you a one-bar 3:2: t2t clicks 3 times per bar, t2 clicks 2 times
per bar, both driven internally at 120 BPM. It uses the default audio output
and runs until Ctrl-C.

If you already have agogo installed, drop the `cargo run ... --bin` prefix:

```bash
agogo run --source internal --bpm 120 --sr 48000 \
  --ch 'id=three,dev=audio,mode=click,grid=t2t,out=default' \
  --ch 'id=two,dev=audio,mode=click,grid=t2,out=default'
```
