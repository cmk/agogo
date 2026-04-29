# PR #43 — Three-mess cleanup: widen Tick, drop Ple, rename Conns

## Summary

Three intertwined cleanups in `crates/core/`:

**1. Widen `Tick` from u32 to u64.** `time_to_tick`'s
`beats: u32 × tick_count: u32` arithmetic could overflow u32; the
existing `checked_mul().expect()` was a backstop, not a fix, and
`arb_time` further dodged the bug by clamping `beats` to
`0..=100_000` — the proptest-coverage-faking anti-pattern called out
in `feedback_proptest_coverage_faking.md`. With the master counter
widened to u64, the panic path is gone (`u64::from(u32) * u64::from(u32)`
fits in u64 with ~32 bits of headroom) and `arb_time` uses
`any::<u32>()` honestly.

`from_ticks` and `from_ticks_floor` become partial — they return
`Option<Time>` because `Time.beats` stays u32, so `Tick(huge)` values
where `huge / chosen_tc > u32::MAX` have no representable result. The
`ticktime` Conn unwraps under a documented precondition (`arb_tick`
caps at `u32::MAX × Grid::T1.tick_count()`); runtime callers
(transport, scheduler) call `from_ticks` directly and pick their own
out-of-range semantics.

The widening rippled to ~30 cast sites in `swing.rs` (i64 → i128
arithmetic on `tick + amount`), `sample_tick.rs` (u128 saturation
clamp at `u64::MAX`), `envelope.rs` (`linear_u8` / `smoothstep_u8`
widened to u64 with u128 Q-frac internals), and CLI test fixtures.

**2. Remove the `Ple` trait and `crates/core/src/preorder.rs`.**
Upstream `connections` removed `Ple` because the lawful framework
now consumes `Eq + PartialOrd` directly. agogo had kept it because
`Grid`'s divisibility preorder isn't the natural order on its
fields — but `PartialOrd for Grid` was already defined via `ple()`,
so `<=` already meant divisibility. For `Tick`, `U7`, `U4`, `Time`
the derived `PartialOrd` matched `Ple` 1:1.

The only conflict was `TBase`: derived `PartialOrd` ran in
declaration order (`T1 < T2 < … < T256`), opposite of its
divisibility `Ple`. Custom `Ord` / `PartialOrd` impls now mirror
divisibility (`a ≤ b ⟺ a.exp() ≥ b.exp()`), matching the shape Grid
uses. Audit confirmed no external caller relied on the old
declaration-order direction. The 75 `.ple(&x)` call-sites collapse
to `<= x`, six `impl Ple for …` blocks come out, and `preorder.rs`
is deleted.

**3. Rename four `time::conn` Conns to the 8-char convention.**
`ticks → ticktime`, `rat_tick → wholtick`, `time → timetime`,
`grid → gridgrid`. Pair-side Conns duplicate the side name since
the rule is silent on pairs. `quantize_at` keeps its name as a Conn
*constructor* (parametric family) — the module doc spells out the
exemption. Helpers (`*_ceil` / `*_inner` / `*_floor`) and ~50 test
names rename in lockstep. All sites confined to `conn.rs`.

## Test plan

- [x] `cargo test --workspace` — 945 lib tests + 17 + 17 + 1 doctest pass
- [x] `cargo clippy --all-targets -- -D warnings` — clean
- [x] `scripts/check-floats.sh` — no new f32/f64 storage
- [x] `time_to_tick_never_panics` proptest exercises the full
      `(beats: u32, base: Grid)` domain
- [x] `from_ticks_some_at_horizon` / `from_ticks_none_above_horizon`
      / `from_ticks_none_at_u64_max` pin the new partial behavior
- [x] `tbase_divisibility_chain_strictly_ascending` pins
      `T256 < T128 < … < T1`

## Local review (2026-04-28)

**Branch:** plan/2026-04-28-07
**Commits:** 4 (origin/main..plan/2026-04-28-07)
**Reviewer:** Claude (sonnet, independent)

