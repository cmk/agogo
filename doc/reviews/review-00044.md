# PR #44 — Add git-side pre-commit hook + flip cargo fmt to blocking

## Summary

Mirrors template-rust's two-PR sequence (`#10` adds the git-side
hook; `#11` flips fmt to blocking) into agogo as a single PR — the
warn-only fmt + missing safety-net are both holdovers from the same
era, so splitting them would mean shipping a `.githooks/pre-commit`
with warn-only fmt only to flip it minutes later.

### What changes

1. **New `.githooks/pre-commit`** (Layer 2 safety net). Fires at
   git's standard hook point (after staging, before commit object
   creation), so it sees the actual staged content regardless of how
   the commit was invoked — chained `git add && git commit`,
   terminal commits, IDE commits, all paths. The Claude Code
   `PreToolUse` hook (Layer 1) has a documented bypass via chained
   Bash (tracked as `cmk/template-rust#8`); this layer is the
   unbypassable safety net.

2. **`.claude/settings.json` fmt step flipped to blocking.** Drops
   the `{ … || echo '[warn] …'; }` wrapper so fmt drift aborts the
   commit instead of logging a warning and proceeding. Both layers
   now run identical chains; both block on fmt.

3. **`CLAUDE.md` Pre-commit section rewritten.** Documents the
   two-layer model, the `git config core.hooksPath .githooks`
   bootstrap, and the new blocking-everywhere policy. The check
   chain is now five steps (fmt, check-pii, check-floats, test,
   clippy) — same content, blocking semantics across all five.

### Layered design

| Layer | Path | Fires on | Bypass risk |
|---|---|---|---|
| 1 (Claude Code) | `.claude/settings.json` `PreToolUse` | agent-invoked Bash matching `git commit*` | chained `git add && git commit`, terminal commits, IDE commits |
| 2 (git) | `.githooks/pre-commit` | every commit, regardless of source | `--no-verify` only |

Layer 1 stays for fast feedback during agent iteration (no need to
invoke git). Layer 2 is the unbypassable safety net at commit time.

### Bootstrap (one-time per clone)

```
git config core.hooksPath .githooks
```

### End-to-end test

This PR's two commits were made with `core.hooksPath` already
pointing at `.githooks`. Both layers ran cleanly:

- Commit 1 (`fmt: cargo fmt --all over pre-existing drift`) — the
  hook ran `cargo fmt --check`, which passed because I'd just run
  `cargo fmt --all` to clean up seven files of pre-existing drift.
  This is a real demonstration that the warn-only policy was
  letting drift accumulate on main; the new blocking policy closes
  that drift channel.
- Commit 2 (this PR's policy change) — the hook ran on the very
  commit that flips its fmt step to blocking. Drift had already
  been cleaned up in commit 1, so the strict check passed.

### Files changed

- `.githooks/pre-commit` (new, executable) — Layer 2 safety net
  (fmt blocking, check-pii, check-floats, cargo test, cargo
  clippy).
- `.claude/settings.json` — drop the warn wrapper around `cargo fmt
  --check`.
- `CLAUDE.md` — Pre-commit section rewritten for two-layer
  model + blocking policy + bootstrap step.

### Why now (motivation)

A real CI fmt failure recently slipped through local tooling in a
sibling project (`connections`, MR !39). The warn-only wrapper
noticed the drift but let the commit proceed; CI then rejected it
on push, costing an extra round-trip. This PR follows that signal
back upstream — agogo had the same warn-only pattern and the same
exposure to drift on main (live-demonstrated by commit 1 of this
PR, which had to clean up seven files before the new hook would
let any commit through).

The fmt-blocking flip was already shipped in `connections`
(`44b40f3`, MR !40) and `template-rust` (`#10` + `#11`). This PR
brings agogo in line with both.
