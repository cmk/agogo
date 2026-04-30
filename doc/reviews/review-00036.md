# PR #36 — SampleTickConn wrap fix + micro_from_ms rename

## Summary

Two narrow follow-ups from PR #35's review backlog, shipped as one
small post-merge cleanup. No new features; no public API/signature
changes besides the `micro_from_ms` rename. `SampleTickConn::inner`
gains a behaviour change on overflow inputs — it now saturates to
`u64::MAX` instead of wrapping modulo 2⁶⁴ (the wrap was a silent
bug, not contracted behaviour).

### What's in this PR

**T1 — `SampleTickConn::inner` saturates the u128→u64 narrow.** The
Copilot review on PR #35 flagged a silent wrap: the u128 quotient was
cast to u64 unchecked, so pathological inputs (`Tick(u32::MAX)` with
`bpm_µ = 1`, `ppqn = 1`) overflowed modulo 2⁶⁴. PR #35 deferred the
fix because the function had been moved verbatim from `time/conn.rs`
and the reorg discipline was "no logic changes during structural
moves." This PR applies the same `min(u64::MAX) as u64` clamp the
inverse direction (`to_tick`) already uses.

A new proptest `sample_tick_inner_saturates_on_overflow` walks the
deliberately-pathological generator domain (`tick` near `u32::MAX`,
`bpm_µ` ∈ `1..=100`, `ppqn` ∈ `1..=8`, `sr = 192_000`) and compares
against an independent u128 reference. **Test-the-test verified**:
reverting the saturation in a dirty worktree fails the proptest at
minimal seed `tick = 2_147_483_647, bpm_u = 1, ppqn = 1`. The
auto-saved regression seed at
`crates/core/proptest-regressions/sync/sample_tick.txt` rides along.

The existing realistic-input proptests (`sample_tick_round_trip`,
`_monotonic`, `_ceil_ge_floor`) use `arb_integer_stc()` which only
emits realistic combos that never hit the wrap region. They continue
to pass unchanged.

**T3 — Rename `micro_from_ms` → `micro_from_user_ms`.** PR #35
sprint-review audit flagged the name as too generic. The function
specifically takes a user-typed decimal in milliseconds at the argv
boundary (`delay=` mini-language value) and converts via
`F064FD06.ceil`. Not a general-purpose helper, not Conn-composable.
The new name signals argv-only semantics at every call site.
Visibility unchanged (`fn`, private to `machine::spec`); three
in-file references updated.

### What's NOT in this PR

The plan flagged two more T12 stragglers from PR #35's audit; both
were verified as no-ops:

- **`MidiRtByte::Continue` `#[allow(dead_code)]`.** The audit
  thought there was a suppression on the variant. There isn't — the
  `#[allow(dead_code)]` at `out/midi.rs:537` is on a different
  test-helper (`_channel_type_is_used`). `MidiRtByte::Continue` is
  exercised by tests at lines 361/394/410 and is legitimate
  MIDI-spec coverage (0xFB).
- **`channel.rs` re-export hub (`CvRole` / `DinRole`).** The audit
  asked whether these are forward-compat scaffolding. They aren't —
  both are active production variants of `Channel::Cv` /
  `Channel::Din` (`channel/transform.rs:48,52`).

### Verification

| Check | Result |
|---|---|
| `cargo build --workspace --all-features` | green |
| `cargo test --workspace --all-features` | 940 + 39 + 39 + 1 = 1019, 2 ignored, 0 failed (was 1018; +1 for the new wrap proptest) |
| `cargo test -p agogo-host-link --features rusty-link` | 31 + 4 = 35, 0 failed |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | clean |
| `scripts/check-floats.sh` | OK (no allowlist changes) |
| Test-the-test for T1 | pass — proptest fails on the wrap when saturation is reverted |

### What's deferred

The PR #35 backlog continues unchanged minus the two items closed
here:

- T6 `LpfPid` (clocked-style controller wrapper for v0.5 Link follower)
- T7 `TransportState<S>` typestate skeleton
- T8 `RelativeClock` calibration helper
- T9 `machine/spec.rs` 1299-line split
- T11 `cli/main.rs` 1727-line extraction
- `compose!` / `ceiling1` body cleanups
- host-link 4-layer wrapping cleanup

## Local review (2026-04-28)

**Branch:** plan/2026-04-28-04
**Commits:** 4 (origin/main..plan/2026-04-28-04)
**Reviewer:** Claude (sonnet, independent)

---

### Commit Hygiene

All four commits use valid prefixes (`plan:`, `fix:`, `debt:`,
`doc:`). Order correct (plan first, then implementation, then
docs). Each commit narrow enough to be independently buildable.

### Code Quality

**Saturation fix (`sync/sample_tick.rs:71–72`):** The
`q.min(u128::from(u64::MAX)) as u64` clamp mirrors `to_tick`'s
pattern exactly. Doc comment updated to mention saturation and
cite PR #35.

**Generator domain rule:** the new proptest bounds inputs to the
wrap region. Per CLAUDE.md the anti-pattern is bounding to *avoid*
boundaries; here the bounds are set to *reach* the overflow
region — the opposite. The plan's review section documents this
explicitly. The realistic-input region is covered by existing
proptests via `arb_integer_stc()`. Acceptable exception.

**Minor:** the plan pseudocode showed `sr` as a generator
parameter; the committed code hardcodes `sr = 192_000` inside
the test body. Effect identical (`sr` is always 192_000); the
hardcoded form is cleaner. No correctness issue.

