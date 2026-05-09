# AGENTS.md

`AGENTS.md` is the shared instruction file for Codex, Claude Code, and
other coding agents. `CLAUDE.md` is a compatibility symlink back to this
file. Claude Code-specific commands and settings remain under `.claude/`.

## What this repo is

<!-- Replace this section with your project description. -->

A Rust workspace with multiple crates.

## Architecture

<!-- Replace this section with your architecture overview. -->

```
Cargo.toml              — workspace root
rust-toolchain.toml     — pinned Rust 1.92 + components
rustfmt.toml            — formatter config (edition 2024)
deny.toml               — cargo-deny policy
crates/
  core/                 — shared types, test utilities, proptest strategies
  cli/                  — binary entrypoint
```

Bumping MSRV requires updating three places together: `rust-version` in
`Cargo.toml`, the channel in `rust-toolchain.toml`, and the action ref
in `.github/workflows/ci.yml`.

## Library conventions

- **No unsafe code**: every crate root declares `#![forbid(unsafe_code)]`.
- **Inter-module imports form a DAG** (pre-commit hook).
- **No floating-point types** outside a file-level allowlist
  (pre-commit hook). Allowed uses: PID controller, PCM audio, CLI
  boundary, C++ FFI.
- **Numerical conversions come from a named `Conn`** (or a
  Conn-lookalike with proptested adjoint laws). Bespoke conversions
  are strongly discouraged. See connections README → 'When to use
  connections'.
- **Connection construction uses upstream macros and is total over
  declared types** (pre-commit hook). See connections README.
- **User-reachable invalid input fails at the boundary**, not in
  bridge or scheduler panics (pre-commit hook).
- **Test fixtures are gitignored**; a fresh checkout passes
  `cargo test --workspace` with zero setup. Fixture-dependent tests
  use `fixture_or_skip!` and `return` cleanly when absent — **do
  not** `#[ignore]` them.
- **Property-based testing is mandatory** for any module that parses,
  encodes, or transforms data (`proptest` workspace dev-dep):
  - Strategies are functions returning `impl Strategy`, not `Arbitrary`
    derive. Use `prop_oneof!` with frequency weights to bias toward
    boundaries.
  - **Generator domain = the input type's full domain.** Boundaries
    (`MAX`, `MIN`, `0`, NaN, ±∞) go in explicit `Just(_)` arms with
    elevated frequency. Bounding the generator to keep arithmetic
    "safe" is an anti-pattern — it fakes coverage by hiding the wrap
    region.
  - **Test the test before pushing.** Revert the fix in a dirty
    worktree and re-run; if the proptest still passes, it's
    decorative.
  - **One arb file per top-level module** — `<top>/arb.rs` exposes
    strategies for that layer's types via
    `#[cfg(any(test, feature = "testkit"))] pub mod arb;`.
  - Sprint-blocking properties go in the plan's **Verification**
    table before any code is written. Temporary `#[ignore]` requires
    a Review-section reason and re-enable plan.
- **Modern module layout** (no `mod.rs` — sibling file one level up,
  named after the directory). Document any deviation.

## Repository conventions

### Parallel work

At the start of each conversation, ask: "Are any other agent instances
working in this repo right now?" If yes, a worktree is **mandatory** —
two agents in the same worktree stall on cargo's `target/` lock. Naming:
`../<repo>.plan-YYYY-MM-DD-NN` + branch `plan-YYYY-MM-DD-NN` (TDD step 1).

Verify worktrees aren't sharing `target/` (would happen if
`CARGO_TARGET_DIR` is set or `~/.cargo/config.toml` overrides
`build.target-dir`):
`cargo metadata --format-version 1 --no-deps | jq -r .target_directory`
in each — different paths = safe.

### The gardener rule

Weeds are weeds, regardless of who planted them. Whenever you spot a
violation of any rule below — stale comment, mis-bounded proptest
generator, lying `expect()` string, undocumented `#[ignore]`, stored
`f64` outside the allowlist — flag it even if you didn't write it.

