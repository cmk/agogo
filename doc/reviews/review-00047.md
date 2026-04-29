# PR #47 — Decouple agogo-cli from agogo-host-link (Plan 09)

## Summary

Stops the trend of pulling `agogo_host_link::*` types into
`crates/cli/`. Closes the long-deferred "wire snap_intent into the
orchestrator" item from `doc/todo.md` *without* widening the cli/Link
coupling — the snap-walk now lives in `host-link` itself.

### What changed

- **`agogo_host_link::apply_snap_offsets(specs, session, channels)` helper.**
  Walks the parallel spec/channel vectors once; for each pair, calls
  `session.snap_offset_for(spec.snap_intent())` and folds the `Micro`
  delta into the channel's `offset`. The orchestrator (`cli/src/run.rs`)
  calls this once after constructing the `LinkSession` and before
  `LinkPhaseSource::new` consumes it.
  - Three tests (`apply_snap_offsets_no_panic_on_empty`,
    `apply_snap_offsets_matches_per_channel`,
    `apply_snap_offsets_idempotent_per_channel`) live in
    `crates/host-link/src/session.rs::tests`. They run with
    `cargo test -p agogo-host-link --features rusty-link` (cmake
    required — local-only, CI doesn't yet have a rusty-link job).
  - Production `cli/src/run.rs` previously skipped the snap-arming
    step entirely; only the integration test
    (`host-link/tests/bidirectional.rs`) exercised it.

- **`cli-no-link` CI gate.** Adds a job that runs
  `cargo build` + `cargo clippy` for `agogo-cli --no-default-features
  --features core,cpal,midi`. The link-feature gating in the cli is
  **already in place** today — investigation confirmed that the
  `--quantum` field at `main.rs:215-216`, the
  `parse_quantum_from_beats` re-export at `main.rs:398`, and
  `RunArgs.link_quantum` at `run.rs:80` all sit inside
  `#[cfg(feature = "link")]`-gated scopes (or transitively-gated
  `cfg(feature = "run")`). The job pins this so a future PR can't
  silently drift a Link import into module scope.

- **`cli/src/run.rs` scoping comment.** Top-of-file documentation
  paragraph codifies the rule for future contributors: module-scope
  `agogo_host_link::*` imports in `run.rs` are OK because the module
  is `cfg(feature = "run")`-gated, but in-function Link constructions
  belong exclusively in the `Source::Link` match arm.

### Ride-along polish

- **`agogo-cli` binary alias removed** from `crates/cli/Cargo.toml`.
  External scripts have migrated since v0.1; the duplicate `[[bin]]`
  entry was generating a "found in multiple build targets" cargo
  warning.
- **`#[bpaf(version)]`** added to the top-level `Cli` parser struct
  so `agogo --version` prints a useful string. Pre-existing follow-up
  from the bpaf swap.
- **`CvRole` / `DinRole` re-exports marked `#[doc(hidden)]`** —
  forward-compat scaffolding for v0.4 (CV pulse / LFO) and v0.2 (DIN
  sync24) backends with no renderer in v0.1. The attribute comes off
  in the plan that ships each renderer.

### Companion plan docs (deferred)

This branch also lands `doc/plans/plan-2026-04-28-{10,11}.md` —
companion proposals from the same planning round but **not in scope
for this sprint**:

- **Plan 10 — Audio-click channel** (`Channel::Audio` variant + cpal
  output stream + ADSR-noise click renderer). Closes two top-level
  v0.4 deferred items at once.
- **Plan 11 — `LpfPid` PID-controller wrapper** around the existing
  `Pll`. Prerequisite for v0.5 Sprint 02's PID-smoothed Link follower.

Both are landed as `doc:` commits so they're discoverable in
`doc/plans/` for the next sprint cycle. No code changes for either.

### What's *not* fixed

The `doc/todo.md` deferred item titled "`cargo build -p agogo-cli
--no-default-features` build break" is **not** closed by this PR.
T2 investigation found it's a separate problem about the `core`
feature gate, not `link`: `time_sched.rs` is pulled in at
`Cli`-enum scope and uses `agogo_core::time::*` unconditionally, so
without `core` the parser doesn't compile. Adding `core` to the
feature list (`--features core,cpal,midi`) succeeds today and is
what T2's CI gate pins. Fixing the no-core build is its own plan —
the simplest path is dropping the `core` feature entirely (making
`agogo-core` a non-optional dep) and removing the
`#[cfg(not(feature = "core"))]` stub blocks in `cli/src/main.rs`.

## Local review (2026-04-28)

**Branch:** `plan/2026-04-28-09`
**Commits:** 8 (origin/main..plan/2026-04-28-09)
**Reviewer:** Claude (sonnet, independent)

---

### Commit Hygiene

Eight commits on the branch. Two original prefixes (`chore:` on
the polish commit, `ci:` on the no-link gate commit) violated
CLAUDE.md's accepted list (`plan|feat|fix|fmt|doc|test|task|debt`
only). Both rewritten to `task:` via `git filter-branch --msg-filter`
during this review round (auto-applied). All commits now conform.

