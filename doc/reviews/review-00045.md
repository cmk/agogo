# PR #45 — Distribute kitchen-sink arb.rs to per-type submodules

## Summary

`crates/core/src/arb.rs` aggregated proptest strategies for 9 unrelated
domain types plus a runtime synthetic-signal generator (`pulse_train`).
The kitchen-sink layout violates the colocation pattern the rest of the
workspace and upstream `connections` use: every type owns its own
`arb.rs`, no aggregating root file. This PR distributes the strategies
to per-type `arb.rs` submodules and moves `pulse_train` (which isn't
testkit) to its consumer subsystem.

**Strategy migration (T1).** Each strategy moves to `<type>/arb.rs`
gated `#[cfg(any(test, feature = "testkit"))]`:

| Strategy | New location |
|---|---|
| `arb_bpm` | `time/tempo/arb.rs` |
| `arb_sample_rate` | `time/sample/arb.rs` |
| `arb_jitter_sigma` | `time/decimal/arb.rs` |
| `arb_tbase` | `time/tbase/arb.rs` |
| `arb_grid` | `time/grid/arb.rs` |
| `arb_tick` / `arb_time` / `arb_small_time` | `time/tick/arb.rs` |
| `arb_rational_nonneg` | `time/conn/arb.rs` |
| `arb_swing` | `time/swing/arb.rs` |

Inter-strategy dependencies (`arb_swing → arb_tbase`,
`arb_time → arb_grid`) become explicit cross-module imports.
`arb_integer_stc` was already a private fn inside
`sync/sample_tick.rs::tests` and stays put.

**`pulse_train` migration (T2).** Not testkit (used by `cli/sync_trace`
at runtime), so it moves to `crates/core/src/sync/pulse_train.rs` as a
regular `pub` module — declared in `sync.rs` alongside `pll`, `detect`,
`source`. The three `pulse_train_*` tests carry along inline. The
`scripts/check-floats.sh` allowlist entry follows the file (same
exception class: synthetic-PCM through lawful Conn-inverse helpers).

**Kitchen-sink deletion (T3).** `crates/core/src/arb.rs` is gone,
`pub mod arb;` removed from `lib.rs`. The defensive
`_sample_rate_sealed` bridge dropped — each per-type arb file imports
its needed traits directly.