- **Flag** in the plan's `## Review` section: `file:line — rule —
  consequence`.
- **Fix** if local, single-file, no API change, no scope expansion —
  ask the user before merging.
- **Defer** otherwise — name the cleanup specifically enough for the
  next plan branch to pick up.

CI gates (fmt, clippy, gitleaks, the `check_*.sh` scripts) catch their
own drift. The gardener rule covers what lives *below* the gate: prose,
doc links, decorative tests, mis-bounded generators, stale section
headers, panic strings that contradict preconditions, deferred
Verification-table properties.

### Git hooks

Hooks are activated by `git config core.hooksPath .githooks`. Bypass
(`--no-verify`) only when explicitly authorized; CI re-runs the same
gates plus a `gitleaks` history scan as defense-in-depth.

- **Each pushed commit must be green.** `pre-push` runs
  `cargo test --workspace` + `cargo clippy --all-targets -- -D warnings`
  (~50s). Intra-branch commits can be transiently red — pre-push is
  the gate, CI is the source of truth for `origin/main`'s bisect
  property. `pre-commit` runs the cheap chain on every commit:
  `cargo fmt --check`, `check_pii.sh`, `check_floats.sh`,
  `check_layers.sh`, `check_connections.sh`. There's also an agent
  `PreToolUse` layer in `.claude/settings.json` that catches PII /
  float drift on agent-invoked `git commit*` — but use separate
  `git add` and `git commit` calls, since chained `add && commit`
  sees an empty pre-add diff and slips through.
- **CI-repair commits are fixups.** `git commit --fixup=<sha>`, then
  `scripts/git_squash.sh` before push. Review-round commits stay
  standalone.
- **Conventional commits.** Imperative subject < 50 chars. Prefixes:
  `plan`, `feat`, `fix`, `fmt`, `doc`, `test`, `task`, `debt`. Scopes
  allowed (`doc(skills):`, etc.). Sprint-opener is a `plan:` commit
  adding the plan doc.

### Sprint workflow

The sprint workflow is a finite state machine, not a menu. The full
review-round lifecycle and `/pr-watch` loop are diagrammed in
`doc/workflow.md` (the prose here is authoritative if the two
disagree). Identify the current state before committing, running
local review, pushing, replying, or merging — take only the
documented transition. Use `scripts/workflow_state.sh` when the
state isn't obvious.

```
main_clean → on_branch → plan_committed → impl_green → plan_finalized
  → local_reviewed → pushed → gh_review → items_pulled → round_unpushed
  → gh_review → merged
```

Workflow-sensitive actions go through repo scripts/commands:

- Local review: `/pr-review` (Claude Code) or `scripts/pr_review.sh`.
  `/review` is post-push help, **not** the canonical pre-push transition.
- PR body: `scripts/pr_report.py path` / `body`.
- GitHub review ingestion: `scripts/pr_report.py reviews`.
- Replies: `/pr-reply` (wraps `scripts/pr_reply.py` + `pr_report.py reviews`).
- Merge: `scripts/git_merge.sh`, **not** `gh pr merge`.

When a `gh`-backed command errors (auth prompt, network, missing
permission), surface the error — **don't silently fall back** to git
plumbing or MCP tools. They almost always do the wrong thing for
GitHub-side state (PRs, reviews, replies, merges).

### Test-driven development (TDD) workflow

A plan at `doc/plans/plan-YYYY-MM-DD-NN.md` maps to branch
`plan-YYYY-MM-DD-NN` and (optionally) worktree
`../<repo>.plan-YYYY-MM-DD-NN`. One slug, three places. Flat branch
name (no `/`) — agents hit ref-creation errors on slash branches.

1. **Pick the filename.** `ls doc/plans/plan-YYYY-MM-DD-*.md` to find
   the next unused `NN`. No writes yet — main stays clean.
2. **Worktree or branch?** Worktree if another agent is active, else
   user's call. `git worktree add ../<repo>.plan-YYYY-MM-DD-NN -b
   plan-YYYY-MM-DD-NN` or `git switch -c plan-YYYY-MM-DD-NN`.
3. **Write the plan.** The Verification table lists property tests
   that must pass to ship. Commit as `plan: <one-line goal>`.
4. Write proptest properties + test skeletons that compile but fail.
5. Implement until green.
6. Commit on the branch when green.
7. **Finalize sprint docs** in one commit: append Deferred/Review
   sections to the plan; create the review file at
   `$(scripts/pr_report.py path)` with `# PR #<N> — <title>` +
   `## Summary` (the PR body, written for a human reviewer — not a
   ship-report). `review-00000.md` is a protected sentinel; real
   reviews start at `00001`. Commit as `doc: Finalize plan NN and PR
   description`. **Must precede local review.**