Companion plan docs for plans 10 + 11 land as a single `doc:`
commit (`07b8092`), explicitly called out in the PR summary as
deferred future-work. Acceptable bundling, not scope creep.

Each commit leaves the repo in a buildable, testable state — the
pre-commit hook chain (fmt / pii / floats / cargo test / clippy)
ran on every commit and was green.

### Code Quality

- `#![forbid(unsafe_code)]` declared in all modified crate roots.
- No stored `f32`/`f64` introduced. `check-floats.sh` clean.
- The `apply_snap_offsets` helper composes existing typed accessors
  (`snap_intent` → `Quantum` wrap → `snap_offset_for` → `Micro`
  fold). No bespoke arithmetic, no naked unit shifts.
- Modern module layout maintained — no `mod.rs` files added.
- The plan's central goal (don't widen cli/Link coupling) is met:
  the only new use of `agogo_host_link::*` in `cli/` is
  `apply_snap_offsets(&specs, &mut session, &mut channels)` at
  `run.rs:278`, inside the existing `Source::Link` match arm.
  Module-scope `agogo_host_link::*` imports in `run.rs` are OK
  because the entire module is `cfg(feature = "run")`-gated and
  `run` requires `link`.

### Test Coverage

The three plan-mandated tests are present and not `#[ignore]`'d.
Initial review round flagged a stub-passing risk in
`apply_snap_offsets_matches_per_channel` and
`apply_snap_offsets_idempotent_per_channel`: both tests compared
two paths' offsets against each other (`drift < 4_000`), satisfied
trivially by a no-op `apply_snap_offsets` returning zero on both
sides. Fix applied this round (commit `994129f`): added liveness
guards asserting at least one snap-armed channel has
`offset != Micro::ZERO` after the bulk call. Now the tests fail
loudly against a stub `fn apply_snap_offsets(_,_,_) {}` and would
catch a regression where the helper silently stops mutating
channels.

The tests are deterministic (hard-coded spec strings), not
`proptest!` properties. The plan's Verification table heading reads
"Properties (must pass)" but these are unit tests with seeded
inputs. Acceptable for an internal helper that just composes
existing well-tested primitives, but flagged as a follow-up: a
proper proptest with arbitrary-permutation spec lists would catch
order-dependent regressions the seeded inputs don't.

The tests live behind `--features rusty-link` (cmake required for
the C++ build). CI doesn't run them today — explicitly tracked as
a follow-up in the plan's Review section.

### Plan Conformance

- T1 (apply_snap_offsets helper + cli wiring) — `host-link/src/session.rs:230-250`,
  re-exported in `lib.rs:206`, called from `run.rs:278`. ✓
- T2 (CI no-link gate, reframed mid-sprint) — `.github/workflows/ci.yml:100-113`.
  Reframe honestly captured in plan's T2 section. ✓
- T3 (cli/run.rs Source::Link scoping doc-comment) — `run.rs:16-34`. ✓
- T4 (drop agogo-cli binary alias) — `cli/Cargo.toml`. ✓
- T5 (`#[bpaf(version)]`) — `cli/src/main.rs:62`. ✓
- T6 (`#[doc(hidden)]` on `CvRole`/`DinRole`) — `core/src/channel.rs:24-25`. ✓

No code in the diff outside the plan's scope.

### Risks

- `apply_snap_offsets` uses `debug_assert_eq!` + `.zip` for arity
  mismatch. In release builds, mismatched lengths silently process
  `min(len1, len2)` channels with no diagnostic. Doc-commented as
  intentional. Raised as a follow-up: `assert_eq!` would fail loud
  in release for a programming error in an internal helper.
- The `cli-no-link` CI job installs `libasound2-dev` on Linux —
  necessary because `--features cpal,midi` pulls in `agogo-host-cpal`
  which links cpal which needs ALSA on Linux. Verified, not spurious.
- No new dependencies, no unsafe, no path/injection vectors.
- `doc/todo.md` sweep recommended as a separate `doc:` commit after
  merge (closes 4–6 items). Tracked in plan's Review section.

### Recommendations

**Auto-applied this round:**

1. `crates/host-link/src/session.rs:387-394, :428-440` — added
   liveness guards to the two comparison tests so a no-op
   `apply_snap_offsets` would now fail them. Commit `994129f`.
2. Two commit prefixes rewritten via `git filter-branch`: `chore:`
   → `task:` on the polish commit; `ci:` → `task:` on the no-link
   gate. All eight commits now use accepted prefixes per CLAUDE.md.

**Follow-up (deferred):**

3. `host-link/src/session.rs:236` — `debug_assert_eq!` arity
   guard could promote to `assert_eq!` for release-build safety.
   Judgment call; document either way.
4. Add a `host-link --features rusty-link` CI job (cmake on
   runners) so `apply_snap_offsets`'s tests run in CI, not just
   locally.
5. Sweep `doc/todo.md` to mark closed items: snap_intent wiring,
   `agogo-cli` alias removal, `bpaf(version)` flag, `channel.rs`
   re-export audit, plus the two stale items the T2 exploration
   confirmed already done (Tempo→f64 link.rs sweep, PLL Tempo
   `abs_diff` collapse).