---

### Commit Hygiene

Four implementation commits on top of the plan opener. All use accepted prefixes (`feat:`, `debt:`, `doc:`). The `debt:` commit (T2) and `debt:` commit (T3) are correctly separated per the plan's dependency graph. No merge commits in the log. Commit messages are under 72 characters and appropriate for what they contain. Clean.

---

### Code Quality

**Conn naming — TICKTIME/WHOLTICK constant names not present (confidence: 80)**

The 8-char Conn naming rule in CLAUDE.md says "Conn accessors are 8-char identifiers." The plan says rename to `ticktime / TICKTIME`, `wholtick / WHOLTICK`, `timetime / TIMETIME`, `gridgrid / GRIDGRID`. The diff adds `pub fn ticktime()`, `pub fn wholtick()`, etc. — the accessor functions are there. But the all-caps constant forms (`TICKTIME`, `WHOLTICK`, `TIMETIME`, `GRIDGRID`) do not appear anywhere in the diff. If the upstream `connections` library expects a constant singleton alongside the accessor function — matching the pattern `F032F016` / `pub const F032F016: Conn<...>` described in CLAUDE.md — those constants are missing. This is worth confirming against the upstream convention; if the singleton constants are the authoritative form in this codebase, their absence is a naming-convention violation.

**`from_ticks` doc comment still references the old `ticks` function name (confidence: 85)**

`crates/core/src/time/tick.rs` — in the new `from_ticks` doc comment:

```
/// the [`ticks`](crate::time::conn::ticks) Conn unwraps under a
/// documented precondition.
```

The old `ticks()` accessor no longer exists; it was renamed to `ticktime()`. The doc link will be a dead intra-doc link and will produce a warning or broken link in `cargo doc`. The correct reference is `crate::time::conn::ticktime`.

**`sample_tick_inner_saturates_on_overflow` no longer tests saturation (confidence: 82)**

`crates/core/src/sync/sample_tick.rs` — the generator was updated from `(u32::MAX / 2)..=u32::MAX` to `u64::from(u32::MAX / 2)..=u64::from(u32::MAX)`. Before the widening, `u32::MAX` was both the type ceiling of `Tick.0` and the clamp target in `to_tick`, so this range tested the saturation boundary. After widening, `to_tick` clamps at `u64::MAX`, which is ~4.3 billion times larger than `u32::MAX`. The test now feeds values far below the new saturation point. The test doc comment says "rule" (referring to the `to_tick` saturation behavior), but it no longer exercises it. The saturation can only be reached when `u128` sample accumulation overflows `u64::MAX`, which requires the old `u32::MAX`-level tick values multiplied by a large sample rate — that region is entirely unsampled by the new generator. As written the test will pass vacuously: `to_tick` just returns the tick unchanged (no clamping happens) for all inputs in range, so the saturation branch is never taken.

---

### Test Coverage

**`tbase_le_matches_old_ple` property missing (confidence: 90)**

The plan's Verification table (mandatory properties to ship) lists:

> `tbase_le_matches_old_ple` | `time::tbase` | for every pair `(a, b)`, new `a <= b` iff `a.exp() >= b.exp()`

No test with this name appears anywhere in the diff. `divisibility_chain_strictly_ascending` covers 8 adjacent pairs in the 9-element chain, but does not verify all 81 combinations. The plan explicitly chose an all-pairs check because the custom `Ord` impl's correctness cannot be verified by a chain test alone — e.g. a buggy impl that returned `a < b iff a.exp() > b.exp()` would pass the chain test but fail on equal-exp pairs. The plan's Review section does not document why it was omitted, so it cannot be treated as a deliberate deferral. CLAUDE.md says: "Properties that must hold for a sprint to ship are defined in the plan's Verification table." This one is absent.

**`arb_time_full_u32_domain` property is a manual inspection claim, not a test (low concern)**

