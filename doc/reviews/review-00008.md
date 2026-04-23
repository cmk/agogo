# PR #8 — Link phase bridge (post-fxp)

## Summary

Completes the read-only half of the Ableton Link integration. PR #6
landed the scaffold + lifecycle-only `LinkClock`; PR #7 aligned its
trait signature to the post-fxp `Phase` type. This PR drops in the
real `phase_at_sample` body so the `PhaseSource::Custom(LinkClock)`
path is fully functional.

### What's new

- **`HostTimeAnchor { host_origin_micros: i64, sample_rate: u32 }`**
  on `LinkClock` carries the static sample-index → host-time mapping.
  `LinkClock::new(initial_bpm, anchor)` is the new constructor;
  `set_anchor` is the runtime setter Plan 09 will promote to an
  atomic-packed per-buffer path.
- **`phase_at_sample` bridge**: `host_micros =
  anchor.host_origin_micros + n × 10⁶ /
  anchor.sample_rate.get()` in `i128` (multi-day-safe), feeds
  `session.phase_at_time(host_micros, 1.0)`, returns via
  `fxp::f64_phase_to_phase` (handles `rem_euclid` + the "rounds to
  `2^32`" edge case). RT-safe. `sample_rate` is `NonZeroU32` so the
  division can't panic on a zero anchor.
- **Tests replacing the `#[should_panic]` guard**:
  `phase_at_sample_returns_valid_phase`,
  `phase_wraps_once_per_beat_at_120bpm_48k`,
  `phase_never_returns_exact_u32_max`,
  `set_anchor_shifts_the_sample_mapping`.
- **Proptest** `phase_delta_matches_tempo`: consecutive phase reads
  advance by the tempo-driven expected ULPs per stride (2²² ULP
  tolerance covers Link's session drift between captures).
- **CLI** `agogo link probe` gains `--sr <u32>` (default 48 000) and
  a `phase` column on every CSV row. Output header is now
  `t_ms,peers,tempo_bpm,phase`.

### Verification

- `cargo build --workspace` — clean (no `link` feature touched).
- `cargo test --workspace` — 204 core + 11 CLI tests green, 1
  pre-existing ignore.
- `cargo test -p agogo-cli --features link` — 12 tests green,
  including the updated `link_probe` smoke test.
- `cargo clippy --all-targets` / `cargo clippy … --features link`
  — both clean.
- E2E local: `cargo run -p agogo-cli --features link -- link probe
  --initial-bpm 120 --sr 48000 --duration-ms 500 --period-ms 100`
  prints five rows of CSV where the `phase` column advances by ~0.2
  per 100 ms at 120 BPM (one cycle per 500 ms beat), wrapping
  smoothly through 0/1.

### Deviations from the plan

See `doc/plans/plan-2026-04-23-05.md` §Review. Summary:

1. Anchor-shift test restructured to use one clock with two
   `set_anchor` calls — two independent `AblLink` instances have
   divergent session states, so cross-session comparisons fail.
2. Proptest named `phase_delta_matches_tempo` (more diagnostic than
   the plan's `phase_monotonic_across_random_anchors` heading).
3. Probe + tests construct one `LinkClock` with a placeholder
   `host_origin_micros: 0` anchor, read `clock_micros()` off the
   live instance, then `set_anchor` with the real origin — no
   throwaway second `AblLink::new`.
4. `phase_wraps_once_per_beat_at_120bpm_48k` tolerance is 2²² ULPs
   (not "1 ULP" from T2 task 2) — Link drifts a few µs between
   consecutive `capture_audio_session_state` calls.
5. CLI `phase` column emits 6 decimal places, not 4 — matches the
   bridge's actual resolution.
6. `HostTimeAnchor::sample_rate` is `NonZeroU32`, not `u32` — lifts
   the division's non-zero invariant into the type system.

### Out of scope / follow-ups

- **Plan 09 (bidirectional)** — tempo push, minimal transport FSM
  via `rust-fsm 0.7`, quantum snap at Channel layer, per-buffer
  atomic-packed host-time re-anchoring, two-peer integration tests
  behind `fixture_or_skip!("link_multicast")`.
- **Link CI** — adding CMake + C++ to the CI image so
  `--features link` runs there too. Separate chore.

## Local review (2026-04-23)

**Branch:** plan/2026-04-23-05
**Commits:** 4 (origin/main..plan/2026-04-23-05)
**Reviewer:** Claude (sonnet, independent)

---

### Commit Hygiene

Four commits — plan, two feats, doc-finalize — all under 72 chars
with conventional prefixes. TDD workflow shape intact. Nothing to
fix.

### Code Quality

`#![forbid(unsafe_code)]` present at both crate roots. Modern module
layout compliant.

`i128` overflow analysis in `phase_at_sample` is correct: `u64::MAX *
1_000_000` is ~1.84 × 10²⁵, twelve orders of magnitude below
`i128::MAX`. The `.clamp(i64::MIN, i64::MAX) as i64` cast is safe.

`f64_phase_to_phase` (crates/core/src/fxp.rs:135–147) confirms the
bridge's doc comment: `rem_euclid(1.0)` normalises, the `!(0.0..2^32
as f64).contains(&scaled)` guard catches the round-to-2^32 edge.

Throwaway-clock pattern at `crates/cli/src/main.rs` diff lines 81–93
(two `AblLink::new` calls at probe startup with explicit `drop` in
between) is already acknowledged in plan §Review deviation 3;
acceptable for now.

`anchor()` getter is a forward-looking API addition, not used in
this diff. Fine.

### Critical

None.

### Important

**1. `phase_wraps_once_per_beat_at_120bpm_48k` tolerance is 2²² ULPs,
but plan T2 task 2 says "1 ULP".**

`crates/host-link/src/link.rs:441`: `ulp < (1u32 << 22)`.

Plan T2 (line 129): "`phase_at_sample(0)` and `phase_at_sample(24_000)`
should be within **1 ULP** of each other."

The code's 2²² tolerance is correct — each `phase_at_sample` does a
fresh `capture_audio_session_state`, so two calls can diverge by the
inter-capture drift; 1 ULP is unreachable in practice. The §Review
deviation section doesn't document this 1-ULP → 4M-ULP change.

Fix: add a §Review deviation bullet recording why the tolerance was
relaxed.

**2. Phase column decimal places: plan says 4, code emits 6.**

`crates/cli/src/main.rs` diff line 41: `"{},{},{:.4},{:.6}"`.

Plan T4 spec (line 161): "`phase` is the `Phase.0 as f64 / 2^32`
with **4 decimal places**."

The code ships 6 decimal places. Defensible (phase has more
meaningful precision than tempo at sub-ms beat resolutions), but the
deviation is undocumented.

Fix: either revert the format to `{:.4}`, or add a §Review bullet
explaining the change.

### Test Coverage

`phase_circular_ulps` wrapping arithmetic is correct for `diff = 0`,
`diff = 1`, and `diff = 2^31`.

`phase_delta_matches_tempo` proptest: the expected-value formula
`stride * 2 * 2^32 / 48_000` matches the 120 BPM / 48 kHz rate.
BPM is fixed at 120.0 in the test; anchor origin ranges across
`[-10⁶, 10⁶]` µs. The formula's integer truncation makes `expected`
a lower bound — `phase_circular_ulps` handles the directional error
by measuring circular distance. Correct.

`set_anchor_shifts_the_sample_mapping` 2¹⁸ ULP tolerance is
plausible: same session state for both calls, only inter-call time
drift contributes error (~µs-level at 120 BPM = ~8.6K ULPs per µs).

### Plan Conformance

T0 (HostTimeAnchor), T1 (bridge), T3 (proptest), T4 (CLI
`--sr`/phase) all implemented. Proptest rename and anchor-shift test
restructuring are documented in §Review.

**Plan dependency graph labels the CLI branch "T3 CLI --sr + phase
column" but the tasks section numbers it T4.** Internal plan-doc
inconsistency with no code consequence.

### Risks

No `todo!()` stubs remain. No new dependencies. Breaking
`LinkClock::new` signature is OK: host-link has no downstream
consumers outside this workspace.

### Recommendations

**Must fix before push:**

1. Add a §Review deviation bullet noting
   `phase_wraps_once_per_beat_at_120bpm_48k` uses 2²² ULP tolerance
   rather than 1 ULP from T2 task 2, because each call re-captures
   session state.
2. Resolve the phase decimal-places gap between plan T4 ("4 decimal
   places") and `crates/cli/src/main.rs` (`{:.6}`). Either revert to
   `{:.4}` or add a §Review bullet documenting the change.

**Follow-up (Plan 09 or later):**

- Add a `link::now_micros()` free function so the throwaway-clock
  pattern in `link_probe` + tests can be retired.
- Randomise BPM in `phase_delta_matches_tempo` across Link's
  [20, 999] range to strengthen the invariant.
- Fix the plan's dependency graph labeling (T3 vs T4 for CLI) on the
  next doc touch.

<!-- gh-id: 4166680342 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-04-23 23:02 UTC](https://github.com/cmk/agogo/pull/8#pullrequestreview-4166680342))

## Pull request overview

Implements the Ableton Link “sample index → host-time → beat-phase” bridge so `LinkClock` can now return an fxp `Phase` via `PhaseSourceImpl::phase_at_sample`, and updates the CLI probe + docs accordingly.

**Changes:**
- Added `HostTimeAnchor` and wired it through `LinkClock` (`new`, `set_anchor`, `anchor`, `clock_micros`) to support sample→host-µs mapping.
- Implemented `LinkClock::phase_at_sample` using `capture_audio_session_state` + `SessionState::phase_at_time`, returning `Phase` via `f64_phase_to_phase`.
- Updated `agogo link probe` to accept `--sr` and emit a `phase` CSV column; added unit tests + proptest for the bridge behavior.

### Reviewed changes

Copilot reviewed 5 out of 5 changed files in this pull request and generated 3 comments.

<details>
<summary>Show a summary per file</summary>

| File | Description |
| ---- | ----------- |
| doc/reviews/review-00008.md | PR review record for Plan 08, summarizing the bridge/tests/CLI updates. |
| doc/plans/plan-2026-04-23-05.md | Plan document detailing the bridge design, tests, and CLI changes (and recorded deviations). |
| crates/host-link/src/link.rs | Adds `HostTimeAnchor`, implements `phase_at_sample`, and introduces unit/property tests for the bridge. |
| crates/host-link/src/lib.rs | Updates crate-level docs and re-exports `HostTimeAnchor` alongside `LinkClock` (feature-gated). |
| crates/cli/src/main.rs | Extends `link probe` with `--sr` and prints `phase` in CSV output. |
</details>






<!-- gh-id: 3134427613 -->
### Copilot on [`crates/host-link/src/link.rs:143`](https://github.com/cmk/agogo/pull/8#discussion_r3134427613) (2026-04-23 23:02 UTC)

`phase_at_sample` divides by `self.anchor.sample_rate` without guarding against `0`. Since `HostTimeAnchor.sample_rate` is a plain `u32` and `set_anchor` is public, a caller can construct an anchor with `sample_rate = 0`, causing a division-by-zero panic on the (advertised) RT-safe path. Consider enforcing non-zero at the type/API boundary (e.g., `NonZeroU32`), or validating in `new`/`set_anchor` and handling the zero case without panicking.
```suggestion
        let host_micros = if self.anchor.sample_rate == 0 {
            self.anchor.host_origin_micros
        } else {
            let offset = (i128::from(n) * 1_000_000) / i128::from(self.anchor.sample_rate);
            (i128::from(self.anchor.host_origin_micros) + offset)
                .clamp(i128::from(i64::MIN), i128::from(i64::MAX))
                as i64
        };
```

<!-- gh-id: 3134427674 -->
### Copilot on [`crates/host-link/src/link.rs:75`](https://github.com/cmk/agogo/pull/8#discussion_r3134427674) (2026-04-23 23:02 UTC)

The `set_anchor` doc says it is "safe to call from any thread", but the method requires `&mut self`, so it cannot be called concurrently with reads (or from multiple threads) without external synchronization. Suggest tightening the wording to avoid implying thread-safe/concurrent use until the anchor becomes atomic-packed in Plan 09.
```suggestion
    /// RT-safety: the setter writes two scalar fields and does not
    /// block, but it requires exclusive `&mut self` access, so it is
    /// not concurrently callable with readers without external
    /// synchronization. Plan 09 will promote the anchor to an
```

<!-- gh-id: 3134427717 -->
### Copilot on [`doc/reviews/review-00008.md:22`](https://github.com/cmk/agogo/pull/8#discussion_r3134427717) (2026-04-23 23:02 UTC)

nit: In the bridge formula, the docs refer to `anchor.origin`, but the actual field name is `host_origin_micros`. Updating this wording would avoid confusion for readers looking for an `origin` field on `HostTimeAnchor`.

<!-- gh-id: 3134449158 -->
#### ↳ cmk ([2026-04-23 23:07 UTC](https://github.com/cmk/agogo/pull/8#discussion_r3134449158))

Good catch — lifted the invariant into the type. `HostTimeAnchor::sample_rate` is now `NonZeroU32`, so the division in `phase_at_sample` can't panic on a zero anchor and the check stays off the RT-safe hot path. CLI call site goes through the existing `parse_positive_u32` gate then `NonZeroU32::new`. Commit d9735b1.

<!-- gh-id: 3134449279 -->
#### ↳ cmk ([2026-04-23 23:07 UTC](https://github.com/cmk/agogo/pull/8#discussion_r3134449279))

Fair — applied the suggested wording. The rustdoc now says `set_anchor` requires exclusive `&mut self` access and is not concurrently callable with readers without external synchronization. Plan 09's atomic-packed variant is still the plan for the audio-thread path. Commit d9735b1.

<!-- gh-id: 3134449392 -->
#### ↳ cmk ([2026-04-23 23:07 UTC](https://github.com/cmk/agogo/pull/8#discussion_r3134449392))

Right, `anchor.origin` was shorthand — fixed to `anchor.host_origin_micros` in the summary, and added a note that `sample_rate` is now `NonZeroU32` (from the div-by-zero fix above). Commit d9735b1.

<!-- gh-id: 3134475222 -->
### Copilot on [`doc/plans/plan-2026-04-23-05.md:75`](https://github.com/cmk/agogo/pull/8#discussion_r3134475222) (2026-04-23 23:13 UTC)

`HostTimeAnchor.sample_rate` is documented in the T0 code snippet as `u32`, but the implementation in `crates/host-link/src/link.rs` now uses `NonZeroU32`. This makes several later snippets/facts in this plan inaccurate too (e.g., T1 divides by `i128::from(self.anchor.sample_rate)` rather than `.sample_rate.get()`, and T4’s anchor construction uses `sample_rate: sr` instead of a `NonZeroU32`). Please update the plan’s code snippets/text so they match the actual API and avoid misleading readers.
```suggestion
    /// Sample rate for the sample → host-µs conversion.
    /// `NonZeroU32` encodes the invariant that the conversion
    /// denominator must never be zero; Link's time domain is still
    /// microseconds rather than rate-typed samples.
    pub sample_rate: NonZeroU32,
```

<!-- gh-id: 3134475253 -->
### Copilot on [`doc/reviews/review-00008.md:62`](https://github.com/cmk/agogo/pull/8#discussion_r3134475253) (2026-04-23 23:13 UTC)

The review record still describes a “throwaway-clock pattern” with two `AblLink::new` calls (both in the top Deviations list and again in the Local review Code Quality section), but the current implementation constructs one `LinkClock` with a placeholder anchor and then calls `set_anchor` using `clock_micros()`. Please update these references so the review doc matches the final code in this PR.

<!-- gh-id: 3134475276 -->
### Copilot on [`crates/cli/src/main.rs:352`](https://github.com/cmk/agogo/pull/8#discussion_r3134475276) (2026-04-23 23:13 UTC)

In `link_probe::probe`, `sr` is already validated as non-zero by the CLI parser, but the code still uses `sr.max(1)` before converting to `NonZeroU32`. This silently changes behavior for any non-CLI caller of `probe` (e.g., `sr = 0` becomes `1`) and makes the comment about the invariant less accurate. Prefer preserving the invariant explicitly (e.g., convert with `NonZeroU32::new(sr)` and fail fast, or change the function signature to accept a `NonZeroU32`).
```suggestion
        // The CLI parser (parse_positive_u32) already enforces `sr >= 1`.
        // Preserve that invariant explicitly here so non-CLI callers
        // fail fast instead of silently changing `sr = 0` to `sr = 1`.
        let sr = NonZeroU32::new(sr).expect("probe requires a non-zero sample rate");
```

<!-- gh-id: 4166735345 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-04-23 23:13 UTC](https://github.com/cmk/agogo/pull/8#pullrequestreview-4166735345))

## Pull request overview

Copilot reviewed 5 out of 5 changed files in this pull request and generated 3 comments.





<!-- gh-id: 3134493366 -->
#### ↳ cmk ([2026-04-23 23:17 UTC](https://github.com/cmk/agogo/pull/8#discussion_r3134493366))

Fair — applied. Plan T0 snippet now shows `sample_rate: NonZeroU32`, T1 uses `self.anchor.sample_rate.get()`, and T4's Anchor paragraph describes the single-clock `set_anchor` pattern + the `{:.6}` phase precision. Commit 798aeb7.

<!-- gh-id: 3134493485 -->
#### ↳ cmk ([2026-04-23 23:17 UTC](https://github.com/cmk/agogo/pull/8#discussion_r3134493485))

Partial — updated the Summary's Deviations list + removed the "throwaway-clock pattern" Out-of-scope bullet, so the top of the file now matches the single-clock code. The Local review section further down stays intact: it's a dated reviewer snapshot (convention from Plan 07 round 1), and rewriting it retroactively would blur the audit trail. The Summary is the evolving record; commit 798aeb7.

<!-- gh-id: 3134493578 -->
#### ↳ cmk ([2026-04-23 23:17 UTC](https://github.com/cmk/agogo/pull/8#discussion_r3134493578))

Good catch — dropped the `sr.max(1)` silent coercion. Non-CLI callers now fail fast on `sr = 0` via the `NonZeroU32::new(sr).expect(...)`; the comment points at `parse_positive_u32` as the CLI-side guard. Commit 798aeb7.
