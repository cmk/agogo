# PR #37 — Extract 7 inline modules from cli/main.rs

## Summary

`crates/cli/src/main.rs` was 1722 lines, with 887 of those being seven
inline `pub mod` blocks of self-contained handler logic plus a 172-line
EOF `#[cfg(test)] mod tests` block. This is the T11 deliverable from
PR #35's audit (plan-2026-04-28-03 §What's actually disorganized).

After this PR:
- **`main.rs` shrinks to 675 lines** (down from 1722, target was ~650).
  The remaining content is the CLI enum hierarchy + parser fns +
  dispatch match + `main()`.
- Each handler module lives in its own sibling file. Feature
  gating stays on the `mod foo;` declaration in `main.rs` —
  `#[cfg(feature = "link")] pub mod link_probe;` etc. — matching
  how the inline `pub mod` blocks were gated before.
- The EOF `mod tests` block is gone — its three test cohorts
  (sync_trace, time_sched, channel_trace) now live in the respective
  sibling modules' own `#[cfg(test)] mod tests`.

No functional changes. Pure file-layout work — every `pub fn` /
`pub struct` keeps its name and signature; the dispatcher in `main()`
keeps its existing call shape now that each `foo` lives in
`crate::foo`.

### Module → file map

| New file | Lines | Feature gate | Tests moved from EOF block |
|---|---|---|---|
| `crates/cli/src/sync_trace.rs` | 81 | `core` | `sync_trace_converges` (1) |
| `crates/cli/src/channel_trace.rs` | 134 | `core` | `channel_trace_t4_120bpm_matches_expected_samples`, `channel_trace_rejects_invalid_grid` (2) |
| `crates/cli/src/time_sched.rs` | 208 | `core` | `swing_to_config_*` × 5 + `schedule_ticks_*` × 3 (8) |
| `crates/cli/src/link_commands.rs` | 114 | `link` | (none — no inline tests existed) |
| `crates/cli/src/link_probe.rs` | 131 | `link` | (had its own inline `mod tests` already; rode along) |
| `crates/cli/src/midi_trace.rs` | 200 | `core` | (had its own inline `mod tests` already; rode along) |
| `crates/cli/src/demo.rs` | 224 | `demo` | (none — no inline tests existed) |

### Other changes

- **`scripts/check-floats.sh` allowlist updated.** The f64 surface
  inherited by `sync_trace.rs`, `time_sched.rs`, and `link_probe.rs`
  was previously inside `cli/main.rs`'s allowlisted scope; the three
  new files inherit the same eligibility. Added to `ALLOWED` (20
  total entries now) with a one-line justification per file. CLAUDE.md
  amended to keep the rule + gate in sync.
- **EOF `#[cfg(all(test, feature = "core"))] mod tests` deleted.**
  After T1+T2+T3 absorbed its contents, the block was empty; T3's
  commit folded the deletion in.

### Verification

| Check | Result |
|---|---|
| `cargo build --workspace --all-features` | green at every commit |
| `cargo test --workspace --all-features` | 940 + 39 + 39 + 1 = 1019, 2 ignored, 0 failed (unchanged) |
| `cargo test -p agogo-host-link --features rusty-link` | 31 + 4 = 35, 0 failed |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | clean |
| `scripts/check-floats.sh` | OK (3 new allowlist entries) |
| `wc -l crates/cli/src/main.rs` | 675 (down from 1722) |
| `grep -nE "^( *)(pub )?mod [a-z_]+\\s*\\{" crates/cli/src/main.rs` | 0 inline-module bodies |

### Commit log

8 commits, one per extraction (T3 collapsed with T8 since the EOF
block emptied at that point):

```
0e74d18 debt: Update check-floats allowlist for cli module extractions
c6779b6 debt: Extract demo from cli/main.rs
ca78e25 debt: Extract midi_trace from cli/main.rs
56381a6 debt: Extract link_probe from cli/main.rs
ebf5d04 debt: Extract link_commands from cli/main.rs
03e95c7 debt: Extract time_sched from cli/main.rs; delete EOF tests block
00abde4 debt: Extract channel_trace from cli/main.rs
e0a00ea debt: Extract sync_trace from cli/main.rs
76885ae plan: Extract 7 inline modules from cli/main.rs
```

### What's deferred

Unchanged from PR #36's deferred list:

- T6 `LpfPid` (clocked-style controller wrapper for v0.5 Link follower)
- T7 `TransportState<S>` typestate skeleton
- T8 `RelativeClock` calibration helper
- T9 `channel/spec.rs` 1299-line split (still its own kitchen sink)
- `compose!` / `ceiling1` body cleanups
- host-link 4-layer wrapping cleanup

## Local review (2026-04-28)

**Branch:** plan/2026-04-28-05
**Commits:** 10 (origin/main..plan/2026-04-28-05)
**Reviewer:** Claude (sonnet, independent)

---

### Commit Hygiene