The plan verification table says `arb_time_full_u32_domain` should assert `any::<u32>()` usage. The implementation does use `any::<u32>()` directly (confirmed in arb.rs). There is no automated test enforcing this — it's a human-inspection item as the plan acknowledges. This is fine given that `time_to_tick_never_panics` covers the consequence.

**`from_ticks_some_at_horizon` / `_none_above_horizon` / `_none_at_u64_max` — all present and meaningful.** The comment at `from_ticks_none_above_horizon` correctly traces through `nicest_from_tick_count`'s algorithm. The math for `effective_tick_saturates_at_u64_max` is correct: u64::MAX = (2^64 − 1) is divisible by 3 and by 5 (hence by 15 = T256's tick count), and the quotient 1229782938247303441 is odd.

**`swing_is_bar_periodic` — widening is correct, bug unchanged.** The removal of the `u32` overflow guard is safe since `t.0 ∈ [0, 100_000]` and `k_bar ≤ 38_400`, both trivially within u64. The bug being tracked (T1-resolution + negative amount off-by-one) is a logical error in `effective_tick`, not a width issue, so the saved seed b9e83f4f remains relevant.

**`ceil_fits` still discriminates** after the arb_tick cap change. For the `Just(horizon)` arm (tick = u32::MAX × 3840), `ceil_fits` with any grid g: `(u32::MAX × 3840).div_ceil(g.tick_count()) ≤ u32::MAX` only if `g.tick_count() ≥ 3840` — only true for T1. So the horizon arm is filtered by `prop_assume!` for all other grids. The assume is not degenerate.

---

### Plan Conformance

**T1 (Widen Tick to u64):** All specified changes present — `Tick(pub u64)`, panic-free `time_to_tick`, `Option<Time>` returns for `from_ticks` / `from_ticks_floor`, i128 widening in `swing.rs`, u128 saturation clamp in `sample_tick.rs`, `any::<u32>()` in `arb_time`, arb_tick capped at horizon. The plan's note about `time/float.rs` and `sample.rs` f64 precision paths is not addressed in the diff — these files are not changed. The plan flags this as "verify those paths don't see ticks that large" but doesn't mandate a code change, so this is acceptable.

**T2 (Remove Ple + preorder.rs):** All 6 impls removed, `preorder.rs` deleted, `pub mod preorder` removed, all ~75 `.ple(&x)` call-sites replaced with `<= x`, `TBase` has custom `Ord`/`PartialOrd`. TBase audit claim ("no external caller relied on declaration order") is covered by the `divisibility_chain_strictly_ascending` test plus the existing proptest suite.

**T3 (Rename 4 Conns):** Functions and test names correctly renamed. `quantize_at` exemption documented in module-level comment. The old names `ticks()`, `rat_tick()`, `time()`, `grid()` are absent from the diff as callsite targets.

---

### Risks

**`ticktime_ceil` / `ticktime_floor` panics are reachable from `quantize_at` callers (confidence: 75 — below threshold, but worth noting)**

`quantize_at`'s `$ceil` and `$floor` macros now contain their own `expect()` with the same precondition. Those are separately documented and have the same boundary as `ticktime`. Not a new risk.

**Dead intra-doc link will cause `cargo doc` warnings** (already flagged above).

**`swing_is_bar_periodic` saved seed**: The seed file `proptest-regressions/` entry records the bug. Since the test's generators are unchanged in domain (just u32→u64 types in a small range), the seed likely still triggers the same failure. Confirmed safe to carry forward as ignored.

---

### Recommendations

**Must fix before push:**

1. **Dead intra-doc link in `from_ticks` doc comment.** `crates/core/src/time/tick.rs`. Change `crate::time::conn::ticks` to `crate::time::conn::ticktime`. `cargo doc` will warn on this (or produce a broken link if the rustdoc intra-doc link resolver rejects unknown items).

2. **Add `tbase_le_matches_old_ple` all-pairs test.** `crates/core/src/time/tbase.rs`. The plan's Verification table mandates it. An exhaustive nested loop over `TBase::ALL × TBase::ALL` asserting `(a <= b) == (a.exp() >= b.exp())` is 9×9 pairs and takes microseconds. The plan's Review section doesn't document why it was omitted, so it cannot be treated as a deliberate deferral.

