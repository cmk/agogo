# AGENTS.md

`AGENTS.md` is the shared instruction file for Codex, Claude Code, and
other coding agents. `CLAUDE.md` is a compatibility symlink back to this
file. Claude Code-specific commands and settings remain under `.claude/`.

## What this repo is

<!-- Replace this section with your project description. -->

A Rust workspace with multiple crates.

## Parallel work

At the start of each conversation, ask the user:
"Are any other agent instances working in this repo right now?"

If yes, a worktree is **mandatory** — see the TDD workflow's Step 1
for the naming convention (`../<repo>.plan-YYYY-MM-DD-NN` + branch
`plan/YYYY-MM-DD-NN`).

Never run two agent instances in the same worktree. Cargo takes a
file lock on `target/` during each build, so concurrent builds stall
behind each other ("Blocking waiting for file lock"). Separate
worktrees each get their own `target/` and sidestep the lock —
**unless** `CARGO_TARGET_DIR` is exported in your shell or
`~/.cargo/config.toml` sets `[build] target-dir`, either of which
forces every worktree to share one directory and reintroduces the
lock. Verify with `cargo metadata --format-version 1 --no-deps | jq
-r .target_directory` in two worktrees — different paths = safe.

## Workflow Is a State Machine

The TDD and review workflow is a finite state machine, not a menu of
roughly-equivalent steps. Before committing, running local review,
pushing, replying to review comments, or merging, agents must identify
the current state and take only the documented transition out of it.
Use `scripts/workflow_state.sh` as a read-only state check when the
state is not obvious.

The intended path is:

```
main_clean
  -> on_branch
  -> plan_committed
  -> impl_green
  -> plan_finalized
  -> local_reviewed
  -> pushed
  -> gh_review
  -> items_pulled
  -> round_unpushed
  -> gh_review
  -> merged
```

Do not skip, reorder, or replace a transition with an ad hoc command
that merely looks equivalent. Use the repo scripts and commands for
workflow-sensitive actions:

- Local review: Claude Code uses `/pr-review`; Codex and shell
  users use `scripts/pr_review.sh`. Claude Code's built-in
  `/review [PR]` is optional post-push review help, not the canonical
  pre-push transition.
- PR body pathing: `scripts/pr_report.py path` and
  `scripts/pr_report.py body`.
- GitHub review ingestion: `scripts/pr_report.py reviews`.
- Review replies: `/pr-reply` or the underlying
  `scripts/pr_reply.py` + `scripts/pr_report.py reviews` flow.
- Merge: `scripts/git_merge.sh`, not raw `gh pr merge`.

## Architecture

<!-- Replace this section with your architecture overview. -->

### Workspace layout

```
Cargo.toml              — workspace root
rust-toolchain.toml     — pinned Rust version (1.92) + components for local and CI
rustfmt.toml            — formatter config (edition 2024)
deny.toml               — cargo-deny license/advisory/source policy
crates/
  core/                 — shared types, test utilities, proptest strategies
  cli/                  — binary entrypoint; feature-gates optional lib crates
```

The active Rust toolchain is pinned via `rust-toolchain.toml`; `rustup`
reads it automatically when you `cd` into the repo, and CI installs
the same channel via `dtolnay/rust-toolchain@1.92.0`. Bumping MSRV
means updating `rust-version` in `Cargo.toml`, the channel in
`rust-toolchain.toml`, and the action ref in `.github/workflows/ci.yml`
together.

Feature flags on the binary crate's `Cargo.toml` control which library
crates are compiled in:

```toml
[features]
default = ["core"]
core = ["dep:project-core"]
```

## The gardener rule — flag and fix drift inline

Weeds are weeds, regardless of who planted them. Whenever an agent
encounters a violation of any rule in this file — stale comment
referencing a renamed identifier, proptest generator bounded to
dodge a wrap, `expect()` string that lies about its precondition,
`#[ignore]`d test without a re-enablement plan, stored `f64` outside
the allowlist, missing `tbase_le_*` proptest the verification table
mandated, etc. — it does not get to silently walk past because "I
didn't write that."

**Minimum bar:**

- **Flag it.** List the violation in the plan's `## Review` section:
  `file:line — rule — one-sentence consequence`.