8. Run `/pr-review` (or `scripts/pr_review.sh`).
9. Open the PR:
   `gh pr create --body-file <(scripts/pr_report.py body N)`.
10. Rebase + land: `git fetch origin && git rebase origin/main`, then
    `git merge --ff-only`. (Worktree case: main is checked out in
    the *primary* worktree, so run the merge from there.)
11. `git worktree remove ...` (if used), then `git branch -d ...`.

### Code review

#### Tier 1 — Local (pre-push)

Before pushing, run `/pr-review` (or `scripts/pr_review.sh`). It
examines `git diff origin/main...HEAD`, appends a
`## Local review (YYYY-MM-DD)` section, and aborts if the review
file or `## Summary` is missing.

#### Tier 2 — GitHub (post-push)

CI runs tests + clippy. Auto-review agents and Copilot review the PR.

After GitHub review activity:
1. `/pr-report <N>` fetches comments and **appends** them to
   `review-NNNNN.md` (idempotent via `<!-- gh-id: NNNNN -->` markers).
2. Address findings as **uncommitted edits** in the working tree.
3. `/pr-reply <N>` posts replies, mirrors them into the doc, and
   makes ONE atomic commit (code + replies + doc).
4. `git push` once.

**Do not pre-commit the fix** — `/pr-reply` expects to start from
`gh_review` (local at-or-behind origin) and produce the round commit
itself. **Do not merge from `round_unpushed`** — `gh pr merge` is
GitHub-side and silently drops local commits. Use
`scripts/git_merge.sh`, which refuses if the branch is ahead of origin.
If a merge already dropped a round commit, cherry-pick the stranded
SHA into the next plan branch's first commit.

#### Automated poll loop (optional)

`/loop 10m /pr-watch <N>` runs the round cycle on a timer. Each tick
either heartbeats, auto-fixes trivially-clear items + runs the
`/pr-reply` flow + pushes, or pauses on push failure. Auto-fix scope:
one file, < 20 lines, no API removal, no cross-module reasoning —
anything ambiguous is surfaced as **needs you**. The loop **never
merges** — that's the user's manual gate. See `doc/workflow.md` →
*`/pr-watch` dynamic-mode loop* for the per-tick state diagram.

## Sprint plan format

```markdown
# Plan NN — Title

## Goal
One sentence.

## Dependency Graph
T1 → T2, T3 → T4, ...

## Tasks
T1, T2, ... — each with: problem/motivation, solution/approach,
types or API surface.

## Verification

### Properties (must pass)
| Property | Module | Invariant |
|----------|--------|-----------|
| `msg_round_trips` | `crate_foo::codec` | encode then decode recovers original |

### Spot checks
Unit test names + specific assertions.

### Build gates
- cargo build, test, clippy --all-targets — all clean
- End-to-end scenario description

## Deferred
What was intentionally left out, why.

## Review
- Any `#[ignore]`d properties — which, why, re-enablement plan
- Design deviations from the plan
- **Drift caught** (file:line — rule — fixed-here / deferred). See
  the gardener rule.
- Recommendations
```