3. **Fix `sample_tick_inner_saturates_on_overflow` to test the actual saturation boundary.** `crates/core/src/sync/sample_tick.rs`. The generator `u64::from(u32::MAX / 2)..=u64::from(u32::MAX)` is ~2 billion below the new `u64::MAX` saturation point. Either: (a) rename the test to make clear it's testing normal arithmetic in the u32 legacy range and add a separate spot-check at the actual overflow boundary (a tick value where `tick.0 * pico_per_tick` in u128 would overflow u64), or (b) update the generator to sample near the new saturation point. The current test no longer exercises what its name says it exercises.

**Follow-up (future work):**

- The `TICKTIME` / `WHOLTICK` / `TIMETIME` / `GRIDGRID` all-caps singleton constants — if the upstream `connections` convention expects them alongside the accessor functions, they should be added in the next sprint that touches `conn.rs`. Confirm against the upstream library's CLAUDE.md.
- `BoundedTick` newtype to encode the Conn precondition at the type level (already deferred in the plan).
- The `from_ticks(...) == None` transport-wrap semantics have no exercising code path yet — when the first caller appears, add a decision and a test at that point.

<!-- gh-id: 3157909687 -->
### Copilot on [`crates/core/src/time/swing.rs:213`](https://github.com/cmk/agogo/pull/43#discussion_r3157909687) (2026-04-29 00:00 UTC)

The saturation-boundary comment above still mentions `arb_tick()` producing `Tick(u32::MAX)` and `effective_tick` clamping to `u32::MAX`, but after this PR `arb_tick()` is capped at the `from_ticks` horizon and `effective_tick` clamps to `u64::MAX`. Update that comment block so it reflects the current boundaries being exercised.

<!-- gh-id: 3157909704 -->
### Copilot on [`crates/core/src/time/tick.rs:72`](https://github.com/cmk/agogo/pull/43#discussion_r3157909704) (2026-04-29 00:00 UTC)

Doc comment still refers to the `ticks` Galois connection, but the Conn accessor was renamed to `ticktime` in this PR. Update the wording here (and any adjacent docs in this module that still say `ticks`) so it matches the new API name.

<!-- gh-id: 3157909717 -->
### Copilot on [`crates/core/src/time/conn.rs:24`](https://github.com/cmk/agogo/pull/43#discussion_r3157909717) (2026-04-29 00:00 UTC)

The module docs say "`Conn::new` isn't `const fn` upstream", but this repo already uses `Conn::new(...)` in `pub const` Conn definitions elsewhere (e.g. `crates/core/src/time/decimal.rs`). This comment looks stale/misleading—either update it to the current reality or explain the real reason these Conns are exposed via accessors instead of constants.


<!-- gh-id: 3157909731 -->
### Copilot on [`doc/plans/plan-2026-04-28-07.md:159`](https://github.com/cmk/agogo/pull/43#discussion_r3157909731) (2026-04-29 00:00 UTC)

This plan’s Conn rename mapping includes `ticktime / TICKTIME`, `wholtick / WHOLTICK`, etc., but the implementation in `crates/core/src/time/conn.rs` only introduces the accessor functions (no all-caps singleton constants). Either update the plan text to reflect the implemented API surface, or add the missing constants if they’re required by the convention.


<!-- gh-id: 3157909742 -->
### Copilot on [`crates/core/src/time/conn.rs:658`](https://github.com/cmk/agogo/pull/43#discussion_r3157909742) (2026-04-29 00:00 UTC)

The comment block above this `quantize_at_*` proptest still references the removed `.ple` method and claims `arb_tick()` includes `Tick(u32::MAX)`. After this PR, `Ple` is gone and `arb_tick()` is capped at the `from_ticks` horizon, so that rationale is stale—update the comment to match the current generator + comparison semantics.

