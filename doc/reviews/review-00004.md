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
