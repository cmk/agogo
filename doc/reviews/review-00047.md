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