Ten commits: one `plan:`, eight `debt:`, one `doc:`. Ordering
matches the plan's smallest-first dependency graph (T1 sync_trace
→ T2 channel_trace → T3+T8 time_sched → T4 link_commands → T5
link_probe → T6 midi_trace → T7 demo → allowlist update). Each
commit is scoped to one module extraction; the T3+T8 collapse is
documented in the plan's Review. Atomic and conventional.

### Code Quality

**Pure-move discipline:** clean. Spot-checks against
`channel_trace`, `link_probe`, and `time_sched` confirm the bodies
are verbatim — comments and all. No logic added or altered.

**Imports:** each new file carries its own self-contained `use`
lines. None relies on the parent's scope.

**Feature gates:** `link_probe`/`link_commands` →
`#[cfg(feature = "link")]`; `sync_trace`/`channel_trace` →
`core`; `demo` → `demo`; `midi_trace`/`time_sched` ungated
(matches the originals). Gating is entirely on the `mod`
declaration in `main.rs`, which is the correct approach.

**check-floats allowlist:** the three new entries are genuinely
needed:
- `sync_trace.rs`: `Vec<f32>` PCM input + Hz spacing math
- `time_sched.rs`: `pub swing: f64` argv-boundary
- `link_probe.rs`: test-only Q0.32 → cycles assertion

The other four extracted files (`channel_trace`, `link_commands`,
`midi_trace`, `demo`) contain no `f32`/`f64` and correctly have
no allowlist entries.

**main.rs line count:** 675 (target ~650). Small overage.

### Test Coverage

Test distribution matches the plan: `sync_trace_converges` →
`sync_trace::tests`; two `channel_trace_*` → `channel_trace::tests`;
eight `swing_to_config_*` / `schedule_ticks_*` →
`time_sched::tests`. `link_probe`, `midi_trace` carry their own
inline `mod tests` that rode along verbatim. EOF block deleted.
Test count unchanged.

### Plan Conformance

All seven extractions performed. T8 collapse and allowlist
deviation documented in plan's Review section.

### Risks

**Public API:** `cli` is a binary crate — nothing outside can
reach these modules. No API break.

**Feature-gate consistency:** verified for all seven modules.

### Recommendations

**Must fix before push:** None.

**Follow-up:** None.

<!-- gh-id: 3157301263 -->
### Copilot on [`doc/reviews/review-00037.md:15`](https://github.com/cmk/agogo/pull/37#discussion_r3157301263) (2026-04-28 21:22 UTC)

The Summary says the extracted handler modules use file-level `#[cfg(feature = "...")]`, but this PR actually applies feature gating on the `mod ...;` declarations in `crates/cli/src/main.rs` (and this same file later notes that approach as correct). Please update the Summary bullet to match the implementation to avoid misleading readers.
```suggestion
- Each handler module lives in its own sibling file, with
  `#[cfg(feature = "...")]` applied to the corresponding `mod ...;`
  declarations in `crates/cli/src/main.rs`.
```

<!-- gh-id: 3157301308 -->
### Copilot on [`doc/plans/plan-2026-04-28-05.md:17`](https://github.com/cmk/agogo/pull/37#discussion_r3157301308) (2026-04-28 21:22 UTC)

The Goal section says each extracted handler file has a file-level `#[cfg(feature = "...")]`, but the implementation in this PR uses `#[cfg(feature = "...")] mod foo;` in `crates/cli/src/main.rs`. Since this plan is being added alongside the implementation, updating this bullet to match what shipped will prevent readers from being misled about where feature gating lives.
```suggestion
- Each handler module lives in its own sibling file and remains
  independently feature-gated via `#[cfg(feature = "...")] mod
  foo;` declarations in `main.rs`.
```

<!-- gh-id: 3157301332 -->
### Copilot on [`crates/cli/src/link_probe.rs:115`](https://github.com/cmk/agogo/pull/37#discussion_r3157301332) (2026-04-28 21:22 UTC)

This assertion makes the test environment-dependent: it will fail on any machine/CI runner that can see a real Link peer on the LAN (since `probe()` enables Link networking). To keep the test deterministic, consider gating it behind an env var / `#[ignore]`, and/or removing the `peers == 0` requirement so the test only checks stable properties (e.g., non-empty rows and initial tempo).

<!-- gh-id: 3157301355 -->
### Copilot on [`crates/cli/src/main.rs:671`](https://github.com/cmk/agogo/pull/37#discussion_r3157301355) (2026-04-28 21:22 UTC)

`midi_trace` and `time_sched` are declared without `#[cfg(feature = "core")]`, but both modules depend on `agogo_core` (which is an optional dep behind the `core` feature). As written, `cargo build -p agogo-cli --no-default-features` (or `--features link` without `core`) will fail at compile time even though the command dispatcher has `#[cfg(not(feature = "core"))]` branches suggesting this build is supported. Gate these modules the same way as `sync_trace`/`channel_trace` (and consider applying `all(feature = "link", feature = "core")` to the Link modules too, since they also use `agogo_core`).