- **Offer to fix the minor ones.** Local, single-file, no API
  change, doesn't expand sprint scope: ask the user in chat before
  merging. ("I noticed `foo/bar.rs:42` still says `old_name`; fold
  the fix into this branch?") The user decides.
- **Defer the rest with a tracking note.** If the fix is too big to
  absorb in the current sprint, the `## Review` entry IS the plan:
  name the cleanup specifically enough that the next plan branch
  can pick it up.

**What does NOT need surfacing.** Drift CI already catches: `cargo
fmt --check`, `cargo clippy --all-targets -- -D warnings`,
`gitleaks`, `scripts/check_floats.sh`, `scripts/check_pii.sh`. The
gate is the safety net for those.

**What MUST be surfaced.** Anything that lives below the CI gate
because compilation and clippy are blind to it: stale prose, doc
links to renamed-away identifiers, decorative tests that pass
trivially, mis-bounded generators that fake coverage, section
headers that name the old thing, `expect()` panic strings that
contradict the actual precondition, deferred properties from a
previous plan's Verification table that never got written. Those
are exactly the weeds humans don't notice on a fast skim.

This rule applies to every agent — `feat:`, `debt:`, `fix:`, the
review agents, `/pr-watch` auto-fix. A `feat:` agent that walks past
a stale comment in the file it's editing plants a weed that sprouts
three sprints later, when somebody trusts the comment and writes
code based on it.

## Repository conventions

- **Each pushed commit must leave the repo green** (`cargo test --workspace`
  + `cargo clippy --all-targets -- -D warnings`). Don't commit a library
  module without the tests that cover it in the same commit. Intra-branch
  commits can be transiently red between `git commit` and `git push` — the
  pre-push hook (`.githooks/pre-push`) is the gate, and the autosquash
  workflow already accommodates fixup commits that weren't green at the
  moment of recording. What matters for `origin/main`'s bisect property is
  the pushed state, which is what CI verifies.
- **No merge commits.** Always rebase onto main — never `git merge`. The
  history must be linear.
- **CI-repair commits must be fixups.** If a commit on this branch broke
  CI and the follow-up exists only to repair it, commit with
  `git commit --fixup=<broken-sha>` instead of a standalone `fix:`.
  Before pushing, run `scripts/git_squash.sh` (a thin wrapper over
  `GIT_SEQUENCE_EDITOR=: git rebase -i --autosquash origin/main`) so the
  fixups collapse into their targets. This keeps main's linear history
  free of commits that temporarily broke the build. Review-round commits
  (addressing reviewer feedback from an earlier push) remain standalone
  so the audit trail survives.
- **No unsafe code**: every crate root must declare `#![forbid(unsafe_code)]`.
- **Inter-module imports respect a partial order.** Plan
  2026-04-29-01 T7 introduced the original pure-crate layering
  rule; Plan 2026-05-03-06 extended it workspace-wide after the
  crate boundary was redrawn:

  `agogo-chan` (`crates/chan/src`) is the pure music/channel/control
  library:

      control  → sink, channel, time, conn
      sink     → channel, time, conn
      channel  → time, conn
      time     → conn
      conn     → (leaf)
      test     → (leaf)

  `agogo-core` (`crates/core/src`) is the runtime orchestration
  crate:

      driver    → bridge, snapshot, transport, event
      runtime   → bridge, driver, snapshot, transport, event
      bridge    → transport, event, snapshot
      transport → event
      snapshot  → (leaf)
      event     → (leaf)

  Host adapter crates declare only the modules they actually have.
  `host-cpal` currently has `cpal` as a leaf layer; `host-midi`
  has `midir` as a leaf layer; `host-link` has:

      session   → link, quantum, transport
      source    → session
      link      → quantum
      transport → (leaf)
      quantum   → (leaf)

  `agogo-cli` (`crates/cli/src`) is the binary shell:

      command → parse
      parse   → (leaf)

  Optional future CLI roots such as `log` or `test` should be
  added to the gate only when the module exists. `src/test.rs`, if
  ever added, must stay limited like `chan::test`; command-surface
  tests belong under `crates/cli/test/*.rs` with explicit
  `[[test]]` entries.

  Each top-level module-root file declares its allowed deps in a
  sentinel header comment:

      //! layer: time
      //! depends-on: conn

  `scripts/check_layers.sh` parses these headers and fails on any
  `use crate::<top>`, `use agogo_chan::<top>`, or
  `use agogo_core::<top>` in production code (column-0 imports —
  including `pub use` re-exports and `use crate::{a, b}` grouped
  forms) that names a module in that crate's layer set which the
  current layer's `depends-on:` list does not authorise. The gate
  also checks that each `//! layer:` sentinel matches its filename
  so a stale rename can't go unnoticed. Test-block imports
  (indented inside `#[cfg(test)] mod tests { … }`) are allowed to
  cross layers — integration tests legitimately need to wire
  pieces together. Adding a new edge requires updating both the
  sentinel comment AND this rule's prose so the gate and the
  convention stay in sync.

- **No stored `f32`/`f64` outside the five documented exceptions.**

  **Glossary.**
  > **PI controller** — the proportional-integral control loop in
  > `crates/chan/src/control/pll.rs`. It reads the phase error (observed
  > vs. expected pulse spacing), scales it by a proportional gain
  > `kp` and an accumulated integrator term `ki × ∑error`, and steers
  > the NCO's frequency toward the true tempo. "PI-exempt" means a
  > value *participates in this specific feedback loop*, whose math
  > is genuinely continuous-valued analog DSP — not just any float
  > in the crate.
  >
  > **ABI-local** — a float that exists inside a function body to
  > interoperate with an external binary interface (PCM audio via
  > cpal; rational coefficients in a parabolic curve fit) and dies
  > inside the function scope. Nothing stored, nothing returned
  > outside the comment-marked locals.
  >
  > **argv boundary** — a float that reaches us from the terminal
  > via bpaf because the user typed a decimal at the command line.
  > It dies on the first line of the handler via `f64_bpm_to_tempo`,
  > `F64F06.ceil(...)`, `F64F12.ceil(...)`, or one of the other
  > named `agogo_core::conn` / `agogo_chan::conn` Conns.
  >
  > **Link FFI** — a float that flows through rusty_link / AblLink's
  > C++ ABI. Contained to `crates/host-link`; every site converts
  > to/from `Tempo` / `Phase` within one or two lines of the FFI
  > call, with a `// Link FFI` comment on the conversion line.
  >
  > **PCM ABI** — a PCM audio sample slice `&[f32]` at the cpal
  > boundary. Comment: `// PCM ABI`.

  The five allowed uses:

  1. PI controller state and gains in `control::pll`
     (`PllSettings`, `PllState`, and the control-law body). Mark
     intermediate locals `// PI-exempt`.
  2. PCM audio sample slices (`&[f32]`) at the cpal ABI boundary.
     Mark `// PCM ABI`.
  3. Parabolic-fit f64 locals inside `control::detect`
     (contained to a handful of lines, converted to Q48.16 before
     escape). Mark `// ABI-local`.
  4. CLI argv parsers — `f64` accepts a human-typed decimal, then
     dies at the handler's first line via `f64_bpm_to_tempo` /
     `F64F06` / `F64F12`. Mark `// argv boundary`.
  5. Link FFI inside `crates/host-link` — AblLink's C++ ABI hands
     us `f64` tempo and phase; we convert to `Tempo` / `Phase`
     within 1–2 lines. Mark `// Link FFI`.

  `scripts/check_floats.sh` (CI job) fails if a naked `f32` / `f64`
  lives outside the file-level allowlist. Plan 2026-04-28-03 T5
  reshuffled the entries when the old pure crate's `fxp.rs` was
  deleted: its argv + PI-exempt content moved to
  `crates/chan/src/conn/float_boundary.rs`
  (replaces the `fxp.rs` entry); `time/decimal.rs` came off the list
  because the `float_conn!` macro split into `time/float.rs` (which
  is now allowlisted in its place — vendored-from-connections, same
  justification); and Plan 2026-04-28-03 T4 moved `Quantum` +
  `f64_beats_to_quantum` to `crates/host-link/src/quantum.rs` (added
  to the allowlist as a Link-FFI parity helper). Plan 2026-04-28-05
  T1/T3/T5 added `cli/src/trace/sync.rs`, `cli/src/time/schedule.rs`,
  and `cli/src/link/probe.rs` when those inline modules were
  extracted from `cli/src/main.rs`'s 1722-line kitchen sink; each
  inherits its predecessor's allowlist eligibility (same exception
  classes, same boundaries, just split into sibling modules). Plan
  2026-04-29-01 T2 collapsed the conn-shaped value types into one
  parent: `boundary.rs` → `conn/boundary.rs`,
  `time/float.rs` → `conn/float.rs`, `time/sample.rs` →
  `conn/sample.rs` — same allowlist eligibility, new path. Plan
  2026-05-04-03 re-exported the boundary helpers through
  `conn::float` via private `conn/float_boundary.rs`, and renamed
  `conn/sample.rs` to `conn/rate.rs` with Sxxx sample-rate markers
  renamed to Rxxx. The
  Plan 2026-04-30-03 moved the CLI argv parsers from
  `cli/src/main.rs` to `cli/src/parsers.rs` and grouped trace/time/link
  handlers under subdirectories. Plan 2026-05-04-01 reshaped the CLI
  modules again: `parsers.rs` became `parse.rs`, and the trace/time/link
  handlers now live under `cli/src/command/`. Plan 2026-05-02-05 moved the
  `control.rs` PCM ABI test locals into `control/transport.rs`
  alongside `Playhead`. Plan 2026-05-03-06 moved that runtime
  transport module to `crates/core/src/transport.rs` and renamed the
  pure crate to `crates/chan`. Plan 2026-05-02-06 kept
  `crates/chan/src/sink/audio.rs` on the allowlist for `AudioIo`
  PCM slices and the generated audio-click renderer's output-boundary
  PCM writes. The current allowlist is the 21 entries in
  `scripts/check_floats.sh::ALLOWED` (Plan 2026-04-28-06 T3 swapped
  `machine/spec.rs` for `machine/spec/parser.rs` when the kitchen
  sink split — same `delay=ms` argv boundary, just lives in the
  parser submodule now); see that script's header for a one-line
  justification per file.
  The annotation comments above (`// PI-exempt`, `// PCM ABI`,
  `// argv boundary`, `// ABI-local`, `// Link FFI`) are
  reviewer-oriented markers inside allowlisted files — they're not
  enforced by the grep gate itself, which would need a full Rust
  parser to classify each use. Pattern 9 in
  `doc/reviews/review-calibration.md` is the complementary review
  check that catches stored-state violations the gate misses.

