# PR #42 — Three-mess cleanup: widen Tick, drop Ple, rename Conns

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