**CLAUDE.md update.** The proptest discipline section now codifies the
per-type colocation rule (was: "shared across crates live in `arb.rs`";
now: "colocated with the type they generate, no aggregating root
file"). Pattern matches upstream Rust `connections::prop::arb` and the
Haskell `Test/Data/Connection/{Float,Int,…}.hs` layout.

External-call surface change: `agogo_core::arb::pulse_train` →
`agogo_core::sync::pulse_train::pulse_train`. Internal `crate::arb::*`
sites move to per-type paths. The `testkit` feature flag stays;
external proptest consumers (none today) would now do
`agogo_core::time::grid::arb::arb_grid` instead of
`agogo_core::arb::arb_grid` — verbose but truthful.

## Test plan

- [x] `cargo test --workspace` — 950 + 17 + 17 + 1 doctest pass (unchanged)
- [x] `cargo clippy --all-targets -- -D warnings` — clean
- [x] `cargo fmt --all -- --check` — clean
- [x] `scripts/check-floats.sh` — clean (allowlist entry moved with `pulse_train`)
- [x] `cargo build -p agogo-core` (no default features) — confirms testkit gate is correct
- [x] `cargo build -p agogo-core --features testkit` — clean

## Local review (2026-04-28)

**Branch:** plan/2026-04-28-08
**Commits:** 3 (origin/main..plan/2026-04-28-08)
**Reviewer:** Claude (sonnet, independent)

---

### Commit Hygiene

All three commits carry correct prefixes (`plan:`, `debt:`, `doc:`). Landing T1+T2+T3 in a single `debt:` commit is justified — the tasks are strongly interdependent (T3 cannot exist without T1+T2, and splitting them would leave a state where `crate::arb` has holes). The merge would be temporarily broken if split. Atomic landing is the right call.

### Code Quality

**Modern module layout** — all 9 new files follow `<type>.rs` + `<type>/arb.rs` sibling shape. No `mod.rs` files introduced. Correct.

**`#[cfg(any(test, feature = "testkit"))]` gate** — every new `time/*/arb.rs` file is gated at the module declaration site in the parent `.rs` file. Correct.

**`pulse_train.rs` is NOT testkit-gated** — `pub mod pulse_train;` in `sync.rs` carries no `cfg` gate. Correct; `cli/sync_trace` is a runtime consumer.

**`_sample_rate_sealed` bridge** dropped cleanly — each per-type `arb.rs` imports what it needs directly and is only compiled under test/testkit.

**Doc strings** — the 9 new `arb.rs` files each carry a module-level doc comment explaining the gate. Consistent, not boilerplate-padded. Intra-doc links spot-checked: `crate::time::conn::quantize_at`, `crate::time::tick::from_ticks`, `crate::time::conn::TICKTIME`, `crate::time::swing::SwingConfig` — all resolvable.

**`sync/source.rs` qualified-path usage** — both `crate::arb::pulse_train::<S048>` raw-qualified-path calls (not in `use` statements) updated to `crate::sync::pulse_train::pulse_train::<S048>`. Caught by the build, fully resolved.

### Test Coverage

**Three `arb_*_in_range` tests** all migrated correctly:
- `arb_bpm_in_range` → `time/tempo/arb.rs`
- `arb_sample_rate_is_standard` → `time/sample/arb.rs`
- `arb_jitter_in_range` → `time/decimal/arb.rs`

**Three `pulse_train_*` tests** all in `sync/pulse_train.rs`. Net `#[test]` count change: 0 (6 added, 6 removed from deleted `arb.rs`).

### Plan Conformance

**T1** — all 8 strategy migrations completed. `arb_integer_stc` deviation documented in the Review section.

**T2** — `pulse_train` + `PULSE_WIDTH_PS` moved, all import sites updated (pll, detect, source, sync_trace). Complete.

**T3** — `arb.rs` deleted, `pub mod arb;` removed from `lib.rs`, `_sample_rate_sealed` dropped. Complete.

**Plan doc inconsistency — stale Critical Files section** *(confidence: 82)*. As-reviewed, the Critical Files section listed `crates/core/src/sync/sample_tick.rs` as getting a `pub mod arb;` declaration and `sync/sample_tick/arb.rs` as a new file; neither was implemented (`arb_integer_stc` stays inline as a documented deviation). Resolved before push: the auto-fix step pruned the Critical Files entries; a follow-up round-1 also pruned the matching tree-diagram entry at the top of the plan and the dependency graph's T1 line.

### Risks

**`testkit` feature external caller audit** — no `--features testkit` usage in cli/host-link/host-cpal. Only external `agogo_core::arb::*` site was `cli/src/sync_trace.rs`'s `pulse_train` use, handled in T2. Risk: none realised.

**CLAUDE.md wording vs. implementation** — new rule example (`time/grid.rs` declares the mod; `time/grid/arb.rs` holds `arb_grid`) matches the diff exactly.

**`check-floats.sh` allowlist** — old `crates/core/src/arb.rs` entry removed from both the ALLOWED array AND the comment header. New `crates/core/src/sync/pulse_train.rs` added to both. Symmetric.

**`arb_bpm_in_range` bound** — assertion upper bound is inclusive 400_000_000 but the strategy's widest arm stops at 399_999_999 (exclusive range). Inherited from old `arb.rs` unchanged; no regression.

### Recommendations

**Must fix before push:** none.

**Follow-up (future work):**
- `agogo-testkit` re-export crate when external proptest consumers materialize — verbose per-type paths will be ergonomically painful then. Deferred per plan; nothing to do now.
- `time/conn/arb.rs` doc references `super::Whole`. If `Whole` is ever renamed/removed in `conn.rs`, the intra-doc link becomes a rustdoc warning. Low priority.

<!-- gh-id: 3158164982 -->
### Copilot on [`doc/plans/plan-2026-04-28-08.md:52`](https://github.com/cmk/agogo/pull/45#discussion_r3158164982) (2026-04-29 01:43 UTC)

The plan still claims `sync/sample_tick/arb.rs` will be created (both in the “cleaner shape” tree and the dependency graph), but the implementation keeps `arb_integer_stc` as a private helper inside `sync/sample_tick.rs::tests` and there is no `sample_tick/arb.rs` in the crate. Please update these sections to match the actual design so the plan doesn’t point readers at non-existent files.

<!-- gh-id: 3158164994 -->
### Copilot on [`doc/reviews/review-00045.md:110`](https://github.com/cmk/agogo/pull/45#discussion_r3158164994) (2026-04-29 01:43 UTC)

This review note says the plan’s “Critical Files” section still lists `sync/sample_tick/arb.rs` as a new file, but the plan’s Critical Files list doesn’t mention `sample_tick` at all. Please either correct this to point at the actually-stale sections (the plan’s module tree / dependency graph), or remove the claim so the review record stays factually accurate.
```suggestion

```

<!-- gh-id: 4193582294 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-04-29 01:43 UTC](https://github.com/cmk/agogo/pull/45#pullrequestreview-4193582294))

## Pull request overview

This PR removes the `crates/core/src/arb.rs` “kitchen-sink” module by relocating proptest strategies into per-type `arb.rs` submodules (behind `#[cfg(any(test, feature = "testkit"))]`) and moving the runtime `pulse_train` generator into the `sync` subsystem as a regular public API.

**Changes:**
- Split shared proptest strategies into per-type `time/*/arb.rs` modules and update all internal test imports accordingly.
- Move `pulse_train` into `crates/core/src/sync/pulse_train.rs`, update callsites (core tests + CLI), and update the float-check allowlist entry.
- Delete `crates/core/src/arb.rs`, remove `pub mod arb;` from `lib.rs`, and codify the colocation rule in `CLAUDE.md`.

### Reviewed changes

Copilot reviewed 30 out of 30 changed files in this pull request and generated 2 comments.

<details>
<summary>Show a summary per file</summary>

| File | Description |
| ---- | ----------- |
| scripts/check-floats.sh | Updates allowlist/comments to track `pulse_train` after the move. |
| doc/reviews/review-00045.md | Adds a review record for PR #45 (contains a factual mismatch vs the plan doc). |
| doc/plans/plan-2026-04-28-08.md | Adds the plan document for the refactor (contains stale references to a non-existent `sample_tick/arb.rs`). |
| crates/core/src/time/tick/arb.rs | New strategies for `Tick`/`Time` with explicit dependency on `grid::arb`. |
| crates/core/src/time/tick.rs | Updates test imports and declares `pub mod arb` under test/testkit cfg. |
| crates/core/src/time/tempo/arb.rs | New `Tempo` strategy (`arb_bpm`) + migrated proptest. |
| crates/core/src/time/tempo.rs | Declares `pub mod arb` under test/testkit cfg. |
| crates/core/src/time/tbase/arb.rs | New `TBase` strategy (`arb_tbase`). |
| crates/core/src/time/tbase.rs | Updates test imports and declares `pub mod arb` under test/testkit cfg. |
| crates/core/src/time/swing/arb.rs | New `SwingConfig` strategy (`arb_swing`) importing `arb_tbase`. |
| crates/core/src/time/swing.rs | Updates test imports and declares `pub mod arb` under test/testkit cfg. |
| crates/core/src/time/sample/arb.rs | New sample-rate strategy (`arb_sample_rate`) + migrated proptest. |
| crates/core/src/time/sample.rs | Declares `pub mod arb` under test/testkit cfg. |
| crates/core/src/time/grid/arb.rs | New `Grid` strategy (`arb_grid`). |
| crates/core/src/time/grid.rs | Declares `pub mod arb` under test/testkit cfg. |
| crates/core/src/time/decimal/arb.rs | New jitter strategy (`arb_jitter_sigma`) + migrated proptest. |
| crates/core/src/time/decimal.rs | Declares `pub mod arb` under test/testkit cfg. |
| crates/core/src/time/conn/arb.rs | New rational strategy (`arb_rational_nonneg`). |
| crates/core/src/time/conn.rs | Updates test imports and declares `pub mod arb` under test/testkit cfg. |
| crates/core/src/sync/source.rs | Updates `pulse_train` callsites to the new module path. |
| crates/core/src/sync/pulse_train.rs | New home for `pulse_train` + constant + migrated unit tests. |
| crates/core/src/sync/pll.rs | Updates test import of `pulse_train`. |
| crates/core/src/sync/detect.rs | Updates test imports (`pulse_train`, `arb_bpm`). |
| crates/core/src/sync.rs | Exposes `pub mod pulse_train;` and documents the submodule. |
| crates/core/src/lib.rs | Removes `pub mod arb;` from the crate root. |
| crates/core/src/channel/transform.rs | Updates test import of `arb_grid`. |
| crates/core/src/channel/scheduler.rs | Updates test import of `arb_grid`. |
| crates/core/src/arb.rs | Deletes the former kitchen-sink `arb.rs` module. |
| crates/cli/src/sync_trace.rs | Updates runtime import to `agogo_core::sync::pulse_train::pulse_train`. |
| CLAUDE.md | Updates proptest discipline docs to codify per-type `arb` colocation. |
</details>






<!-- gh-id: 3158168269 -->
#### ↳ cmk ([2026-04-29 01:44 UTC](https://github.com/cmk/agogo/pull/45#discussion_r3158168269))

Fixed — pruned the `sync/sample_tick/arb.rs` references from both the cleaner-shape tree (top of plan) and the dependency graph's T1 line. Plan is now consistent with the implementation: `arb_integer_stc` stays inline in `sync/sample_tick.rs::tests`, and no `sample_tick/arb.rs` is created.

<!-- gh-id: 3158168458 -->
#### ↳ cmk ([2026-04-29 01:44 UTC](https://github.com/cmk/agogo/pull/45#discussion_r3158168458))

Fixed — clarified the wording. The Critical Files entry was real at review time but pruned in the auto-fix commit before push; the follow-up round also pruned the matching tree-diagram and dependency-graph entries. The Local review note now points at all three pruned sites and marks the issue resolved.