- **Every numerical conversion comes from a named `Conn` (or a
  Conn-lookalike with proptested adjoint laws).** Bespoke `fn
  f64_some_thing_to_other(x: f64) -> Other` helpers (now living in
  `crate::conn::float` after Plan 2026-04-28-03 T5 deleted `fxp.rs`
  and Plan 2026-04-29-01 T2 moved boundary under `conn/`) are only
  allowed for types that can't be expressed as a lawful `Conn` (e.g.
  `Phase` is a wrapping quotient onto a torus, not a monotone map —
  the bespoke `f64_phase_to_phase` is the one legitimate exception). Naming follows the conventions in the
  upstream library:
  - The total identifier is **exactly 8 ASCII chars**. Names shorter
    than 8 chars (e.g. the legacy `S88S44`) are not permitted.
  - Each side is **exactly 4 chars**, picking one of `{A123, AB12,
    ABC1, ABCD}` independently. Sides shorter or longer than 4 chars
    are not permitted.
  - Digits are zero-padded to fill the digit count for the side's
    shape (e.g. `R048`, not `S48`).
  - Letters and digits only — no underscores, hyphens, or other
    separators inside the name.
  - The `AGENTS.md` in the upstream repository spells this out in detail.

- **Connection construction must use the upstream macros and must be
  total over the declared types.** Production code in agogo must not
  call `Conn::new_l`, `Conn::new_r`, `RuntimeConn::new`, or a local
  wrapper macro such as `def_conn_marker!` to publish a connection.
  Declare connections with the upstream `connections::triple!`,
  `connections::iso!`, `connections::compose!`,
  `connections::conn_l!`, `connections::conn_r!`,
  `connections::compose_l!`, or `connections::compose_r!` macros.

  A connection adjoint (`ceil`, `inner`, `floor`) must not contain
  `expect`, `unwrap`, `panic`, `unreachable`, arithmetic overflow, or
  a prose-only precondition to fake totality. Do not bound a proptest
  generator to keep the bad part of the input type away from the
  adjoint; that cooks the test instead of testing the connection. If
  the lawful shape is not known, stop and ask for the missing math or
  API design. It is categorically better to say "I do not know how to
  implement this connection lawfully" than to fake a connection and
  hide the failure in the generator.

  `scripts/check_connections.sh` enforces the constructor side of this
  rule. Temporary migration allowlists are allowed only with a plan
  Review entry that names the remaining design problem.

