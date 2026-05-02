# PR #48 — Plan 2026-04-29-01: Reorganize crates/core into five top-level modules

## Summary

Collapse the ad-hoc top-level layout (`boundary`, `channel`,
`dsl`, `host`, `machine`, `midi`, `out`, `sync`, `testing`,
`time`) in `crates/core/src/` into five intent-named layers —
`conn`, `time`, `channel`, `control`, `sink` — plus a `test`
rename of `testing`. The previous seams weren't load-bearing
(`dsl` and `machine/spec` were both channel configuration;
`host` and `out` were both sinks; `sync` and `machine` were both
control plane; `boundary` / `midi` / parts of `time` were all
conn-shaped value types).

The PR also adds a layering rule enforced by a new
`scripts/check-layers.sh` so the structure stays clean. Each
top-level module-root file declares its allowed deps in a
sentinel header comment; the script parses these and fails on
any back-edge in production code (column-0 imports). Test-block
imports inside `#[cfg(test)] mod tests { … }` are allowed to
cross layers because integration tests legitimately need to wire
pieces together.

### What moved (eight atomic commits)

- **T1** — `testing.rs` → `test.rs` (smallest rename, warmup).
- **T2** — `boundary` / `midi` / `time::{decimal, float, sample,
  tempo}` / `sync::phase` → `conn/`. `decimal.rs` renames to
  `fixed.rs`. Phase and Tempo move with the rest of the
  conn-shaped value types so `conn` is a layering leaf.
- **T3** — `sync::sample_tick::SampleTickConn` merged into
  `time::conn` (where the other `Tick`-flavored Galois conns
  live). The "no tempo coupling on time/" prose convention was
  relaxed because Tempo itself is now in conn and the layering
  rule pins the partial order more strictly than prose did.
- **T4** — Per-type `arb.rs` files consolidated into
  `time/arb.rs` and `conn/arb.rs`. CLAUDE.md's
  "Strategies are colocated with the type" rule relaxed to
  "one arb file per top-level module".
- **T5** — `dsl/`, `dsl.rs`, `machine/spec/`, `machine/spec.rs`
  all move under `channel/`. `channel/transform.rs` renames to
  `channel/time.rs`.
- **T6** — `machine.rs` → `control.rs`, `sync.rs` →
  `control/sync.rs`, `sync/` → `control/sync/`,
  `pulse_train.rs` → `pulse.rs`, `host.rs` → `sink/audio.rs`,
  `out/midi.rs` → `sink/midi.rs`, `channel/scheduler.rs` →
  `control/event.rs`. `out.rs` deleted.
- **T7** — Layering enforcement: sentinel headers on each
  module-root, `scripts/check-layers.sh`, wired into
  `.githooks/pre-commit` and `.github/workflows/ci.yml`,
  smoke-tested with an injected back-edge.
- **T8** — Sweep `doc/plans/plan-2026-04-28-*.md`,
  `doc/reviews/`, CLAUDE.md, scripts/ for path references that
  the rename invalidated.

### Why the layering rule

`crates/core/src/lib.rs` shrinks from 11 `pub mod` declarations
to 6. The "can-A-import-B?" question gets a mechanical answer:
just check the declared `depends-on:` list.

```
control  → sink, channel, time, conn
sink     → channel, time, conn
channel  → time, conn
time     → conn
conn     → (leaf)
test     → (leaf)
```

### Verification

- `cargo build --workspace` clean after each of T1..T8 (each
  commit individually green per the no-red-suite rule).
- `cargo test --workspace` — 950 unit + 17 integration + 1 doc
  test, all passing, no `#[ignore]`.
- `cargo clippy --all-targets -- -D warnings` clean.
- `scripts/check-floats.sh` passes (ALLOWED list updated to new
  paths in T2, T5, T6).
- `scripts/check-layers.sh` passes; smoke-tested by injecting
  `use crate::control::sync::pll::Pll;` into
  `crates/core/src/conn/fixed.rs` (a back-edge), confirming the
  script fails; reverting confirms it passes.
- `cargo doc --workspace --no-deps` clean.

### Notes for reviewers

