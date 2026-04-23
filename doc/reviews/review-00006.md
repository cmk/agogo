# PR #6 — Link read-only (structural slice)

## Summary

Lands the workspace + API scaffolding for Ableton Link integration in
`agogo` without committing to the numeric shape of
`phase_at_sample`. The fixed-point refactor happening on
`plan/2026-04-23-03` (the companion fxp branch) will settle the
`Phase` / rate-typed `Sample` types; this PR sets up every other
moving part so the follow-up sprint can drop the bridge in place.

### What's new

- **`crates/host-link/`** — new sibling crate wrapping `rusty_link`
  (pinned at `5b3f44e81b0aa30dae4b0650b2f5882048c0b842` via path dep to
  `ext/rusty_link`). `#![forbid(unsafe_code)]`; rusty_link is
  safe-fronted so we never reach for `unsafe` ourselves.
- **`PhaseSourceImpl` trait + `PhaseSource::Custom(...)` variant** in
  `agogo_core::sync::source`. The extension point for sibling-crate
  clock sources (Link first, DIN or MIDI-clock later). Documented
  RT-safety contract; delegate-through in the enum's
  `phase_at_sample` / `feed_samples`.
- **`LinkClock` lifecycle** — `new`, `enable`, `is_enabled`, `tempo`,
  `num_peers`. All reads go through the rusty_link RT-safe
  `capture_audio_session_state` path with a reused `SessionState`
  scratch buffer. Construction and `enable` are documented
  not-RT-safe (socket + thread setup on Link's C++ side).
- **`agogo link probe` CLI subcommand**, feature-gated behind
  `--features link`. Emits CSV `t_ms,peers,tempo_bpm` at a
  configurable period for a configurable duration. No phase column
  yet — added in the follow-up sprint.
- **Workspace plumbing** — `crates/host-link` added as a member,
  `rusty_link` + `agogo-host-link` in `[workspace.dependencies]`,
  `ext/` added to `workspace.exclude` so third-party path deps don't
  drag their upstream doc-tests into our CI.
- **`crates/host-link/README.md`** with setup instructions for cloning
  `rusty_link` at the pinned rev with submodules — `ext/` is
  gitignored, each worktree/CI runner clones it explicitly.

### Deliberately deferred

- **`LinkClock::phase_at_sample` body.** Ships as `unimplemented!()`
  with a matching `#[should_panic]` test. The Tick ↔ Host-Time bridge
  needs the fxp refactor's `Phase` / `Sample` types to settle first.
  Plan 07-b (a follow-up sprint) drops the body in place plus the
  numerical properties.
- **Tempo push-back, transport FSM, quantum snap** — Plan 08 scope.
- **CI for the `link` feature** — requires a CMake + C++ toolchain in
  the CI image plus resolving the gitignored-`ext/` clone step.
  Separate chore; default CI builds stay lean.

### Verification

- `cargo build --workspace` — clean (no `link`).
- `cargo build -p agogo-cli --features link` — clean, links rusty_link.
- `cargo test --workspace` — 186 core + 11 CLI + 6 host-link tests
  green, 1 pre-existing ignore.
- `cargo test --workspace --features link` — adds the `link_probe`
  smoke test (1 more, green).
- `cargo clippy --all-targets --features link -- -D warnings` — clean.
- E2E (manual, local-only): `cargo run -p agogo-cli --features link
  -- link probe --duration-ms 500 --period-ms 100` prints five rows
  of CSV with `peers=0` (no Live/LinkHut on LAN) and tempo stable at
  120 BPM.

### Deviations from the plan

Full list in `doc/plans/plan-2026-04-23-04.md` §Review. Three points:

1. Added `workspace.exclude = ["ext"]` — not in T0's file list, but
   necessary to keep rusty_link's upstream doc-tests out of
   `cargo test --workspace`.
2. `parse_positive_f64` was already in the CLI from Plan 03; reused
   rather than re-added.
3. T4 (tests) merged into T1 and T2 commits per repo convention
   (tests ship with their module).

### Out of scope / follow-ups

- **Plan 07-b (post-fxp phase bridge)** — replace
  `LinkClock::phase_at_sample`'s body with the `HostTimeAnchor` +
  sample → host-µs bridge; add monotonicity and anchor-respect
  property tests; add a `phase` column to the CLI probe.
- **Plan 08 (bidirectional)** — tempo push, minimal transport FSM
  via `rust-fsm 0.7`, quantum snap at the Channel layer, per-buffer
  host-time re-anchoring, two-peer integration tests.
- **Link CI** — separate chore once the CMake + C++ story is ready.

## Local review (2026-04-23)

**Branch:** plan/2026-04-23-04
**Commits:** 6 (origin/main..plan/2026-04-23-04)
**Reviewer:** Claude (sonnet, independent)

---

## Commit Hygiene

All six commits have conventional subjects with scope where appropriate,
stay under 72 characters, and follow the `plan → feat → doc` ordering
the workflow requires. Each commit that adds code (T0-T3) includes its
own tests — no commit lands a module without coverage. No merge
commits; history is linear.

One minor observation: the plan's Context section says `phase_at_sample`
ships as a `todo!()` stub (three places in the doc), but the code uses
`unimplemented!()`. Both expand to the same macro family and both
panic, so this is a doc/code vocabulary inconsistency, not a bug.

## Critical Issues

None.

## Important Issues

### 1. `probe` loop can spin-OOM when `period_ms == 0`

File: `crates/cli/src/main.rs`, lines 128-149 (`link_probe::probe`).

`period_ms` is parsed by `parse_positive_u32`, which rejects `0` with an
error. The guard is correctly at the CLI boundary. However, `probe` is
a `pub fn` callable directly from Rust code (e.g. from the test at line
160). If a caller passes `period_ms = 0` directly, `Duration::from_millis(0)`
is valid and `sleep(Duration::ZERO)` is a no-op. With `duration_ms > 0`
the loop runs unthrottled, allocating `ProbeRow` entries as fast as the
thread can turn until `start.elapsed() > duration`. For a 3-second
default that could be tens of millions of rows before the OOM killer
intervenes.

The `pub` visibility makes this a real API surface, not just an internal
implementation detail.

Fix: add an assertion or saturating clamp at the top of `probe`:
```rust
let period_ms = period_ms.max(1);
```
or make the function's contract explicit with a debug assertion:
```rust
debug_assert!(period_ms > 0, "period_ms must be ≥ 1");
```

### 2. `t_ms: elapsed.as_millis() as u32` silently truncates after ~49 days

File: `crates/cli/src/main.rs`, line 322.

`Duration::as_millis()` returns `u128`. Casting to `u32` truncates
silently above ~49.7 days. The default `duration_ms` is 3 seconds, so
this is not a practical concern in normal use. But the `duration_ms`
parameter is a `u32` (max ~49.7 days in ms), and a user who passes
`--duration-ms 4294967295` would get a probe that runs for ~49 days
with the `t_ms` column wrapping to 0 around the midpoint, producing a
CSV that is silently incorrect.

Fix: either truncate `duration_ms` at the CLI to a practical limit
(e.g. 600_000 ms = 10 minutes) via a custom parser, or use `u64` for
`t_ms` in `ProbeRow`.

### 3. `is_enabled_tracks_enable_call` test touches the network

File: `crates/host-link/src/link.rs`, lines 126-132.

Calling `c.enable(true)` starts Link's peer-discovery UDP listener. The
plan's T4 item 4 noted "does touch the network briefly; if that's a CI
issue, gate behind `fixture_or_skip!` — to be decided during
implementation." The implementation decision was to ship without the
gate. The `link` feature is excluded from default CI builds, so this is
blocked from running in normal CI. But any developer who runs `cargo
test -p agogo-host-link` (or CI once the `link` feature is added) will
open a UDP socket and multicast join in the test suite. On a
locked-down CI box (no multicast, restricted sockets) this can fail or
block.

Fix (minimum): add a comment in the test and in the Review section
explaining that this test requires multicast socket access and will be
moved behind a `fixture_or_skip!`-style network gate in Plan 08. No
functional change needed before push if the `link` feature is not in
CI.

## Code Quality Notes (no blocking issues)

- `#![forbid(unsafe_code)]` present at both new crate roots.
- Modern module layout correct (no `mod.rs`).
- `PhaseSource::Custom` `Send`-bound propagation is safe — existing
  variants were already implicitly `Send`.
- Exhaustive match sites for `PhaseSource` (the two methods on the
  enum) are both updated with the `Custom` arm. No other match sites
  in the diff.
- `workspace.exclude = ["ext"]` — correct; does not affect
  `cargo-deny` license checks (operates on resolved deps, not members).
- `LinkClock::new -> Self` (not `Result`) matches plan + rusty_link.
- `phase_at_sample_panics_until_plan_07b` regression guard is useful;
  doesn't fossilize the deferral (Plan 07-b replaces the test body).
- `phase_source_custom_dispatches` correctly pins both dispatch paths
  and argument pass-through via `Arc<AtomicU64>` counters.

## Plan Conformance

T0–T3 deliverables all present. Files match the plan's file list. The
three documented deviations (workspace `exclude`, reuse of
`parse_positive_f64`, T4 merged into T1/T2) are accurate descriptions
of what actually happened. `set_anchor` is correctly deferred per the
plan (listed under §Deferred phase-bridge work).

The plan labels `phase_source_custom_dispatches` as a "property" in the
Verification table, but it is a deterministic unit test, not a
proptest property. The plan's body text says "No property tests this
sprint" — the table heading is a leftover category label. Test itself
does what the table row says it must do.

## Must Fix Before Push

1. **`link_probe::probe` is unguarded against `period_ms == 0` from
   non-CLI callers.** Add a `debug_assert!(period_ms > 0)` or
   `let period_ms = period_ms.max(1)` at the top of `probe`.
2. **`t_ms` field in `ProbeRow` will silently truncate for large
   `duration_ms`.** Change `ProbeRow::t_ms` to `u64` and the cast to
   `elapsed.as_millis() as u64`, or cap `duration_ms` at the CLI.

## Follow-Up (Future Work)

- `is_enabled_tracks_enable_call` and `probe_emits_rows_and_keeps_initial_tempo`
  should gain a network-availability guard before CI enables the `link`
  feature. Track in Plan 08 or a standalone deferred item.
- `unimplemented!()` vs `todo!()` in `LinkClock::phase_at_sample`:
  swap to `todo!()` to match the plan's language. One-word change,
  fold into the next plan branch's first commit as a nit.
- The plan's Verification table calls `phase_source_custom_dispatches`
  a "property" while the plan body says "No property tests this sprint."
  Worth retitling the table column to "Spot checks / delegation proofs"
  in Plan 07-b.

<!-- gh-id: 3130342096 -->
### Copilot on [`crates/host-link/src/link.rs:12`](https://github.com/cmk/agogo/pull/6#discussion_r3130342096) (2026-04-23 11:12 UTC)

The module-level docs say peer/tempo reads go through `capture_audio_session_state` and are lock-free, but `is_enabled()` and `num_peers()` currently call `AblLink::{is_enabled,num_peers}` directly and don't use the `SessionState` scratch buffer. Either update the docs to match the implementation, or route these reads through the captured `SessionState` (and adjust method signatures as needed) so the RT-safety story is consistent.
```suggestion
//! networking threads. The remaining read-only queries are RT-safe;
//! `tempo()` captures audio session state via
//! `capture_audio_session_state`, which is lock-free on Link's C++
//! side.
```

<!-- gh-id: 3130342170 -->
### Copilot on [`crates/host-link/src/lib.rs:9`](https://github.com/cmk/agogo/pull/6#discussion_r3130342170) (2026-04-23 11:12 UTC)

Crate docs say `LinkClock` "in later sprints — will implement `agogo_core::sync::PhaseSourceImpl`", but `LinkClock` already implements `PhaseSourceImpl` in this PR (with `phase_at_sample` deferred via `todo!()`). Update the rustdoc to avoid misleading readers about the current API surface.
```suggestion
//! a `LinkClock` that implements `agogo_core::sync::PhaseSourceImpl`.
//! This sprint ships the lifecycle surface (enable / tempo /
//! num_peers); the `phase_at_sample` bridge remains deferred post-fxp.
```

<!-- gh-id: 3130342198 -->
### Copilot on [`crates/cli/src/main.rs:337`](https://github.com/cmk/agogo/pull/6#discussion_r3130342198) (2026-04-23 11:12 UTC)

`link_probe::probe` buffers the entire run into a `Vec<ProbeRow>` and `main` prints after the probe completes. That means the command doesn't actually *emit* CSV at the requested period (no incremental output), and long `duration_ms/period_ms` combinations can cause very large allocations/OOM. Consider streaming rows directly to stdout inside the loop (or accept a writer/callback/iterator) so output is produced in real time and memory stays bounded.

<!-- gh-id: 3130342220 -->
### Copilot on [`doc/plans/plan-2026-04-23-04.md:27`](https://github.com/cmk/agogo/pull/6#discussion_r3130342220) (2026-04-23 11:12 UTC)

The plan references a local absolute path (`/Users/cmk/.claude/plans/lets-do-a-bit-hashed-dream.md`) as the "approved design doc" (also repeated later in the recommendations section). This link won't resolve for other contributors/CI artifacts; please replace it with a repo-relative path/URL, or inline the relevant design points so the plan is self-contained.
```suggestion
This sprint lands Sprint A of the Link integration's structural slice.
The approved design constraints relevant to this sprint are summarized
below so the plan remains self-contained. Full Sprint A would also
implement the phase bridge (host-time anchor, `phase_at_sample(n)`,
monotonicity property tests). That path touches `f32`/`f64` arithmetic
on phase values — the same surface the concurrent fxp refactor
(`plan/2026-04-23-03`, commits `6251586 plan:` and `ed03f9e feat(core): fxp shim`)
is replacing with rate-typed sample + decimal time tiers. Shipping both
against each other risks conflict; **this plan ships only the
non-numerical parts**, leaves `LinkClock::phase_at_sample` as a
`todo!()` stub, and queues the numerical slice for a "Plan 07-b"
sprint once fxp merges.
```

<!-- gh-id: 3130342250 -->
### Copilot on [`crates/host-link/src/link.rs:102`](https://github.com/cmk/agogo/pull/6#discussion_r3130342250) (2026-04-23 11:12 UTC)

In `new_does_not_panic_across_bpm_range`, the comment says dropping without enabling means "no network I/O happens", but earlier docs in this module state `LinkClock::new` opens a UDP socket and spawns Link threads. Consider rewording to something more precise (e.g., no peer discovery / multicast join) to avoid contradicting the RT-safety notes.
```suggestion
        // peer discovery / multicast join never starts.
```

<!-- gh-id: 3130342280 -->
### Copilot on [`Cargo.toml`](https://github.com/cmk/agogo/pull/6#discussion_r3130342280) (2026-04-23 11:12 UTC)

`crates/host-link` is a workspace member and depends on `rusty_link = { path = "ext/rusty_link" }`, but `ext/` is gitignored. As a result, a fresh clone (and current GitHub Actions CI which runs `cargo test --workspace`) will fail unless `ext/rusty_link` is manually populated and a C++/CMake toolchain is present. To keep default CI/builds lean as described, either (a) remove `crates/host-link` from `workspace.members` (build it only when pulled in via the CLI `link` feature), (b) change CI to exclude this crate from default workspace builds, or (c) switch `rusty_link` to a fetchable dependency (git rev) so `cargo` can obtain it automatically.

<!-- gh-id: 4161890133 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-04-23 11:12 UTC](https://github.com/cmk/agogo/pull/6#pullrequestreview-4161890133))

## Pull request overview

Introduces scaffolding for a read-only Ableton Link integration, adding a new host-side crate and extending `agogo-core`’s phase source abstraction to allow sibling-crate clock sources, plus a feature-gated CLI probe command.

**Changes:**
- Add `agogo-host-link` crate (LinkClock lifecycle + tests) and workspace wiring for the `rusty_link` binding.
- Add `PhaseSourceImpl` trait and `PhaseSource::Custom(...)` variant with delegation + a unit test in `agogo-core`.
- Add `agogo link probe` CLI subcommand behind `--features link`.

### Reviewed changes

Copilot reviewed 11 out of 12 changed files in this pull request and generated 6 comments.

<details>
<summary>Show a summary per file</summary>

| File | Description |
| ---- | ----------- |
| `Cargo.toml` | Adds `crates/host-link` to workspace + introduces `ext/` exclusion and `rusty_link` path dep. |
| `crates/cli/Cargo.toml` | Adds `link` feature and optional dependency on `agogo-host-link`. |
| `crates/cli/src/main.rs` | Adds `link probe` command and the `link_probe` module. |
| `crates/core/src/sync/source.rs` | Adds `PhaseSourceImpl` + `Custom` variant and delegates calls; adds a dispatch test. |
| `crates/core/src/sync.rs` | Re-exports `PhaseSourceImpl`. |
| `crates/host-link/Cargo.toml` | Defines new `agogo-host-link` crate and dependencies. |
| `crates/host-link/src/lib.rs` | New crate root; exports `LinkClock`. |
| `crates/host-link/src/link.rs` | Implements `LinkClock` lifecycle + deferred `phase_at_sample` + tests. |
| `crates/host-link/README.md` | Documents how to populate `ext/rusty_link` and build with the feature gate. |
| `doc/plans/plan-2026-04-23-04.md` | Adds Plan 07 implementation plan and review notes. |
| `doc/reviews/review-00005.md` | Adds PR review record / summary of the change set. |
</details>






<!-- gh-id: 3130475328 -->
#### ↳ cmk ([2026-04-23 11:39 UTC](https://github.com/cmk/agogo/pull/6#discussion_r3130475328))

Updated the module-level RT-safety doc to distinguish `tempo()` (which does go through `capture_audio_session_state`) from `is_enabled()` / `num_peers()` (which call AblLink's own RT-safe atomic reads). Commit b67f9b3.

<!-- gh-id: 3130475756 -->
#### ↳ cmk ([2026-04-23 11:39 UTC](https://github.com/cmk/agogo/pull/6#discussion_r3130475756))

Applied — the crate doc now says LinkClock implements `PhaseSourceImpl` (with `phase_at_sample` `todo!()`-deferred until fxp lands). Commit b67f9b3.

<!-- gh-id: 3130476110 -->
#### ↳ cmk ([2026-04-23 11:39 UTC](https://github.com/cmk/agogo/pull/6#discussion_r3130476110))

Good catch. Refactored `link_probe::probe` to take a `FnMut(ProbeRow)` callback so the CLI streams rows to stdout as they land; memory stays bounded regardless of duration/period. Test threads a `Vec::push`-ing closure. Commit b67f9b3.

<!-- gh-id: 3130476549 -->
#### ↳ cmk ([2026-04-23 11:39 UTC](https://github.com/cmk/agogo/pull/6#discussion_r3130476549))

Right, the `/Users/cmk/...` path was an artifact of the planning session. Inlined the design constraints directly into the plan's Context section, and rewrote the Plan 07-b recommendations to spell out the bridge math rather than linking to an external file. Commit b67f9b3.

<!-- gh-id: 3130477251 -->
#### ↳ cmk ([2026-04-23 11:39 UTC](https://github.com/cmk/agogo/pull/6#discussion_r3130477251))

Applied the suggested rewording. The comment now clarifies that "no network I/O" means no peer discovery / multicast join, while acknowledging that Link's C++ side still opens a UDP socket on construction. Commit b67f9b3.

<!-- gh-id: 3130477741 -->
#### ↳ cmk ([2026-04-23 11:39 UTC](https://github.com/cmk/agogo/pull/6#discussion_r3130477741))

Good diagnosis — already landed in commit 3f19519 (one behind this review). Applied option (a): removed `crates/host-link` from `[workspace].members` so default `cargo test --workspace` never resolves its deps. Also switched from the `ext/rusty_link` path dep to the crates.io release `rusty_link = "=0.4.8"` (the published crate bundles the Ableton Link C++ submodule, so local `ext/` population is no longer needed). The remaining deny failure (GPL-2.0-or-later on rusty_link) is addressed here in b67f9b3 via an explicit per-crate exception in `deny.toml`.