<!-- gh-id: 3157909755 -->
### Copilot on [`crates/core/src/time/conn.rs:725`](https://github.com/cmk/agogo/pull/43#discussion_r3157909755) (2026-04-29 00:00 UTC)

This section header/comment still talks about lattice-connection laws for `time`/`grid` and refers to a `Ple` impl, but the Conns are now `timetime`/`gridgrid` and `Ple` no longer exists. Update the wording to reference the new Conn names and the current `PartialOrd`/divisibility relation.

<!-- gh-id: 3157909770 -->
### Copilot on [`crates/core/src/arb.rs:186`](https://github.com/cmk/agogo/pull/43#discussion_r3157909770) (2026-04-29 00:00 UTC)

The `arb_tick` docs say it avoids values the `ticks` Conn can’t canonicalise, but the Conn accessor was renamed to `ticktime` in this PR. Update the comment to the new Conn name.


<!-- gh-id: 3157909783 -->
### Copilot on [`crates/core/src/time/grid.rs:555`](https://github.com/cmk/agogo/pull/43#discussion_r3157909783) (2026-04-29 00:00 UTC)

nit: These property tests are still named `ple_*`, but the implementation under test is now `<=` via `PartialOrd` (the `Ple` trait was removed). Consider renaming the tests to avoid referring to a removed abstraction.

<!-- gh-id: 3157909801 -->
### Copilot on [`crates/core/src/time/conn.rs:529`](https://github.com/cmk/agogo/pull/43#discussion_r3157909801) (2026-04-29 00:00 UTC)

Comment heading still says `// ── ticks ──` even though the connection and tests in this block are `ticktime_*`. Rename the heading to match the new Conn name to avoid confusion while navigating this file.


<!-- gh-id: 3157909812 -->
### Copilot on [`crates/core/src/time/tbase.rs:306`](https://github.com/cmk/agogo/pull/43#discussion_r3157909812) (2026-04-29 00:00 UTC)

nit: These proptest names still use the `ple_*` prefix even though the `Ple` trait was removed and the tests now exercise `<=`/`Ord`. Renaming them (e.g. `le_reflexive`, `order_transitive`) would better match the post-T2 terminology.

<!-- gh-id: 3157909823 -->
### Copilot on [`crates/core/src/midi.rs:174`](https://github.com/cmk/agogo/pull/43#discussion_r3157909823) (2026-04-29 00:00 UTC)

nit: This test name still refers to `ple`, but `Ple` was removed and the assertions now use `<=`/`>`. Renaming the test would keep terminology consistent.