**T3 rename:** zero remaining `micro_from_ms` hits across all
crates. All four occurrences updated (def + 1 call + 2 doc
references). Visibility unchanged.

### Test Coverage

**Test-the-test:** the regression seed at
`crates/core/proptest-regressions/sync/sample_tick.txt`
contains the exact minimal failing case `tick = 2147483647`
that would produce ~2.47×10²⁵ as the numerator at sr=192_000.
proptest only saves seeds for cases that actually failed —
corroborates the plan's claim that the test fails without the
clamp.

**Independent reference correctness:** the test computes the
expected value via standalone u128 math (not by calling
`inner`), so `prop_assert_eq!(result, expected)` is a real
check, not tautological.

**Existing proptests:** `sample_tick_round_trip`,
`_monotonic`, `_ceil_ge_floor` all use `arb_integer_stc()` with
realistic tempos (minimum 60 BPM = 60_000_000 µBPM); none
reaches the wrap region. Unmodified, will continue to pass.

### Plan Conformance

T1 + T3 implemented as specified. The two dropped items (T2
`MidiRtByte::Continue`, T4 `channel.rs` re-exports) are
documented in the plan's Goal section with specific evidence
for why they are no-ops.

### Risks

**Saturation behaviour change:** any caller that treats wrapped
garbage as meaningful was already broken; saturation is the
correct recovery. No realistic musical input comes near the
wrap boundary.

**Rename:** `micro_from_user_ms` is `fn` (private). No external
caller possible. Zero residual hits confirmed.

### Recommendations

**Must fix before push:** None.

**Follow-up:** None.

<!-- gh-id: 4192236408 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-04-28 20:40 UTC](https://github.com/cmk/agogo/pull/36#pullrequestreview-4192236408))

## Pull request overview

Post-PR #35 cleanup that fixes an integer overflow wrap in `SampleTickConn::inner` by saturating the u128→u64 narrowing, and renames an argv-boundary helper to better reflect its semantics.

**Changes:**
- Clamp `SampleTickConn::inner`’s u128 quotient to `u64::MAX` instead of allowing modulo wrap on overflow.
- Add a targeted proptest (plus regression seed) that exercises the overflow region and validates the saturation behavior.
- Rename `micro_from_ms` → `micro_from_user_ms` within `machine::spec` to make argv-only semantics explicit.

### Reviewed changes

Copilot reviewed 5 out of 5 changed files in this pull request and generated 2 comments.

<details>
<summary>Show a summary per file</summary>

| File | Description |
| ---- | ----------- |
| doc/reviews/review-00036.md | Adds the review record for PR #36 and documents the rationale/verification. |
| doc/plans/plan-2026-04-28-04.md | Adds the plan describing the saturation fix + rename, with verification notes. |
| crates/core/src/time/conn.rs | Implements u128→u64 saturation in `inner` and adds a new overflow-focused proptest. |
| crates/core/src/channel/spec.rs | Renames `micro_from_ms` to `micro_from_user_ms` and updates call sites/comments. |
| crates/core/proptest-regressions/sync/sample_tick.txt | Checks in the proptest regression seed for the overflow case. |
</details>






<!-- gh-id: 3157044396 -->
### Copilot on [`doc/reviews/review-00036.md:7`](https://github.com/cmk/agogo/pull/36#discussion_r3157044396) (2026-04-28 20:40 UTC)

nit: The summary says “No … API changes outside the rename”, but this PR also changes `SampleTickConn::inner` behavior for overflow inputs (it now saturates instead of wrapping). Consider rephrasing to “no public API/signature changes (besides the rename); `inner` now saturates on overflow” to avoid a misleading top-level claim.
```suggestion
small post-merge cleanup. No new features; no public API/signature
changes besides the rename; `SampleTickConn::inner` now saturates on
overflow.
```

<!-- gh-id: 3157044455 -->
### Copilot on [`crates/core/src/time/conn.rs:187`](https://github.com/cmk/agogo/pull/36#discussion_r3157044455) (2026-04-28 20:40 UTC)

The proptest doc comment says the generator domain “spans the full `u32`/`u32` / `192_000` regions”, but the strategy is intentionally bounded (`tick` in `u32::MAX/2..=u32::MAX`, `bpm_u` in `1..=100`, `ppqn` in `1..=8`). Please reword to reflect that it *targets* the overflow region rather than covering the full domain.
```suggestion
        /// garbage. This generator targets that overflow-prone
        /// region by using large `tick` values near `u32::MAX`,
        /// small `bpm_u`/`ppqn`, and `sr = 192_000`; the
```

<!-- gh-id: 3157100360 -->
#### ↳ cmk ([2026-04-28 20:50 UTC](https://github.com/cmk/agogo/pull/36#discussion_r3157100360))

Done — rephrased the summary to call out the saturation behaviour change explicitly. The wrap-on-overflow was a silent bug, not contracted behaviour, so I framed it that way.

<!-- gh-id: 3157100591 -->
#### ↳ cmk ([2026-04-28 20:50 UTC](https://github.com/cmk/agogo/pull/36#discussion_r3157100591))

Done — reworded to clarify the generator is *targeting* the overflow region (with the realistic-input region covered separately by `arb_integer_stc()`-driven proptests). Also linked it explicitly to the CLAUDE.md anti-pattern: bounding to *avoid* boundaries is the bug, bounding to *reach* a specific failure region is fine.