- This is a rename-only PR. No behavioral changes, no API
  consolidation, no new conn types, no `Conn::then` work.
- No compatibility shims for old paths
  (`agogo_core::testing`, `agogo_core::boundary`,
  `agogo_core::time::tempo`, `agogo_core::sync::phase`, etc.)
  — the compiler is the migration tool; downstream call sites
  get updated atomically inside each task's commit.
- T8's path sweep is mechanical; plan-2026-04-28-03's
  narrative description of the `sample_tick → sync` transition
  is intentionally preserved (that plan moved the file *to*
  sync; Plan 2026-04-29-01 T3 reversed it; both statements are
  historically accurate).
- Future-plan refs (plan-2026-04-28-10's `out/audio.rs`
  renderer, plan-2026-04-28-11's `sync/lpf_pid.rs` PID wrapper)
  were rewritten to their new-layout equivalents
  (`sink/audio.rs`, `control/sync/lpf_pid.rs`); the renderer's
  path now collides with `host.rs → sink/audio.rs` and will
  need editorial revision when implementation lands. Flagged
  in the plan's Review section.

## Local review (2026-04-29)

**Branch:** plan/2026-04-29-01
**Commits:** 10 (origin/main..plan/2026-04-29-01)
**Reviewer:** Claude (sonnet, independent)

---

### Commit Hygiene

All ten commits use valid conventional prefixes (`plan:`, `feat(core):`, `task(scripts):`, `doc:`). The ordering follows the dependency graph: T1→T2→T3→T4→T5→T6→T7→T8→finalize. The commit subjects are accurate descriptions of what each commit does. No merge commits. The `doc: Finalize plan 29-01 and PR description` commit correctly precedes the sprint review, as required by CLAUDE.md step 7.

One small oddity: the T7 commit (`task(scripts): Add check-layers.sh enforcing module partial order`) creates `check-layers.sh` after all the module moves are done. The script passes green on that commit and every subsequent one. Order is sound.

### Code Quality

**`scripts/check-layers.sh` — heuristic gaps (follow-up, not must-fix)**

The script's grep pattern is `^use[[:space:]]+(crate|agogo_core)::[a-z]`. This catches bare `use` statements but misses two forms:

1. **`pub use crate::<layer>::...`** — the leading `pub` means the line starts with `pub`, not `use`, so it's invisible to the gate. Currently there are no column-0 `pub use crate::` lines in `crates/core/src/`, so the gate passes correctly today. But someone adding a layer-violating re-export (`pub use crate::control::Playhead;` inside `conn/`) would sail through.

2. **Grouped imports** — `use crate::{conn::fixed::Pico, control::Playhead};` at column 0 starts with `use crate::{`, where `{` does not match `[a-z]`, so the grep never fires. This form does not appear in the current code, but it's a known gap.

Neither gap triggers a false negative right now (confirmed: `scripts/check-layers.sh` passes clean, and `grep -rn "^pub use crate::"` returns nothing in `crates/core/src/`). Both are structural blind spots to document.

**Leaf-layer sentinel parsing is correct.** `conn.rs` and `test.rs` have `//! depends-on:` with no trailing content. `parse_deps` returns `""`, `[[ -n "$deps" ]]` is false, and `authorised` contains only the self-layer — correctly treating these as leaves.

**`time::conn::SampleTickConn` merge (T3) — clean.** The old `time/conn.rs` had a placeholder comment "moved to `crate::sync::sample_tick`"; the diff replaces it with the actual implementation. The `sample_tick_tests` module is a separate `#[cfg(test)]` block (not `testkit`-gated) so these tests are test-only, which is correct since `SampleTickConn` itself is public. The proptest regression seed from `proptest-regressions/sync/sample_tick.txt` was appended to `proptest-regressions/time/conn.txt` before the source file was deleted — confirmed in the diff.

**`conn/boundary.rs` stays within the conn layer.** All column-0 `use crate::` imports in `conn/` reference `crate::conn::*` only. `Phase` and `Tempo` both moved to `conn/phase.rs` and `conn/tempo.rs` respectively, so there are no back-edges from `conn/boundary.rs` into `time` or `control`.

**`agogo_core::channel::time` vs `agogo_core::time` coexistence.** The plan's review section addresses this: `cargo doc --workspace --no-deps` is clean, no shadowing at call sites. Rustdoc renders them as distinct paths (`agogo_core::channel::time` for the transform pipeline, `agogo_core::time` for the grid algebra). Not a runtime issue, and doc verification is in the plan's spot-check table.

**Stale paths in crate READMEs (follow-up, gardener rule):**

- `crates/host-midi/README.md:3` — references `agogo_core::out::midi::MidiSink`. Should be `agogo_core::sink::midi::MidiSink`.
- `crates/host-cpal/README.md:3` — references `agogo_core::host::AudioHost`. Should be `agogo_core::sink::audio::AudioHost`.

T8's stated scope was `doc/plans/plan-2026-04-28-*.md`, `CLAUDE.md`, and `scripts/`. The crate-level READMEs were outside scope. The plan's Verification table requires `git grep crates/core/src/{boundary,...}` to return zero — that grep doesn't cover README prose, so this slipped through the stated gate.

**Stale path in `CLAUDE.md:338`** — `crate::time::tempo::arb::arb_bpm` is cited as the old-bloated-path example motivating the new convention. The plan's review section explicitly documents this as intentional ("Sed didn't touch it"). The path points to no longer extant code, but it's used rhetorically to name what was bad, not to direct a reader to production code. Acceptable as-is; a future maintenance pass could rephrase to make it clearer it's a historical example.

**Historical plans (04-23 through 04-27) still reference old paths** (`crates/core/src/sync/...`, `machine/...`, `out/...`). These were outside T8's scope and are historical-record documents, not live references. Consistent with the plan's stated scope; flagging only for awareness.

**`pulse_train` fn inside `pulse` module** (`control::sync::pulse::pulse_train`) — plan's review section acknowledges the slight redundancy and defers the fn rename. The `pub use` chain is intact; callers compile correctly.

### Test Coverage

**Properties at new paths.** The existing proptest suites in `time/conn.rs`, `conn/arb.rs`, `time/arb.rs` all compile and the plan confirms `cargo test --workspace` is green. The consolidated `arb` files contain all strategies needed by the test modules, and the call-site `use` paths were updated correctly (`crate::time::arb::arb_grid`, `crate::conn::arb::arb_rational_nonneg`, etc.).

**`arb_integer_stc` — bounded generator, documented (minor concern).**

`arb_integer_stc` is a `prop_oneof!` of seven `Just(...)` instances — it tests only seven `(sr, bpm, ppqn)` triples, all chosen to make `inner` return an integer number of samples. The `sample_tick_round_trip` and `sample_tick_monotonic` tests use only this bounded strategy. CLAUDE.md prohibits bounding generators to avoid the problem region; the comment correctly documents why the bound exists ("needed for the round-trip property") and points to `sample_tick_inner_saturates_on_overflow` as the complementary pathological-region test. The saturation proptest directly targets the overflow region with a wide tick range (`u32::MAX/2..=u32::MAX`), satisfying the CLAUDE.md "separate `#[test]` spot-check at the un-sampled boundary" requirement. However, the `sample_tick_monotonic` and `sample_tick_ceil_ge_floor` properties do not require integer-exact inputs — monotonicity holds for arbitrary `(sr, bpm, ppqn)`. Constraining those two tests to `arb_integer_stc` is narrower than it needs to be, though it's not wrong.

**Arb migration completeness.** `conn/arb.rs` contains `arb_bpm`, `arb_jitter_sigma`, `arb_sample_rate`, `arb_rational_nonneg`, plus the full fixed-point ladder (`fixed_coarse`, `fixed_fine`, `fixed_safe_fine`, `extended_fd00`..`extended_fd12`). `time/arb.rs` contains `arb_grid`, `arb_tbase`, `arb_tick`, `arb_time`, `arb_small_time`, `arb_swing`. The old per-type `pub mod arb;` declarations were removed from `grid.rs`, `tbase.rs`, `tick.rs`, `swing.rs`, confirmed by `grep -rn "pub mod arb" crates/core/src/time/`.

**`sample_tick_tests` uses `#[cfg(test)]` only**, not `#[cfg(any(test, feature = "testkit"))]`. This is correct because `arb_integer_stc` is a local strategy not meant for external callers.

### Plan Conformance

| Task | Implemented | Notes |
|------|------------|-------|
| T1 — `testing.rs` → `test.rs` | ✓ | `fixture_or_skip!` macro updated; `host-link/tests/bidirectional.rs` updated |
| T2 — `conn/` extraction | ✓ | All seven files moved; `CLAUDE.md` path refs updated; `check-floats.sh` ALLOWED updated |
| T3 — `sample_tick` merge | ✓ | Content appended to `time/conn.rs`; regression seed migrated; old file deleted |
| T4 — arb consolidation | ✓ | `conn/arb.rs` and `time/arb.rs` contain all consolidated strategies; old `pub mod arb` declarations removed |
| T5 — `channel/` growth | ✓ | `dsl/` and `machine/spec/` moved; `transform.rs` → `time.rs`; proptest-regressions renamed |
| T6 — `control/` + `sink/` | ✓ | All renames complete; `tick_stream` re-exported from `control`; `pulse_train.rs` → `pulse.rs` |
| T7 — layer enforcement | ✓ | Sentinels in all six module roots; `check-layers.sh` added; wired into pre-commit and CI |
| T8 — doc sweep | ✓ with gaps | `doc/plans/plan-2026-04-28-*.md` clean; crate READMEs not swept (outside stated scope) |

Verification table properties: all existing proptests pass at new paths (CI gate). `SampleTickConn` round-trip holds. `arb_grid` covers `Grid::ALL`. `fixed_safe_fine` boundary coverage preserved. `arb_bpm` covers µBPM range. `f64_phase_to_phase` round-trip preserved.

### Risks

**`pub use` blind spot in `check-layers.sh`.** If a future contributor adds `pub use crate::control::Foo;` at column 0 inside a `conn/` file, the script won't catch it. The workaround is code review (which would catch it), but the script's own header doc doesn't warn about this. Low immediate risk; worth a one-line comment in the script.

**Grouped import blind spot.** Same class: `use crate::{conn::..., control::...};` at column 0 in a conn-layer file would not be flagged. Not present today; same mitigation (code review + the fact that rustfmt keeps grouped imports rare in this codebase).

**Plan-2026-04-28-10's `sink/audio.rs` path collision** — the plan's review section documents this explicitly. The existing `sink/audio.rs` (moved from `host.rs`) now occupies the path plan-10 intended for a CV renderer. Plan-10 will need to use a different name (`sink/cv.rs`). This is tracked; no action needed here.

**Older plan docs (04-23 through 04-27) still contain old paths.** These are historical records, but a future agent reading them for context would see `crates/core/src/sync/pll.rs` and navigate to a path that no longer exists. The consequence is confusion, not a compilation error. Consider a one-time sweep or a note in the plan directory README.

---

### Recommendations

**Must fix before push:**

None. `cargo test --workspace` passes, `check-layers.sh` passes, no new `#[ignore]`, no back-edges in production code.

**Follow-up (future work):**

1. `crates/host-midi/README.md:3` — update `agogo_core::out::midi::MidiSink` → `agogo_core::sink::midi::MidiSink`. `crates/host-cpal/README.md:3` — update `agogo_core::host::AudioHost` → `agogo_core::sink::audio::AudioHost`. Two-line fix; fold into the next touching commit for these crates.

2. `scripts/check-layers.sh` — add a comment noting the two blind spots (`pub use crate::` and grouped imports `use crate::{...}` at column 0). The gaps are inert today but documenting them helps the next person extending the script.

3. `sample_tick_monotonic` and `sample_tick_ceil_ge_floor` — these properties hold for any `SampleTickConn`, not just integer-exact ones. Consider an `arb_any_stc` strategy drawing from `arb_sample_rate()` and `arb_bpm()` from `conn::arb` to give these two properties broader coverage. Not urgent since the existing tests were ported verbatim from pre-move and already caught real bugs.
