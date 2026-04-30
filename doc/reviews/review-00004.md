# PR #4 — `channel/` + `SampleTickConn`

## Summary

Delivers the Plan 03 per-channel scheduler: pure-logic divider /
shuffle / shift / offset transforms over master tick streams, plus
the §7 `SampleTickConn` shim bridging Tick ↔ Sample in the presence
of `connections::Conn`'s fn-pointer constraint.

### What's new

- **`time::conn::SampleTickConn`** — runtime-parameterised
  `(ceil, inner, floor)` triple mirroring `Conn<Sample, Tick>`. Cannot
  be a genuine `Conn` because its conversion closes over `(sr, bpm)`.
  Laws verified by proptest (`sample_tick_round_trip`,
  `sample_tick_monotonic`, `sample_tick_ceil_ge_floor`).
- **`channel/mode.rs`** — `ChannelMode` enum with the full v0.1 spec
  surface (`MidiClock`, `Din`, `AnalogPulse`, `AnalogLfo`, `MidiCc`);
  only `MidiClock` is rendered in this sprint.
- **`channel/transform.rs`** — `Channel` struct, `ScheduledEvent`,
  and the pure divide → shuffle → Tick-to-Sample → shift → offset
  pipeline. Shift saturates at `[0, 300]` ms; negative shift remains
  deferred to v0.2.
- **`channel/scheduler.rs`** — `tick_stream(channel, stc,
  buffer_start, frames)` returns events whose `sample_index` lands in
  the half-open buffer window. Consecutive buffer calls covering a
  contiguous sample range reproduce the single-call result exactly
  (no dupes, no gaps).
- **CLI** — `agogo channel trace --bpm … --sr … --divider … --frames
  … --buffers …` emits CSV (`buffer_index,sample_index,tick`). The
  plan's build gate (120 BPM / 48 kHz / T4 / 4096-frame buffers →
  events at samples 0, 24 000, 48 000) is asserted as a unit test.
- **PR #1 drive-bys** — "uncuumed" typo in `sync/pll.rs` test comment
  and the `pll_phase_converges` verification-table footnote in
  `plan-2026-04-22-02.md`.
- **Plan-03-emergent PLL fix** — proptest surfaced an `f64 → f32`
  rounding edge where `PllOutput.phase` could reach exactly `1.0`
  after ~1000 free-run steps at certain BPMs, violating the `[0, 1)`
  invariant. Fixed to wrap to `0.0`; the discovering proptest seed
  is committed alongside.

### Verification

All plan-specified properties green:

| Property | Module |
|----------|--------|
| `sample_tick_round_trip` | `time::conn` |
| `sample_tick_monotonic`  | `time::conn` |
| `tick_monotonicity`      | `channel::transform` |
| `divider_rate_preservation` | `channel::transform` |
| `shift_upper_clamp` / `shift_lower_clamp` | `channel::transform` |
| `shuffle_identity_on_even_steps` (replaces plan's `shuffle_zero_mean_per_beat` — see Deviations) | `channel::transform` |
| `scheduler_events_in_window` | `channel::scheduler` |
| `scheduler_block_equivalence` (combines `no_dupes` + `no_gaps`) | `channel::scheduler` |

No new `#[ignore]`s introduced by this sprint.

### Deviations from the plan

Full list in `doc/plans/plan-2026-04-23-01.md` §Review. Highlights:

1. `shuffle_zero_mean_per_beat` can't hold under Haskell's one-sided
   swing (documented in Plan 02); replaced with
   `shuffle_identity_on_even_steps`.
2. `divider_rate_preservation` uses `div_ceil` so dividers coarser
   than a beat (T1, T2) work.
3. `tick_stream` omits `&mut PhaseSource` — unused in v0.1; add back
   in Plan 04 if the MIDI-clock emitter needs live phase.
4. `MidiCc { cc, range }` uses `u8` (no `u7` newtype in core).
5. `tick_monotonicity` property is tested only under
   `|swing displacement| < divider.tick_count()` — the
   musically-meaningful regime. Outside that bound, one-sided swing
   can reorder events; documented in plan §Review.
6. Plan's `SampleTickConn::inner` rationale overstates `u128`'s
   guarantee — the implementation casts to `f64` before dividing,
   so precision above 2⁵³ is lost. Exact in the exercised range;
   the planned repo-wide float → fixed-precision refactor will
   make it exact at every scale.

### Out of scope / follow-ups for Plan 04

- **MidiClock cadence vs `Channel.divider`.** MidiClock bytes fire
  at PPQN/24, but `divider` is a musical `TBase`. Reconcile before
  writing the emitter (either add `TBase::Tmidi24` or walk the
  master stream directly).
- **Negative `offset_ms` near a buffer's start silently drops events.**
  An event whose natural sample falls in
  `[buffer_start, buffer_start + |offset_samples|)` saturates below
  `buffer_start` after the offset and is filtered out. It does not
  reappear in the previous buffer (that window has passed). At most
  240 samples dropped at 48 kHz with the current ±5 ms calibration
  range. Plan 04's audio callback should decide whether to reclaim
  via a previous-buffer retry.
- Negative shift buffer budget (§10 open question) blocks the audio
  callback; spec before Plan 06.

## Local review (2026-04-23)

**Branch:** plan/2026-04-23-01
**Commits:** 9 (origin/main..plan/2026-04-23-01)
**Reviewer:** Claude (sonnet, independent)

---

### Commit Hygiene

All 9 commit subjects are conventional (present-tense imperative, accepted
prefix, scope where appropriate) and under 72 characters. The progression
is logical: plan opener → scaffolding → features → drive-bys → test seeds
→ doc finalization.

The PLL fix `e1e58dc fix(sync): wrap pll phase to 0 when f32 cast reaches
1.0` is unrelated to Plan 03's main goal. Bundling is explicitly permitted
by the "Bundle PR nits with next plan" memory. The fix is small,
self-contained, and does not touch Plan 03's new modules. No concern.

`d21b430 fix: carry-over drive-bys from PR #1` is missing a scope. The
commit touches two files in different subsystems — a scope would help
blame readability. Not a blocker.

### Code Quality

**`#![forbid(unsafe_code)]`**: Present at `crates/core/src/lib.rs`
(unchanged) and covers all new submodules. Correct.

**Module layout**: `channel.rs` + subdirectory `channel/` holding
`mode.rs`, `transform.rs`, `scheduler.rs`. No `mod.rs`. Correct modern
layout.

**`SampleTickConn::inner` — precision vs. plan claim**: The plan (T1)
states "Uses `u128` intermediate arithmetic to avoid overflow on
multi-day sample streams." The implementation computes `num` as `u128`
but immediately casts to `f64` before dividing:

```rust
let num = u128::from(tick.0) * u128::from(self.sr) * 60;
let denom = self.bpm * f64::from(self.ppqn);
((num as f64) / denom).round().max(0.0) as u64
```

`f64` has a 53-bit mantissa. The `u128` prevents the multiplication from
wrapping, but the `as f64` cast then loses precision above 2⁵³.
For the app's current operational range (tick ≤ 10⁶) this is harmless,
but the plan's stated rationale ("avoid overflow") only half-applies —
exactness is bounded by `f64` precision, not `u128`. Documentation
inconsistency, not a bug in the exercised range. Follow-up.

**Saturating arithmetic in `transform`**: `saturating_add`/`saturating_sub`
on `u64` sample indices caps at `u64::MAX` (~9×10¹⁸ samples ≈ 6×10¹²
seconds at 48 kHz). Saturation is unreachable in production. Acceptable.

**`buffer_start_sample as i64` truncation** (`scheduler.rs:47`): Values
above `i64::MAX` wrap silently. Beyond any plausible audio session (~6×10¹²
seconds at 48 kHz). Not a bug; acceptable.

**Coupling**: `channel/` depends only on `time::conn::SampleTickConn`,
`time::swing`, `time::tbase`, `time::tick`. No `sync` dependency. Clean.

**Dead code / Clippy**: No obvious issues. `ChannelMode` non-MidiClock
variants are documented stubs.

### Test Coverage

**Property tests — completeness**: All nine plan-listed properties have
corresponding tests. The two substitutions (`shuffle_identity_on_even_steps`,
`scheduler_block_equivalence`) are each documented in the plan's Review
section and the review file summary.

**`shuffle_zero_mean_per_beat` → `shuffle_identity_on_even_steps`**: The
substitution is defensible. The plan's §Review correctly explains that
one-sided swing cannot satisfy a zero-mean constraint (the pre-existing
`swing_zero_mean_over_beat` in `time/swing.rs` is already `#[ignore]`d).
The replacement tests that even-parity T16 steps are identity under
swing — the correct mechanistic statement of "only off-beats move."
Spirit preserved.

**`scheduler_no_dupes` + `scheduler_no_gaps` → `scheduler_block_equivalence`**:
The combined property asserts that splitting `[0, total)` into `n`
consecutive buffers and calling `tick_stream` on each produces exactly
the same multiset of events as one call on the full range. This
logically covers both: duplicates would increase cardinality, gaps would
decrease it. Correct.

**`tick_monotonicity` strategy restriction is undocumented in the Review
section** (`channel/transform.rs:765-780`, `doc/plans/plan-2026-04-23-01.md`
§Review). The plan lists `tick_monotonicity` as load-bearing
("per-channel Tick stream is non-decreasing") without qualification.
The implementation uses `arb_divider_with_bounded_swing`, which caps
`|amount| < tick_count()` — so the test delivers a weaker form:
non-decreasing *only within the bounded-swing regime*. This scope
restriction is documented only in the test comment, not in the plan's
Review section. CLAUDE.md requires in-scope invariant restrictions to
appear in the Review section. **Must-fix.**

**`divider_rate_preservation` — `div_ceil` generalisation**: Correct
across `TBase`. For T8 span=192: 192/96 = 2 events per beat. For T1
span=192: `div_ceil(192, 768) = 1` — Tick(0) fires once. Documented in
Review section as deviation 2.

**CLI test `channel_trace_t4_120bpm_matches_expected_samples`**: 16 ×
4096-frame run at 120 BPM / 48 kHz / T4 emits samples `[0, 24_000,
48_000]` (72_000 exceeds the 65_536-sample total). Correct regression
of the plan's E2E build gate.

**`scheduler_events_in_window` — negative `offset_ms` edge case**: With
`offset_ms = -5.0` (240 samples at 48 kHz), an event whose natural
sample falls in `[buffer_start, buffer_start + 240)` saturates below
`buffer_start` after offset and gets filtered out by the window check.
It does not reappear in the previous buffer (that window has passed).
The property still passes — filtered events don't violate "all emitted
events are in-window" — but the scenario demonstrates events can be
silently dropped. Not a correctness violation given current semantics,
but worth flagging for Plan 04/05.

### Plan Conformance

- T0–T5 all implemented. ✓
- All Verification-table properties implemented (two with documented
  substitutions).
- Drive-by fixes from PR #1 landed (`d21b430`).
- PLL phase-wrap fix (`e1e58dc`) is not in the plan. The fix closes a
  real correctness hole discovered during this sprint (regression seed
  committed). The review file attributes it under "PR #1 drive-bys" in
  the summary, but it's really a Plan 03 emergent discovery — worth
  clarifying. Follow-up.
- Review file (`doc/reviews/review-00004.md`): present, has `## Summary`,
  documents the five deviations. Format correct.

### Risks

- **PLL phase-wrap behavioural change**: Callers relying on `PllOutput.phase
  == 1.0` (which would have violated the `[0, 1)` invariant) now receive
  `0.0`. This is the correct fix; `1.0` was always a bug. Regression
  seed captures the case.
- **Panic paths from CLI input**: `SampleTickConn::new` panics on invalid
  inputs. The CLI validates `bpm > 0` (via `parse_positive_f64`), `sr >=
  1` (via clap range). `ppqn` is hardcoded to 192. No reachable panic
  from user input.
- **No new runtime dependencies**: Confirmed.

### Recommendations

**Must fix before push:**

1. **Document the `tick_monotonicity` strategy restriction in the plan's
   Review section.** (`doc/plans/plan-2026-04-23-01.md` §Review.)
   The test is scoped to `|swing displacement| < divider.tick_count()`
   (via `arb_divider_with_bounded_swing`). Outside that bound, one-sided
   swing can push an off-beat past the preceding on-beat, violating
   monotonicity. CLAUDE.md §"Property-based testing is mandatory"
   requires scope narrowings to be documented in the Review section.
   Add a deviation entry stating the regime tested and what would be
   needed to handle the unbounded case (e.g., a weaker "non-decreasing
   after sort" invariant, or redefining the property's domain).

**Follow-up (future work):**

2. **`SampleTickConn::inner` precision vs. plan claim.** Either revise
   the plan note ("prevents multiplication overflow; division uses
   f64 and loses precision above 2⁵³") or switch to fixed-point
   integer arithmetic if long-session exact precision becomes a
   requirement.
3. **`d21b430 fix:` missing scope.** Add `fix(sync):` or split the
   doc change into a separate `doc:` commit to improve blame.
4. **PLL phase-wrap fix attribution.** Clarify in the review file
   summary that `e1e58dc` is a Plan 03 emergent discovery (regression
   seed generated during this sprint), not a PR #1 drive-by.
5. **Negative `offset_ms` near buffer start can silently drop events.**
   Document in Plan 04 so the audio callback can decide whether a
   previous-buffer reclaim strategy is needed. At most 240 samples at
   48 kHz with the current ±5 ms calibration range.

<!-- gh-id: 3129881799 -->
### Copilot on [`doc/plans/plan-2026-04-23-01.md:118`](https://github.com/cmk/agogo/pull/4#discussion_r3129881799) (2026-04-23 09:50 UTC)

The `Channel` struct snippet says `shift_ms` is “clamped to ±300 ms”, but later the plan states v0.1 clamps shift to `0..=+300 ms` (negative shift deferred). Please make these consistent (either update the snippet comment or the later prose).
```suggestion
    pub shift_ms: f32,    // clamped to 0..=+300 ms in v0.1
```

<!-- gh-id: 3129881841 -->
### Copilot on [`doc/reviews/review-00004.md:194`](https://github.com/cmk/agogo/pull/4#discussion_r3129881841) (2026-04-23 09:50 UTC)

This review file still claims the `tick_monotonicity` bounded-swing restriction is “undocumented in the Review section” and marks it **Must-fix**, but `doc/plans/plan-2026-04-23-01.md` now includes deviation #5 documenting that restriction. Please update this section to reflect the current state (either remove the must-fix claim or change it to a confirmation).
```suggestion
**`tick_monotonicity` strategy restriction — now documented in the Review
section** (`channel/transform.rs:765-780`, `doc/plans/plan-2026-04-23-01.md`
§Review). The implementation uses `arb_divider_with_bounded_swing`,
which caps `|amount| < tick_count()` — so the test delivers a weaker
form: non-decreasing *only within the bounded-swing regime*. That scope
restriction is now recorded in the plan's Review section as deviation
#5, so the review note should be treated as confirmed rather than an
open documentation gap.
```

<!-- gh-id: 3129881871 -->
### Copilot on [`crates/core/src/channel/time.rs:213`](https://github.com/cmk/agogo/pull/4#discussion_r3129881871) (2026-04-23 09:50 UTC)

The `tick_monotonicity` test description says it checks that the per-channel “tick stream is non-decreasing”, but the assertion is on `ScheduledEvent.sample_index` (post Tick→Sample + shift/offset), not on the tick values. Please either assert monotonicity of `e.tick` (or both), or rename/reword the property so it matches what is actually being verified.

<!-- gh-id: 3129881894 -->
### Copilot on [`crates/core/src/control/event.rs:50`](https://github.com/cmk/agogo/pull/4#discussion_r3129881894) (2026-04-23 09:50 UTC)

`buffer_start_sample as i64` will wrap for values > `i64::MAX`, which can make the scheduler compute incorrect bounds (and potentially return spurious/missing events) for very long-running streams. If feasible, avoid the lossy cast by doing the math with `i128`/`u128` or by using checked/saturating unsigned arithmetic when applying `delta`.
```suggestion
    let delta_i128 = delta as i128;
    let swung_lo_signed = buffer_start_sample as i128 - delta_i128;
    let swung_hi_signed = buffer_end as i128 - delta_i128;
    let swung_lo = swung_lo_signed.clamp(0, u64::MAX as i128) as u64;
    let swung_hi = swung_hi_signed.clamp(0, u64::MAX as i128) as u64;
```

<!-- gh-id: 3129881928 -->
### Copilot on [`crates/cli/src/main.rs:96`](https://github.com/cmk/agogo/pull/4#discussion_r3129881928) (2026-04-23 09:50 UTC)

`shift_ms`/`offset_ms` are parsed as raw `f32`, which allows non-finite values like `NaN`/`inf` from the CLI. Since you already have `parse_non_negative_f32`/`parse_positive_f32` helpers, consider adding/using a “finite f32” parser here (and `parse_non_negative_f32` for `shift_ms`) so invalid inputs are rejected at the boundary instead of being silently coerced by `clamp`/float→int casts.

<!-- gh-id: 3129881942 -->
### Copilot on [`doc/plans/plan-2026-04-23-01.md:133`](https://github.com/cmk/agogo/pull/4#discussion_r3129881942) (2026-04-23 09:50 UTC)

In the pipeline description, “Shuffle … shift even-numbered ticks” is incorrect: `time::swing::effective_tick` only shifts off-beat (odd-parity T16) steps (`is_swung_step`), while even steps pass through unchanged. Please update this sentence to match the actual swing semantics so readers don’t implement the inverse behavior downstream.
```suggestion
   shift off-beat (odd-parity T16) steps; even steps pass through
   unchanged.
```

<!-- gh-id: 3129881964 -->
### Copilot on [`doc/reviews/review-00004.md:226`](https://github.com/cmk/agogo/pull/4#discussion_r3129881964) (2026-04-23 09:50 UTC)

In “Plan Conformance”, the review says the review-file summary attributes the PLL phase-wrap fix under “PR #1 drive-bys”. In this same file’s Summary, the PLL fix is already called out as a separate “Plan-03-emergent PLL fix” bullet, so this appears stale/contradictory. Please reconcile this section with the Summary so the review reads consistently.
```suggestion
  committed) and is correctly called out in the summary as a Plan 03
  emergent discovery, separate from the PR #1 drive-bys.
```

<!-- gh-id: 4161391597 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-04-23 09:51 UTC](https://github.com/cmk/agogo/pull/4#pullrequestreview-4161391597))

## Pull request overview

Implements Plan 03’s per-channel scheduling pipeline by introducing `channel/` modules and a runtime-parameterized `SampleTickConn` shim for Tick↔Sample conversion, plus a CLI trace command and a small PLL phase invariant fix.

**Changes:**
- Added `time::conn::SampleTickConn` (and property tests) to bridge Tick↔Sample with runtime `(sr, bpm, ppqn)`.
- Introduced `channel::{mode, transform, scheduler}` implementing the divide→shuffle→Tick→Sample→shift→offset pipeline and per-buffer event scheduling.
- Extended CLI with `agogo channel trace` and added regression seeds / doc updates (including a PLL phase wrap fix).

### Reviewed changes

Copilot reviewed 13 out of 13 changed files in this pull request and generated 9 comments.

<details>
<summary>Show a summary per file</summary>

| File | Description |
| ---- | ----------- |
| doc/reviews/review-00004.md | Adds a local review record for PR #4. |
| doc/plans/plan-2026-04-23-01.md | Adds Plan 03 spec + review/deviation notes. |
| doc/plans/plan-2026-04-22-02.md | Adds a footnote clarifying the `pll_phase_converges` measurement deviation. |
| crates/core/src/time/conn.rs | Adds `SampleTickConn` implementation + unit/proptests. |
| crates/core/src/control/sync/pll.rs | Wraps `PllOutput.phase` to 0.0 when f32 rounding yields 1.0; fixes a test-comment typo. |
| crates/core/src/lib.rs | Exposes the new `channel` module. |
| crates/core/src/channel/time.rs | Adds `Channel`/`ScheduledEvent` and the pure transform pipeline + tests. |
| crates/core/src/control/event.rs | Adds per-buffer `tick_stream` scheduler + tests. |
| crates/core/src/channel/mode.rs | Adds `ChannelMode` enum covering v0.1 spec surface (stubs except `MidiClock`). |
| crates/core/src/channel.rs | Adds the top-level `channel` module façade and re-exports. |
| crates/core/proptest-regressions/sync/pll.txt | Checks in a proptest regression seed for the PLL phase edge case. |
| crates/core/proptest-regressions/channel/transform.txt | Checks in a proptest regression seed for a transform property failure case. |
| crates/cli/src/main.rs | Adds `channel trace` CLI command and unit tests for the plan build gate. |
</details>






<!-- gh-id: 3129881992 -->
### Copilot on [`crates/cli/src/main.rs:331`](https://github.com/cmk/agogo/pull/4#discussion_r3129881992) (2026-04-23 09:51 UTC)

`start = u64::from(b) * args.frames as u64` can overflow for large `--frames/--buffers` values; in release builds this will wrap and produce incorrect sample indices in the trace output. Consider using `checked_mul` (and likely `checked_add` for the next steps) and returning a user-facing error when the requested trace range exceeds `u64`.
```suggestion
        let frames = u64::try_from(args.frames)
            .map_err(|_| format!("trace range exceeds u64: --frames {}", args.frames))?;
        frames
            .checked_mul(u64::from(args.buffers))
            .ok_or_else(|| {
                format!(
                    "trace range exceeds u64: --frames {} * --buffers {}",
                    args.frames, args.buffers
                )
            })?;
        let mut rows = Vec::new();
        for b in 0..args.buffers {
            let start = u64::from(b).checked_mul(frames).ok_or_else(|| {
                format!(
                    "trace range exceeds u64: --frames {} * --buffers {}",
                    args.frames, args.buffers
                )
            })?;
```

<!-- gh-id: 3129882017 -->
### Copilot on [`crates/core/src/time/conn.rs:282`](https://github.com/cmk/agogo/pull/4#discussion_r3129882017) (2026-04-23 09:51 UTC)

The `inner` doc comment’s example “(e.g. integer BPM on standard rates)” is misleading: integer BPM alone doesn’t imply `tick × sr × 60` is divisible by `bpm × ppqn` (e.g. 44.1 kHz at 120 BPM is not exact). Consider replacing the parenthetical with a concrete exact configuration (like 48 kHz / 120 BPM / 192 PPQN) or rewording to avoid implying this holds for all integer BPMs.
```suggestion
    /// `bpm × ppqn` (e.g. 48 kHz / 120 BPM / 192 PPQN); otherwise
```

<!-- gh-id: 3129910856 -->
#### ↳ cmk ([2026-04-23 09:55 UTC](https://github.com/cmk/agogo/pull/4#discussion_r3129910856))

Applied; the comment now reads `// clamped to 0..=+300 ms in v0.1`. Commit 3c7beff.

<!-- gh-id: 3129911415 -->
#### ↳ cmk ([2026-04-23 09:55 UTC](https://github.com/cmk/agogo/pull/4#discussion_r3129911415))

The `## Local review (2026-04-23)` section is a historical snapshot of the reviewer's report at that moment and stays intact (matching the append-only convention for `/sprint-review` output). The must-fix it flagged was resolved in commit 38de5c7, which added deviation #5 to the plan's Review section — the resolution lives in the plan (the durable doc), not via in-place edit of the snapshot.

<!-- gh-id: 3129911826 -->
#### ↳ cmk ([2026-04-23 09:56 UTC](https://github.com/cmk/agogo/pull/4#discussion_r3129911826))

Added a `w[0].tick <= w[1].tick` assertion alongside the existing `sample_index` check so both are verified — leaves the property's meaning aligned with its description. Commit 3c7beff.

<!-- gh-id: 3129912195 -->
#### ↳ cmk ([2026-04-23 09:56 UTC](https://github.com/cmk/agogo/pull/4#discussion_r3129912195))

Switched the delta math to `i128`, matching the suggested shape — `buffer_start_sample` and `buffer_end` now convert via `i128::from` and the signed difference can't wrap. Commit 3c7beff.

<!-- gh-id: 3129913028 -->
#### ↳ cmk ([2026-04-23 09:56 UTC](https://github.com/cmk/agogo/pull/4#discussion_r3129913028))

Added `parse_finite_f32` and wired `shift_ms` through `parse_non_negative_f32` (v0.1 rejects negative shift at the boundary), `offset_ms` through `parse_finite_f32`. NaN/inf now fail at the CLI. Commit 3c7beff.

<!-- gh-id: 3129913392 -->
#### ↳ cmk ([2026-04-23 09:56 UTC](https://github.com/cmk/agogo/pull/4#discussion_r3129913392))

Applied the suggested wording — the pipeline note now reads `shift off-beat (odd-parity T16) steps; even steps pass through unchanged`. Commit 3c7beff.

<!-- gh-id: 3129913774 -->
#### ↳ cmk ([2026-04-23 09:56 UTC](https://github.com/cmk/agogo/pull/4#discussion_r3129913774))

Same reasoning as the tick_monotonicity note above — the Local review section is the 2026-04-23 snapshot and stays intact. The Summary was updated in commit 38de5c7 to correctly attribute the PLL fix as a Plan-03-emergent discovery separate from the PR #1 drive-bys; that update is the authoritative reconciliation.

<!-- gh-id: 3129914184 -->
#### ↳ cmk ([2026-04-23 09:56 UTC](https://github.com/cmk/agogo/pull/4#discussion_r3129914184))

Applied. Pre-flight `buffers × frames` check emits a user-facing error if the product would exceed `u64`; the inner-loop `start` uses `checked_mul` (guaranteed safe after the pre-flight). Commit 3c7beff.

<!-- gh-id: 3129914565 -->
#### ↳ cmk ([2026-04-23 09:56 UTC](https://github.com/cmk/agogo/pull/4#discussion_r3129914565))

Applied — the example now reads `48 kHz / 120 BPM / 192 PPQN`, which is exact. Commit 3c7beff.