<!-- gh-id: 4193297801 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-04-29 00:01 UTC](https://github.com/cmk/agogo/pull/43#pullrequestreview-4193297801))

## Pull request overview

This PR performs three related cleanups in `crates/core/`: widening the master `Tick` counter to `u64` to eliminate overflow risk, removing the local `Ple` preorder trait in favor of `PartialOrd`, and renaming `time::conn` Conns to match the project’s 8-character naming convention.

**Changes:**
- Widen `Tick(pub u32)` → `Tick(pub u64)`, make `from_ticks*` partial (`Option<Time>`), and update affected arithmetic/casts across time + scheduling paths.
- Remove `Ple` + `preorder.rs`, replace call sites with `<=`, and implement divisibility-based `Ord/PartialOrd` for `TBase`.
- Rename `time::conn` accessors (`ticks/rat_tick/time/grid` → `ticktime/wholtick/timetime/gridgrid`) and update call sites/tests.

### Reviewed changes

Copilot reviewed 17 out of 17 changed files in this pull request and generated 13 comments.

<details>
<summary>Show a summary per file</summary>

| File | Description |
| ---- | ----------- |
| doc/reviews/review-00042.md | Adds a review record for PR #42 (currently contains some stale assertions vs the final diff). |
| doc/plans/plan-2026-04-28-07.md | Adds the implementation plan + verification checklist for the sprint. |
| crates/core/src/time/tick.rs | Widens `Tick`, removes overflow panic path, and makes `from_ticks*` return `Option<Time>`. |
| crates/core/src/time/tbase.rs | Drops `Ple`, adds divisibility-based `Ord/PartialOrd`, and adds an all-pairs regression test. |
| crates/core/src/time/swing.rs | Updates tick arithmetic to `u64` + `i128` widening and refreshes saturation tests. |
| crates/core/src/time/grid.rs | Removes `Ple`, implements divisibility via `PartialOrd`, and updates lattice-law tests. |
| crates/core/src/time/envelope.rs | Widens ramp/smoothstep helpers to `u64` and uses `u128` intermediates. |
| crates/core/src/time/conn.rs | Renames Conns, adapts to partial `from_ticks`, and updates adjoint-law tests to use `<=`. |
| crates/core/src/sync/sample_tick.rs | Updates Tick saturation/clamps to `u64::MAX` and extends overflow boundary testing. |
| crates/core/src/preorder.rs | Deletes the local `Ple` trait module. |
| crates/core/src/midi.rs | Removes `Ple` impls for `U7/U4` and switches tests to `<=`. |
| crates/core/src/lib.rs | Removes `pub mod preorder;` export. |
| crates/core/src/channel/transform.rs | Updates tick/divisor arithmetic to `u64` and adjusts tests accordingly. |
| crates/core/src/channel/scheduler.rs | Widens tick window math to `i128`/`u64` to avoid overflow after Tick widening. |
| crates/core/src/arb.rs | Updates `arb_tick`/`arb_time` strategies for widened Tick + full `u32` beats domain. |
| crates/cli/src/time_sched.rs | Updates CLI scheduling math/tests for `Tick(u64)`. |
| crates/cli/src/channel_trace.rs | Widens trace row tick field to `u64`. |
</details>






<!-- gh-id: 3157909833 -->
### Copilot on [`crates/core/src/time/conn.rs:413`](https://github.com/cmk/agogo/pull/43#discussion_r3157909833) (2026-04-29 00:01 UTC)

Several spot-check tests are still named `rat_tick_*` even though the Conn accessor is now `wholtick()`. This conflicts with the PR description/plan (“test names rename in lockstep”) and makes grepping for the new Conn name harder; consider renaming these tests (and the corresponding proptests below) to `wholtick_*`.

<!-- gh-id: 3157909848 -->
### Copilot on [`doc/plans/plan-2026-04-28-07.md:190`](https://github.com/cmk/agogo/pull/43#discussion_r3157909848) (2026-04-29 00:01 UTC)

Verification table uses property names that don’t match the tests added in this PR (e.g. `from_ticks_some_on_horizon` vs `from_ticks_some_at_horizon`, and `tbase_divisibility_total_order` vs `divisibility_chain_strictly_ascending`). Update the table so it points at the actual test names, otherwise it’s easy to think required properties are missing.


<!-- gh-id: 3157954202 -->
#### ↳ cmk ([2026-04-29 00:17 UTC](https://github.com/cmk/agogo/pull/43#discussion_r3157954202))

Fixed — refreshed the saturation-spot-check comment to describe `arb_tick`'s `u32::MAX × Grid::T1.tick_count()` cap and the `u64::MAX` clamp in `effective_tick`. Both `u32::MAX` references are gone.

<!-- gh-id: 3157954351 -->
#### ↳ cmk ([2026-04-29 00:17 UTC](https://github.com/cmk/agogo/pull/43#discussion_r3157954351))

Fixed — module doc now refers to the `ticktime` Galois connection.

<!-- gh-id: 3157954500 -->
#### ↳ cmk ([2026-04-29 00:17 UTC](https://github.com/cmk/agogo/pull/43#discussion_r3157954500))

Good catch. Verified `Conn::new` is `const fn` upstream (`connections/src/conn.rs:227`), so the four single-type-side Conns are now `pub const` constants matching the upstream pattern (`F032F016`, `F064FD12`, etc.):

```rust
pub const TICKTIME: Conn<Tick, Time> = Conn::new(...);
pub const WHOLTICK: Conn<Whole, Tick> = Conn::new(...);
pub const TIMETIME: Conn<(Time, Time), Time> = Conn::new(...);
pub const GRIDGRID: Conn<(Grid, Grid), Grid> = Conn::new(...);
```

Module doc updated. `quantize_at` stays a function — its inner/ceil/floor pointers vary per `Grid` value.

<!-- gh-id: 3157954627 -->
#### ↳ cmk ([2026-04-29 00:17 UTC](https://github.com/cmk/agogo/pull/43#discussion_r3157954627))

Fixed — promoted the four single-type-side accessor functions to `pub const TICKTIME` / `WHOLTICK` / `TIMETIME` / `GRIDGRID`. Plan text and implementation now agree.

<!-- gh-id: 3157955119 -->
#### ↳ cmk ([2026-04-29 00:17 UTC](https://github.com/cmk/agogo/pull/43#discussion_r3157955119))

Fixed — rewrote the comment block. It now describes `arb_tick`'s horizon cap (`u32::MAX × Grid::T1.tick_count()`), the `ceil_fits` filter that skips finer-grid overflow, and the `<=` semantics that replaced `.ple`. No more references to `.ple` or `u32::MAX`.

<!-- gh-id: 3157955330 -->
#### ↳ cmk ([2026-04-29 00:17 UTC](https://github.com/cmk/agogo/pull/43#discussion_r3157955330))

Fixed — heading now reads `// ── Lattice-connection laws for \`TIMETIME\` and \`GRIDGRID\` ──`, and the body sentence cites "the standard divisibility \`PartialOrd\` for \`Grid\`" instead of the removed \`Ple\` impl.

<!-- gh-id: 3157955470 -->
#### ↳ cmk ([2026-04-29 00:18 UTC](https://github.com/cmk/agogo/pull/43#discussion_r3157955470))

Fixed — `arb_tick` doc now intra-doc-links `[`TICKTIME`](crate::time::conn::TICKTIME)`.

<!-- gh-id: 3157955598 -->
#### ↳ cmk ([2026-04-29 00:18 UTC](https://github.com/cmk/agogo/pull/43#discussion_r3157955598))

Fixed — renamed `ple_reflexive` / `ple_antisymmetric` / `ple_transitive` to `le_*`.

<!-- gh-id: 3157955868 -->
#### ↳ cmk ([2026-04-29 00:18 UTC](https://github.com/cmk/agogo/pull/43#discussion_r3157955868))

Fixed — section heading is now `// ── ticktime ──`.

<!-- gh-id: 3157956010 -->
#### ↳ cmk ([2026-04-29 00:18 UTC](https://github.com/cmk/agogo/pull/43#discussion_r3157956010))

Fixed — renamed `ple_reflexive` / `ple_antisymmetric` / `ple_transitive` / `ple_total` to `le_*` in this proptest block.

<!-- gh-id: 3157956185 -->
#### ↳ cmk ([2026-04-29 00:18 UTC](https://github.com/cmk/agogo/pull/43#discussion_r3157956185))

Fixed — renamed to `le_compares_inner`.

<!-- gh-id: 3157956398 -->
#### ↳ cmk ([2026-04-29 00:18 UTC](https://github.com/cmk/agogo/pull/43#discussion_r3157956398))

Fixed — renamed all `rat_tick_*` spot checks and proptests to `wholtick_*`. Plan-conformant now.

<!-- gh-id: 3157956634 -->
#### ↳ cmk ([2026-04-29 00:18 UTC](https://github.com/cmk/agogo/pull/43#discussion_r3157956634))

Fixed — Verification table now lists the actual property names (`from_ticks_some_at_horizon`, `from_ticks_none_above_horizon`, `from_ticks_none_at_u64_max`, `divisibility_chain_strictly_ascending`, `tbase_le_matches_old_ple`).