- **User-reachable invalid input fails at the boundary, not in bridge
  or scheduler panics.** CLI parsers, host-command parsers, config
  loaders, and FFI adapters return `Result` / `Option` with precise
  errors for invalid user input. Once a value enters core scheduling,
  rendering, or bridge code, it should either be type-validated or
  already boundary-checked. Do not use `assert!`, `expect`, `unwrap`,
  `panic!`, or `unreachable!` in bridge/scheduler modules to reject
  values that can originate from CLI args, host commands, config
  files, devices, or network peers.

  `scripts/check_boundary_panics.sh` enforces the narrow production
  scope where this mistake is most damaging. A true internal invariant
  may be annotated with `// boundary-panic-ok: <reason>`, but the
  reason must state why the value is not user input and why returning
  an error would be misleading.

- **Cross-conversions compose existing `Conn`s — they are not
  hardcoded.** If `A → C` is needed and `Conn<A, B>` + `Conn<B, C>`
  already exist, compose the two at the call site. A small helper
  that wraps a *visible* composition (name reflects the operation,
  body shows the chain) is fine if it's used at ≥2 call sites
  that would otherwise drift — e.g. `transform::micro_to_samples`
  wraps `F12F06.inner` + `PicoSampleConn::ceil` +
  `Q48.16.round().to_num::<i64>()` and is called from both
  `transform` and `scheduler` to guarantee rounding agreement. A
  helper is **not** fine if it hides what would otherwise be a
  single direct `Conn` call (that's just renaming) or if it
  open-codes the arithmetic.

  ```rust
  // Good: compose Micro → Pico → Sample at the call site.
  let samples = psc.ceil(F12F06.inner(micro_value));

  // Good: helper that wraps the composition for ≥2 call sites.
  // The body shows the chain; `.round().to_num::<i64>()` is the
  // final non-Conn extraction that motivates the helper.
  fn micro_to_samples(m: Micro, sr: u32) -> i64 {
      let pico = F12F06.inner(m);
      PicoSampleConn::new(sr).ceil(pico).round().to_num::<i64>()
  }

  // Bad: open-code the arithmetic.
  let samples = (micro_value.0 * sr as i128 * 10 / ...);   // nope

  // Bad: a helper that hides a single Conn call behind a new name.
  fn micro_to_pico(m: Micro) -> Pico { F12F06.inner(m) }   // nope

  // Bad: a bespoke `f64_*_to_*` function where a Conn constant
  // would do.
  pub fn f64_ms_to_micro(ms: f64) -> Micro { ... }   // nope — use F64F06.
  ```

  If repeated composition becomes ergonomic debt beyond the narrow
  helper case above, the fix is upstream (e.g. a `Conn::then`
  composition primitive in the `connections` crate) — not a
  local hardcoded helper.

- **Test fixtures are gitignored**, and a fresh checkout must pass
  `cargo test --workspace` with zero setup. Tests that depend on a
  fixture file must use the `fixture_or_skip!` macro from the core
  crate and `return` cleanly when the fixture is absent — **do not**
  `#[ignore]` them and do not panic.
- **Property-based testing is mandatory** for any module that parses,
  encodes, or transforms data. Use `proptest` (workspace dev-dep).
  - Define strategies as functions returning `impl Strategy`, not
    `Arbitrary` derive. Use `prop_oneof!` with frequency weights to
    bias toward boundary values and edge cases.
  - **Generator domain = the input type's full domain.** Default to
    `any::<i64>()`, `any::<u32>()`, `prop::num::f64::NORMAL`, etc.
    Named boundaries (`i64::MAX`, `i64::MIN`, `0`, NaN, ±∞) go in
    explicit `Just(_)` arms with elevated frequency. **Bounding the
    generator to keep intermediate arithmetic "safe" (i.e. under a
    wrap or overflow threshold) is an anti-pattern — it fakes
    coverage by hiding the exact region where wrap / saturation
    bugs live.** If you genuinely must bound the domain, document
    *why* immediately above the strategy and add a separate
    `#[test]` spot-check at the un-sampled boundary.
  - **Pair adjoint-law and round-trip properties with orthogonal
    sanity checks.** A Galois-law / round-trip proptest can pass
    trivially through a silent wrap (the law compares two sides of
    the same broken function). Monotonicity across the full input
    range, an "adjacent inputs differ by ≤ bounded step" property,
    or a saturation spot-check at the type boundary expose wraps
    the adjoint law is blind to.
  - **Test the test before pushing.** For any new proptest intended
    to catch a specific class of bug, revert the relevant fix in a
    dirty worktree and re-run — if the proptest still passes, the
    generator isn't reaching the failure region and the test is
    decorative. Restore the fix from backup once verified.
  - **One arb file per top-level module.** Each top-level module
    that exposes proptest strategies owns a single
    `#[cfg(any(test, feature = "testkit"))] pub mod arb;`
    declaration on its module-root file, with all strategies for
    types under that layer collected in `<top>/arb.rs` (e.g.
    `time/arb.rs` holds `arb_grid`, `arb_tbase`, `arb_tick`,
    `arb_swing`; `conn/arb.rs` holds `arb_bpm`, `arb_jitter_sigma`,
    `arb_sample_rate`, the `fixed_*` / `extended_fd*` / `rate_*` /
    `pico_*` battery vendored from `connections @ d1ac1ead`'s
    `property::arb`). Same shape as the Haskell connections test
    layout (`Test/Data/Connection/{Float,Int,…}.hs`) — strategies
    grouped per top-level module rather than per type. Plan
    2026-04-29-01 T4 collapsed per-type `arb.rs` files into the
    top-level files; the prior convention (per-type colocation) was
    relaxed because it forced sibling-import paths
    (`crate::time::tempo::arb::arb_bpm`) that bloated importer
    `use` lines without adding clarity. Strategies private to a
    single test module stay inline in that module's `#[cfg(test)]`
    block.
  - Properties that must hold for a sprint to ship are defined **in
    the plan's Verification table** before any code is written.
  - If a property test blocks progress during implementation, you may
    `#[ignore]` it temporarily but **you must document it** in the
    plan's Review section with the reason and a plan to re-enable.
- **Use Rust's modern module layout.** If you have a specific reason why
  you cannot then again **you must document it**. The modern layout does
  not have a `mod.rs` file. The equivalent module sits one level up and
  is named after the module directory:

  ```
  src/
  ├── main.rs
  ├── network.rs      <-- Defines 'network' module
  └── network/
      └── server.rs   <-- Submodule of 'network'
  ```

### Session notes

`doc/notes/` is gitignored and holds the user's personal notes for the
project. Agents may read from it for context but must not write to it
unless explicitly asked.

### Commit style

Conventional commits, present-tense imperative subject. Accepted prefixes:
`plan`, `feat`, `fix`, `fmt`, `doc`, `test`, `task`, `debt`. Scopes are
allowed (e.g. `doc(skills):`, `fix(scripts):`).

- `plan:` lands a new plan doc in `doc/plans/` — always the first
  commit on a `plan/YYYY-MM-DD-NN` branch.
- `feat:` and the rest cover the implementation that follows.

```
plan: Widget-format parser, sprint goals and verification table
feat: Add parser for widget format
fix(codec): Handle timeout on reconnect
test: Add round-trip property tests for codec
doc: Append Sprint 2 completion report
task: Add serde to core dependencies
debt: Remove dead handshake branch
```

Keep subjects under 72 characters. Use the body for non-obvious decisions.

## Two-tier review workflow

`doc/workflow.md` has mermaid state diagrams for the review-round
lifecycle and the `/pr-watch` loop — useful when debugging an
unexpected situation (stuck fix commit, loop that won't quit). The
prose below is authoritative; the diagrams are derived views.

### Tier 1 — Local review (pre-push)

The coding agent makes atomic commits as it works. Each commit must pass
`cargo test` and `cargo clippy` (enforced by the pre-commit hook in
`.claude/settings.json`). Commits can be as small as desired.

Step 7 of the TDD workflow creates the PR's review file with the
sprint's PR description under a `## Summary` heading. The path comes
from `scripts/pr_report.py path` — no argument, it predicts the next PR
number (via `scripts/pr_request.sh`) and emits the zero-padded
filename, e.g. `doc/reviews/review-00017.md`. The `## Summary`
section is the single source of truth for the PR body: open the PR
with `gh pr create --body-file <(scripts/pr_report.py body N)` so
the GitHub body is a direct copy of the file. Because the
description is committed *before* push, a PR that gets no review
comments merges without any extra round-trip — the body is already
in history. `review-00000.md` is a protected sentinel; real reviews
start at `00001`.

Before pushing, run the local review transition. Claude Code uses
`/pr-review`; Codex and shell users use `scripts/pr_review.sh`.
This spawns an independent reviewer that examines
`git diff origin/main...HEAD` and the commit log. The reviewer flags
must-fix issues and follow-ups, which the transition appends as a
`## Local review (YYYY-MM-DD)` section below the summary. The local
review transition aborts if the review file or its `## Summary`
section is missing — step 7 is a prerequisite.

If another issue or PR is opened between running step 7 and opening
this branch's PR, the predicted number can drift — re-run
`scripts/pr_report.py path` before pushing and `mv` the old file to the
new path if needed.

If must-fix items exist, resolve them before pushing. If the review
is clean, push and open the PR with `--body-file` as above.

### Tier 2 — GitHub review (post-push)

Once pushed, CI runs `cargo test --workspace` and
`cargo clippy --all-targets -- -D warnings` (see
`.github/workflows/ci.yml`). Automated code review agents and/or GitHub Copilot
perform a second-round review on the PR automatically.

After GitHub review activity, run `/pull-reviews <N>` to fetch the PR's
review bodies and inline comments and **append them chronologically to the
same `doc/reviews/review-NNNNN.md`** used by Tier 1. The command is
idempotent — it records `<!-- gh-id: NNNNN -->` markers for each appended
item and skips any id already present, so running it repeatedly only
appends new comments. The result is one file per PR containing the full
local + GitHub review history in order.

Once the findings are addressed as **uncommitted edits in the working
tree**, run `/pr-reply <N>`. The command does the whole round
in order: posts replies to each unresolved thread, runs
`scripts/pr_report.py reviews` to mirror the replies into `review-NNNNN.md`,
then makes ONE atomic commit containing both the code edits and the
mirrored doc. You then `git push` once — code + replies + review doc
land in a single round trip.

**Do not commit the fix yourself before running `/pr-reply`.**
The command runs on the `gh_review → items_pulled → round_unpushed`
arrow per `doc/workflow.md` — it expects to start from `gh_review`
(local at-or-behind origin) and produce the round commit itself.
Pre-committing a fix would put the branch at an unpushed-state that
breaks the precondition; if you have a stranded pre-existing fix
commit, push it first, then re-run. `/pr-reply` refuses to run
if the branch already has unpushed commits.

**Do not merge before pushing the round commit.** Per
`doc/workflow.md`'s state machine, the merge transition is
`gh_review → merged` — there is no edge from `round_unpushed → merged`.
Merging from `round_unpushed` (the state after `/pr-reply`
makes its commit but before push) silently drops the local commit
because `gh pr merge` is GitHub-side and doesn't see local state.
Use `scripts/git_merge.sh <pr-args>` instead of `gh pr merge` —
the wrapper refuses to invoke the merge while the local branch
is ahead of origin. Recovery (if a merge already dropped a round
commit): cherry-pick the stranded SHA into the next plan branch's
first commit per the bundle-into-next-plan convention; don't open
a tiny standalone PR.

`/pull-reviews <N>` remains available as a lower-level primitive for
fetching comments without posting. Use it standalone only to refresh
the doc right before the final pre-merge push, to capture any trailing
reviewer comments; its output rides with the next fix commit, never as
a standalone `doc:` commit.

The local review catches design issues and convention violations early.
The GitHub review catches anything that slipped through and validates in
the CI environment. Claude Code's built-in `/review [PR]` can be used as
post-push review help, but it is not the canonical pre-push transition.
Joining local and GitHub review into a single file per PR preserves the
conversational flow and keeps the review record in one place.

### Automated poll loop (optional)

For PRs where you don't want to manually ping "check the replies", pair
`/pr-watch <N>` with `/loop`:

```
/loop 10m /pr-watch 17
```

Each tick does one of: (a) heartbeat if no new activity, (b) one
finish-the-round cycle — auto-fix the trivially-clear items, push back
or defer the rest, run the `/pr-reply` flow, **push the round
commit**, or (c) `paused at round_unpushed: push failed` if the push
itself errored (network, non-fast-forward).

Auto-fix is scoped tightly: only items where the reviewer's intent is
unambiguous and the change is local (one file, under ~20 lines, no
API removal, no cross-module reasoning). Anything involving judgment
is classified as **needs you** and surfaced in the round report with
`path:line` — those threads stay open on GitHub for you to resolve.

The command never **merges**. The merge is the user's safety gate:
each PR is reviewed manually before `gh pr merge` /
`scripts/git_merge.sh`. Pushing the round commit advances the
branch to `gh_review` so CI re-runs and the reviewer sees replies
attached to the right tip — that's normal mid-PR motion, not a risk
worth gating on.

## TDD workflow

Every sprint follows this order. Naming is keyed to the plan filename:
a plan at `doc/plans/plan-YYYY-MM-DD-NN.md` maps to branch
`plan/YYYY-MM-DD-NN` and (if used) worktree `../<repo>.plan-YYYY-MM-DD-NN`.
One slug, three places.

1. **Pick the plan filename.** `ls doc/plans/plan-YYYY-MM-DD-*.md` to
   find the next unused `NN` for today's date (zero-padded, starts at
   `01`). No writes yet — main stays clean.
2. **Ask the user: worktree or branch?** Worktree is mandatory if
   another agent instance is active in this repo; otherwise it's the
   user's call. Then:
   - worktree: `git worktree add ../<repo>.plan-YYYY-MM-DD-NN -b plan/YYYY-MM-DD-NN`, `cd` into it.
   - branch: `git switch -c plan/YYYY-MM-DD-NN`.
   If `git` reports `cannot lock ref` or `unable to create directory`
   for the slash branch, first diagnose the ref shape with
   `git show-ref --heads | grep 'refs/heads/plan'`. If a real
   `refs/heads/plan` ref exists, or slash branches remain blocked,
   fall back to the flat branch name `plan-YYYY-MM-DD-NN`. If no
   conflicting ref exists, the failure may be a sandbox or Git-ref
   write-permission issue; retry with the appropriate approval before
   falling back. Keep the plan filename canonical either way, and note
   any flat-branch fallback in the plan's **Review** section.
3. **Write the plan** to `doc/plans/plan-YYYY-MM-DD-NN.md` on that branch.
   The plan's **Verification** table must list the property tests that
   must pass for the sprint to ship (e.g., "message round-trips through
   encode/decode", "parser never panics on arbitrary input"). Commit
   as `plan: <one-line goal>` — this is the sprint-opener, always the
   first commit on the branch.
4. Write proptest properties and test skeletons that compile but
   trivially fail. Properties come first — they define the contract.
5. Implement the module until all tests are green.
6. Commit on the branch, when green.
7. **Finalize the sprint docs.** In one commit:
   - Append Deferred and Review sections to the plan document. If any
     property tests were `#[ignore]`d during implementation, document
     the reason and the re-enablement plan here.
   - Create the review file at `$(scripts/pr_report.py path)` (no
     argument predicts the next PR number and zero-pads the
     filename). Header is `# PR #<N> — <title>` followed by a
     `## Summary` section containing the PR body. This section is
     consumed verbatim by
     `gh pr create --body-file <(scripts/pr_report.py body N)`, so
     write it as the PR description (what & why for a human
     reviewer) — not a ship-report.

   This must happen *before* the local review — the reviewer agent
   reads the plan as context and should see the final version, and
   local review transition aborts if the review file is missing its
   `## Summary`. Commit as `doc: Finalize plan NN and PR description`.
8. Run the local review transition against the branch before merging:
   Claude Code uses `/pr-review`; Codex and shell users use
   `scripts/pr_review.sh`.
9. Rebase and land on main. First, on the feature branch:
   `git fetch origin && git rebase origin/main`. Then fast-forward main:
   - **Branch case**: `git checkout main && git merge --ff-only plan/YYYY-MM-DD-NN`.
   - **Worktree case**: main is already checked out in the *primary*
     worktree, so you can't `checkout main` here. `cd` back to the
     primary and run `git merge --ff-only plan/YYYY-MM-DD-NN` there.
     Step 10's `git worktree remove` then runs from the primary too.
10. Clean up: `git worktree remove ../<repo>.plan-YYYY-MM-DD-NN`
    (worktree case only), then `git branch -d plan/YYYY-MM-DD-NN`.

### Pre-commit and pre-push hooks

The git-side hook chain is split by cost across two events. Cheap
deterministic checks (sub-second combined) fire on every commit;
the expensive `cargo test --workspace` + `cargo clippy
--all-targets` (~50s combined) fire once per push. Both are
activated by the same line:

```
git config core.hooksPath .githooks
```

**Layer 1 — Agent `PreToolUse`** (`.claude/settings.json`):
fires on agent-invoked shell calls matching `git commit*`. Catches
PII / float-discipline issues during agent iteration without
invoking git for real. Limitation: `PreToolUse` runs *before* the
matched Bash call's body executes, so a chained command like `git
add file && git commit -m "..."` sees an empty pre-add staged diff
at hook time and slips through `check_pii.sh` / `check_floats.sh`.
Use separate `git add` and `git commit` calls to keep this layer
effective.

**Layer 2 — Git `pre-commit`** (`.githooks/pre-commit`): fires at
git's standard hook point (after staging, before commit object
creation). Sees the actual staged content regardless of how the
commit was invoked — chained Bash, terminal, IDE, anything. This
is the unbypassable safety net at commit time. Runs the cheap
chain:

1. `cargo fmt -p agogo-chan -p agogo-core -p agogo-cli -- --check` — fmt drift
   aborts the commit. Run `cargo fmt -p agogo-chan -p agogo-core -p agogo-cli`
   to fix. Scope is explicit (not `--all`) because the sibling
   `connections` path-dep is reachable from this workspace and
   we don't want to fail on its formatting state. (The fmt step
   was warn-only previously; flipped to blocking after a real CI
   fmt failure in a sibling project that local tooling let
   through. Keeping the fmt step blocking forces drift to be
   fixed at commit time when the cost is one `cargo fmt`
   invocation.)
2. `scripts/check_pii.sh` — grep the staged diff for absolute
   user-home paths (`/Users/...` on macOS, `/home/...` on Linux),
   private-key headers, and common API-token shapes. Fail fast on
   any match. Allow-list exceptions go in `.pii-allow`.
3. `scripts/check_floats.sh` — fail if naked `f32`/`f64` appears
   in a non-allowlisted file. See "no stored f32/f64" rule above.
4. `scripts/check_layers.sh` — fail if any `use crate::<top>` /
   `use agogo_chan::<top>` / `use agogo_core::<top>` (column-0
   imports) violates the partial order in each module-root's
   `//! depends-on:` sentinel. See the layering rule above.
5. `scripts/check_connections.sh` — fail if production code constructs
   `Conn` values directly with `Conn::new_l`, `Conn::new_r`,
   `RuntimeConn::new`, or local marker wrappers instead of the upstream
   declaration/composition macros. Temporary allowlists must be named
   in the active plan's Review section.

**Layer 3 — Git `pre-push`** (`.githooks/pre-push`): fires once
per `git push`, regardless of intra-branch commit count. Runs the
expensive chain:

1. `cargo test --workspace` — all tests must pass.
2. `cargo clippy --all-targets -- -D warnings` — matches CI.

This is what enforces the "every pushed commit is green"
invariant from the conventions list above. Intra-branch commits
can be transiently red between `git commit` and `git push` — the
autosquash workflow expects this for `fixup!` commits. The
pre-push hook short-circuits with `exit 0` if no refs are being
pushed (delete-only / no-op pushes don't pay the cost).

This is the automated quality gate; `/pr-review` and
`scripts/pr_review.sh` are the manual local-review gates. Bypass
with `--no-verify` (or `git push --no-verify`) only when explicitly
authorized.

CI adds a `gitleaks` job (`.github/workflows/ci.yml`) that scans the
full history on every PR as defense-in-depth against anything that
bypasses both local hooks (e.g. `git commit --no-verify`, or a
clone where `core.hooksPath` was never set).

## Sprint plan format

```markdown
# Plan NN — Title

## Goal
One sentence.

## Dependency Graph
ASCII art showing task dependencies (T1 → T2, T3 → T4, etc.)

## Tasks
Each task is T1, T2, etc. Each task section includes:
- Problem or motivation
- Solution / implementation approach
- Types or API surface

## Verification

### Properties (must pass)
Table of proptest property names, the module they live in, and the
invariant they assert. These are the contract — if a property can't
be satisfied, the sprint isn't done.

| Property | Module | Invariant |
|----------|--------|-----------|
| `msg_round_trips` | `crate_foo::codec` | encode then decode recovers original |

### Spot checks
Table of unit test names + specific assertions.

### Build gates
- cargo build — no errors
- cargo test — all pass (no `#[ignore]` without Review documentation)
- cargo clippy --all-targets — no errors
- End-to-end scenario description

## Deferred
What was intentionally left out and why.

## Review
- Any `#[ignore]`d properties: which ones, why, re-enablement plan
- Design deviations from the plan
- **Drift caught** (file:line — rule — fixed-here / deferred). See
  "The gardener rule" above. Even a clean sprint usually finds a few
  stale comments or test names; an empty list usually means the
  agent didn't look.
- Recommendations
```