<!-- gh-id: 4192530411 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-04-28 21:22 UTC](https://github.com/cmk/agogo/pull/37#pullrequestreview-4192530411))

## Pull request overview

Refactors the `agogo-cli` binary by extracting seven previously-inline handler modules (and their tests) out of `crates/cli/src/main.rs` into dedicated sibling source files, plus updates the float-usage allowlist/docs to match the new file layout.

**Changes:**
- Split handler modules (`sync_trace`, `channel_trace`, `time_sched`, `midi_trace`, `link_commands`, `link_probe`, `demo`) into separate `crates/cli/src/*.rs` files and removed the EOF test block from `main.rs`.
- Moved the relevant test cohorts into the corresponding extracted modules.
- Updated the float gate allowlist (`scripts/check-floats.sh`) and repository convention docs (`CLAUDE.md`), and added plan/review docs for the change.

### Reviewed changes

Copilot reviewed 12 out of 12 changed files in this pull request and generated 5 comments.

<details>
<summary>Show a summary per file</summary>

| File | Description |
| ---- | ----------- |
| scripts/check-floats.sh | Adds new CLI module files to the f32/f64 allowlist after extraction. |
| CLAUDE.md | Updates documented allowlist count/entries to match the script. |
| doc/reviews/review-00037.md | Adds a review record describing the extraction work and verification. |
| doc/plans/plan-2026-04-28-05.md | Adds the plan document for the extraction series. |
| crates/cli/src/main.rs | Replaces inline module bodies with `mod foo;` declarations and removes EOF tests. |
| crates/cli/src/sync_trace.rs | New extracted `agogo sync trace` handler + moved convergence test. |
| crates/cli/src/channel_trace.rs | New extracted `agogo channel trace` handler + moved tests. |
| crates/cli/src/time_sched.rs | New extracted `agogo time sched` handler + moved tests. |
| crates/cli/src/midi_trace.rs | New extracted `agogo midi trace` handler + existing tests. |
| crates/cli/src/link_commands.rs | New extracted Link subcommands implementation. |
| crates/cli/src/link_probe.rs | New extracted Link probe loop + existing network-touching test. |
| crates/cli/src/demo.rs | New extracted end-to-end demo pipeline implementation. |
</details>






<!-- gh-id: 3157301370 -->
### Copilot on [`crates/cli/src/channel_trace.rs:46`](https://github.com/cmk/agogo/pull/37#discussion_r3157301370) (2026-04-28 21:22 UTC)

The comment references `fxp::pico_to_samples`, but the float↔fixed boundary helpers (including `pico_to_samples`) live in `agogo_core::conn::boundary` now (and `fxp.rs` has been deleted). Update this comment to point at the current function/module so readers can find the invariant being enforced here.
```suggestion
    // supported by `agogo_core::conn::boundary::pico_to_samples` (the
    // downstream Pico → Sample dispatch). Validate here rather than
    // letting `micro_to_samples` panic deep inside the transform.
```

<!-- gh-id: 3157330931 -->
#### ↳ cmk ([2026-04-28 21:28 UTC](https://github.com/cmk/agogo/pull/37#discussion_r3157330931))

Done — corrected the Summary to describe gating-on-the-mod-decl, matching what shipped (and what the Code Quality section already says correctly).

<!-- gh-id: 3157331074 -->
#### ↳ cmk ([2026-04-28 21:28 UTC](https://github.com/cmk/agogo/pull/37#discussion_r3157331074))

Done — same correction in the plan's Goal section. Thanks for catching the doc/impl drift across both files.

<!-- gh-id: 3157331223 -->
#### ↳ cmk ([2026-04-28 21:28 UTC](https://github.com/cmk/agogo/pull/37#discussion_r3157331223))

Deferring this one. The `peers == 0` assertion + the Link-network-dependent test predate this PR — they were inside main.rs's inline `mod link_probe { #[cfg(test)] mod tests { ... } }` block before extraction and rode along verbatim per the no-logic-changes-during-reorg discipline. Worth fixing properly with a `fixture_or_skip!("link_multicast")` gate (host-link/tests/bidirectional.rs already uses that pattern) — tracking as a follow-up alongside the host-link 4-layer wrapping cleanup.

<!-- gh-id: 3157331456 -->
#### ↳ cmk ([2026-04-28 21:28 UTC](https://github.com/cmk/agogo/pull/37#discussion_r3157331456))

Deferring — verified pre-existing: `cargo build -p agogo-cli --no-default-features` fails with the same 33 errors against origin/main (before this PR). The inline `pub mod midi_trace { use agogo_core::... }` and `pub mod time_sched { use agogo_core::... }` were also un-gated; this PR moved them verbatim. Real bug, but a behaviour change beyond the structural-only scope. Tracking as a follow-up — likely also needs gating on `MidiSub::Trace` / `TimeOp::Sched` enum variants in the CLI shape, since those are what reach into the modules.

<!-- gh-id: 3157331660 -->
#### ↳ cmk ([2026-04-28 21:28 UTC](https://github.com/cmk/agogo/pull/37#discussion_r3157331660))

Done — updated the comment to point at `agogo_core::conn::boundary::pico_to_samples` (the post-fxp.rs-deletion home from PR #35 T5). Thanks.
