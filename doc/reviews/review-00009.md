# PR #9 — Post-fxp enforcement MR #1: scaffolding + PLL migration

## Summary

First of two MRs for Plan 10 (post-fxp enforcement). Pure scaffolding
— no public API changes, no stored-state flips. Sets up the new
primitives that MR #2 will use to do the full boundary sweep.

### What ships

- **Rev bump** to the upstream `connections` crate at `883b4ea`
  (MR !3 merged). Picks up the `F64F00..F64F12` lawful float-to-
  rung conns, the `FloatExt` / `Extended` wrappers, the U0 sample-
  tier rename, and the crate-level naming legend. No agogo call-
  sites referenced the renamed symbols, so the bump is mechanical.
- **T0 `PicoSampleConn`** in `crates/core/src/time/conn.rs`. A
  runtime-parameterised Pico ↔ Sample Conn-lookalike mirroring
  upstream `connections::Conn<Pico, Sxx>`. Uses
  `connections::sample::Q48_16` samples so the bidirectional Galois
  laws (`ceil ⊣ inner ⊣ floor`) hold exactly at every IEEE-
  reasonable rate — including the 44.1 kHz family. Eleven tests
  covering the full Galois battery plus spot checks at 44.1 / 48
  kHz, the exact boundary, the half-sample bracket, negative
  offsets, and round-trips in both directions.
- **T1 `tempo_to_hz` + `bits_q48_16_to_seconds`** PI-exempt helpers
  in `crates/core/src/fxp.rs`. These replace the 6× open-coded
  `(bpm.0 as f64 / 1.0e6) * ppq as f64 / 60.0` and
  `bits as f64 / (65_536.0 * sr as f64)` formulas scattered
  through the PLL. Also drops three unused f32 argv-boundary
  helpers (`f32_bpm_to_tempo`, `f32_jitter_us_to_sigma`,
  `f32_threshold_to_q15`) plus their tests; re-exports
  `FloatExt` / `Extended` / the `F64F??` + `F12F??` constants
  through `agogo_core::fxp` so `agogo-cli` can reach them without
  a direct `connections` dep.
- **T6 PLL migration** — the six formula sites in `sync::pll`
  (production + proptest) now consume `tempo_to_hz` /
  `bits_q48_16_to_seconds`. Zero behavioural diff by construction;
  the only win is readability and a single site per formula for
  future edits.

### What's deferred to MR #2

- T2 Channel state → `Micro` (the main structural change).
- T3/T4 CLI argv f64 + `TraceArgs` / `ProbeRow` drop floats.
- T5 `LinkClock` exposes `Tempo`.
- T7 `scripts/check-floats.sh` grep gate + CI wire.
- T8 CLAUDE.md rule additions + `doc/reviews/review-calibration.md`
  Patterns 9 / 10 / 11.

MR #2 is a natural next step: every remaining task is a boundary
migration that consumes the primitives landed here.

### Design deviations

- **`F64TMP` / `F64PHS` / `F64Q15` Conn constants dropped from
  T1.** Plan originally specified full-shape
  `Conn<FloatExt<f64>, Extended<T>>` for those targets. All three
  target types are u32 / u16-backed, which doesn't fit upstream's
  i64-backed `float_conn!` macro without duplicating it. The
  existing argv-boundary helpers (`f64_bpm_to_tempo`,
  `f64_phase_to_phase`) are semantically equivalent for the "ceil"
  direction that the CLI parser uses. Deferred beyond MR #2 — see
  plan §Deferred.
- **`PicoSampleConn` uses Q48.16 samples, not plain `i64`.** An
  earlier revision of T0 used `i64` samples; at 44.1 kHz (`10¹²`
  not divisible by `sr`) the adjoint laws fail because plain
  integers can't carry sub-sample Pico offsets. Switching to
  `Q48_16` matches upstream `F12SXX` exactly and makes both laws
  hold at every rate.

## Test plan

- [x] `cargo build --workspace` — clean.
- [x] `cargo test --workspace` — 226 passed (215 core + 11 cli, + 2
  ignored fixture-gated); zero failures; 0.20 s.
- [x] `cargo clippy --all-targets -- -D warnings` — clean.
- [x] T0 adjoint / monotonicity / round-trip proptests at
  44.1 / 48 / 88.2 / 96 / 176.4 / 192 kHz — all green.
- [x] T6 proptest `phase_rms_under_jitter_bound` still passes post-
  migration (zero behavioural diff, confirmed).

## Local review (2026-04-23)

**Branch:** `plan/2026-04-23-07`
**Commits:** 6 (origin/main..HEAD)
**Reviewer:** Claude (sonnet, independent)

---

### Commit Hygiene

All six commits carry valid prefixes (`plan:`, `task:`, `feat:`, `doc:`). The `feat(time):` and `feat(fxp):` commits separate T0 and T1 cleanly; T6 is its own `feat(pll):` commit. The `doc:` finalisation commit is last, as required by the TDD workflow. Each commit type matches the content; no unrelated changes are mixed. Clean.

### Code Quality

**`gcd_i128` uses `unsigned_abs() as i128`** (`crates/core/src/time/conn.rs`, line 357): `u128::unsigned_abs()` returns `u128`. Casting to `i128` wraps for values ≥ 2^127. In this codebase `num` is `1e12` and `den` is at most `192_000 × 65_536 ≈ 1.26e10`, so overflow is physically impossible. No bug, but the cast is silent; a `try_from` or a comment would be more defensive.

All existing float sites in `pll.rs` now carry `// PI-exempt` annotation as T6 requires. The `bits_q48_16_to_seconds` formula matches the old open-coded formula exactly (both expressions are `bits / (HZ * 65536)`). No rounding reordering. T6 behavioural diff is zero.

No `mod.rs` files introduced. Module layout follows repo convention. No `unsafe`. Lint attributes appear consistent with existing crates.

### Critical Issue

**`FloatExt::Finite(NaN)` wrapping in jitter conversion** (`crates/cli/src/main.rs`, lines 52–55 of the diff / hunk at `+463`):

```rust
let jitter: Pico = match F64F12.ceil(FloatExt::Finite(f64::from(jitter_us) * 1.0e-6)) {
    Extended::Finite(p) => p,
    Extended::NegInf | Extended::PosInf => Pico(0),
};
```

If `jitter_us` is `f32::NAN` or `f32::INFINITY`, `f64::from(jitter_us) * 1.0e-6` is `f64::NAN` or `f64::INFINITY`. The code wraps that in `FloatExt::Finite(...)`, which explicitly asserts "this value is finite". The old `f32_jitter_us_to_sigma` checked `!us.is_finite()` before doing anything, and the PR comment claims "matches the previous bespoke helper's NaN → 0 behaviour". That claim is only correct if `F64F12.ceil` happens to handle `Finite(NaN)` gracefully — but the caller has already told it "this is finite", so the upstream conn has no reason to guard against NaN. Confidence: **90**.

There is also a secondary difference in rounding: the old helper used `.round()` (nearest), this code uses `.ceil()` (round up). For the jitter-sigma use case, ceil is more conservative (never under-estimates jitter), so this is probably acceptable — but it is an undocumented behavioural change.

### Test Coverage

**T0 proptest #6 absent** (`crates/core/src/time/conn.rs`): The plan's Verification section lists six required proptests for `PicoSampleConn`. Five are present. Proptest #6 — `tick_pico_sample_triangle` (`tick → pico → sample` agrees with `tick → sample` within tolerated rounding) — is not in the diff. Confidence: **85**.

**Proptest for `bits_q48_16_to_seconds`** (lines 186–193): The test compares `got` against `expected` where both are computed by the exact same expression (`bits as f64 / ((sr as f64) * (1u64 << 16) as f64)`). This verifies copy-correctness of the formula but not its numerical contract.

**`arb_pico_sample_conn` uses `prop_oneof!` without frequency weights**: The CLAUDE.md convention says "Use `prop_oneof!` with frequency weights to bias toward boundary values and edge cases." 44.1 kHz is the only rate where `10^12` is not divisible by `sr`, so it's the structurally interesting case. Confidence: **80**.

### Plan Conformance

- **T0**: Implemented. Struct uses Q48.16 samples (deviation documented). Three methods present. Five of six required proptests present.
- **T1**: Partially implemented. `tempo_to_hz` and `bits_q48_16_to_seconds` present. `F64TMP`/`F64PHS`/`F64Q15` Conn constants absent — deviation documented in plan §Design Deviations and §Deferred. `f32_bpm_to_tempo`, `f32_jitter_us_to_sigma`, `f32_threshold_to_q15` deleted.
- **T6**: All six open-coded formula sites replaced. Conformant.
- **T2–T5, T7–T8**: Deferred to MR #2 as intended.

### Risks

**`f32_threshold_to_q15` deleted without visible replacement in the diff**: The diff only shows the `sync_trace` module import section. The PR says 226 tests pass and build is clean, so this must be handled — but the review cannot verify it from the diff alone.

No TODOs or stubs are introduced. No security concerns.

### Recommendations

**Must fix before push:**

1. ✅ **Fixed in commit `a41849d`**. `FloatExt::Finite(NaN)` in jitter
   conversion: `crates/cli/src/main.rs` now guards
   `jitter_s.is_finite()` before the `F64F12.ceil` match; NaN / ±∞
   collapse to `Pico(0)` up front.

2. ✅ **Addressed in commit `a41849d`**. `tick_pico_sample_triangle`
   shipped as the spot check
   `sample_tick_and_pico_sample_agree_at_120bpm_48k` — a proptest
   needs a combined `(bpm, ppq, sr, tick) → sample` bridge agogo
   doesn't have. Plan's T0 section now reflects the spot-check
   approach.

**Follow-up (future work):**

3. ✅ **Fixed in commit `a41849d`**. `arb_pico_sample_conn` now
   weights the 44.1 kHz family 3:1 over the 48 kHz family.

4. ✅ **Fixed in commit `a41849d`**. `gcd_i128` carries an inline
   comment explaining why the `unsigned_abs() as i128` cast cannot
   wrap for the inputs this codebase supplies.

5. ✅ **Fixed in commit `a41849d`**. `tempo_to_hz` and
   `bits_q48_16_to_seconds` have hand-computed spot checks
   (120 BPM / 24 PPQ → 48 Hz, 48 000 × 2¹⁶ bits → 1 s, etc.)
   alongside the tautological property tests.

<!-- gh-id: 3135926951 -->
### Copilot on [`crates/core/src/time/conn.rs:329`](https://github.com/cmk/agogo/pull/9#discussion_r3135926951) (2026-04-24 06:45 UTC)

`PicoSampleConn` doc comment references `plan-2026-04-24-01`, but there is no such plan file in `doc/plans/` in this branch. This cross-reference looks stale/incorrect; please update it to an existing plan (e.g. the current plan file) or remove the reference.
```suggestion
/// tracked in `connections` deferred work.
```

<!-- gh-id: 3135926987 -->
### Copilot on [`crates/core/src/time/conn.rs:365`](https://github.com/cmk/agogo/pull/9#discussion_r3135926987) (2026-04-24 06:45 UTC)

`PicoSampleConn::inner` converts an `i128` intermediate back to `i64` with `as i64`. For sufficiently large `Q48_16` inputs this will wrap/truncate and return an incorrect `Pico` value. Consider clamping/saturating to `i64::{MIN,MAX}` (or returning an `Extended<Pico>`/`Result`) before the cast to avoid silent wraparound.
```suggestion
        let pico = n
            .div_euclid(self.den)
            .max(i128::from(i64::MIN))
            .min(i128::from(i64::MAX));
        connections::fixed::Pico(pico as i64)
```

<!-- gh-id: 3135927017 -->
### Copilot on [`crates/cli/src/main.rs:470`](https://github.com/cmk/agogo/pull/9#discussion_r3135927017) (2026-04-24 06:45 UTC)

The comment claims this conversion matches the previous jitter helper’s semantics, but the old helper rounded to nearest while this path uses `F64F12.ceil` (round-up). If round-up is intentional, the comment should state the behavioural change (and rationale); otherwise adjust the conversion to preserve the prior rounding semantics.

<!-- gh-id: 3135927026 -->
### Copilot on [`crates/cli/src/main.rs:473`](https://github.com/cmk/agogo/pull/9#discussion_r3135927026) (2026-04-24 06:45 UTC)

This comment is internally inconsistent: `-∞` cannot be wrapped in `FloatExt::Finite`, and the code already guards `!jitter_s.is_finite()` before constructing `FloatExt::Finite`. Please correct/clarify the explanation of what cases are being guarded against (e.g. NaN/±∞ vs finite-but-out-of-range values) to avoid misleading future readers.
```suggestion
        // inputs (`NaN` / `±∞`) are rejected up front and mapped to
        // `Pico(0)`. For finite inputs, any conversion result that is
        // still out of range (`Extended::NegInf` / `Extended::PosInf`)
        // also saturates to `Pico(0)`, matching the previous bespoke
        // helper's "safe default" behavior for invalid jitter values.
```

<!-- gh-id: 4168437557 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-04-24 06:45 UTC](https://github.com/cmk/agogo/pull/9#pullrequestreview-4168437557))

## Pull request overview

Scaffolding for Plan 10 (post-fxp enforcement), introducing new fixed-point conversion primitives and migrating PLL float formulas to shared helpers, plus a dependency bump to pick up upstream `connections` lawful float-to-rung work.

**Changes:**
- Bump upstream `connections` dependency revision.
- Add `PicoSampleConn` (runtime Pico↔Sample bridge using `Q48_16`) with property/spot tests.
- Add PI-exempt helpers (`tempo_to_hz`, `bits_q48_16_to_seconds`) and migrate `sync::pll` + CLI trace code to use them.

### Reviewed changes

Copilot reviewed 7 out of 8 changed files in this pull request and generated 8 comments.

<details>
<summary>Show a summary per file</summary>

| File | Description |
| ---- | ----------- |
| doc/reviews/review-00009.md | Adds a local review record for this MR. |
| doc/plans/plan-2026-04-23-07.md | Adds Plan 10 writeup covering MR #1 and MR #2 scope. |
| crates/core/src/time/conn.rs | Introduces `PicoSampleConn` and its test suite. |
| crates/core/src/sync/pll.rs | Replaces duplicated Tempo/seconds formulas with shared helpers. |
| crates/core/src/fxp.rs | Re-exports upstream float wrappers/Conn constants; adds PI-exempt helpers + tests; removes unused f32 boundary helpers. |
| crates/cli/src/main.rs | Updates `sync_trace` argv-boundary conversions to use `f64_bpm_to_tempo`, `F64F12`, and `tempo_to_hz`. |
| Cargo.toml | Updates `connections` git rev. |
| Cargo.lock | Locks the updated `connections` rev. |
</details>






<!-- gh-id: 3135927045 -->
### Copilot on [`doc/reviews/review-00009.md:113`](https://github.com/cmk/agogo/pull/9#discussion_r3135927045) (2026-04-24 06:45 UTC)

This local review section describes a `FloatExt::Finite(NaN)` bug and shows a code snippet that no longer matches the current implementation in this PR (the actual code now guards non-finite jitter inputs). Please update the review record to reflect the final code state (e.g. mark the issue as fixed or adjust the snippet/analysis).

<!-- gh-id: 3135927060 -->
### Copilot on [`doc/reviews/review-00009.md:123`](https://github.com/cmk/agogo/pull/9#discussion_r3135927060) (2026-04-24 06:45 UTC)

The “Test Coverage” findings are out of date relative to the code in this PR: `tick_pico_sample_triangle` is now present (as a unit spot-check), `arb_pico_sample_conn` uses weighted `prop_oneof!`, and the PI-exempt helper tests include hand-computed witnesses. Please revise this section so it accurately reflects what’s in the diff (or explicitly note what is still missing vs intentionally deferred).

<!-- gh-id: 3135927075 -->
### Copilot on [`doc/plans/plan-2026-04-23-07.md:145`](https://github.com/cmk/agogo/pull/9#discussion_r3135927075) (2026-04-24 06:45 UTC)

In T0, the design sketch still specifies `PicoSampleConn` as Pico↔`i64` samples with the `sample = pico * sr / 10^12` formula, but the implementation (and later “Design deviations”) use `connections::sample::Q48_16` and a pico-per-bit ratio. Please update this section so the plan’s API sketch and formulas match the actual intended design.

<!-- gh-id: 3135927089 -->
### Copilot on [`doc/plans/plan-2026-04-23-07.md:156`](https://github.com/cmk/agogo/pull/9#discussion_r3135927089) (2026-04-24 06:45 UTC)

T0’s proptest list still states `tick_pico_sample_triangle` as a required proptest. In this PR it’s implemented as a single hand-computed spot check instead. Please either (a) update the plan to reflect the spot-check approach, or (b) keep the plan as-is and add the missing proptest to match the stated verification contract.

<!-- gh-id: 3135947867 -->
#### ↳ cmk ([2026-04-24 06:50 UTC](https://github.com/cmk/agogo/pull/9#discussion_r3135947867))

Fixed — the comment now just says "tracked in the `connections` crate's deferred work" without naming a specific upstream filename that isn't visible from this repo.

<!-- gh-id: 3135948565 -->
#### ↳ cmk ([2026-04-24 06:50 UTC](https://github.com/cmk/agogo/pull/9#discussion_r3135948565))

Fixed — `inner` now clamps the i128 division result to `[i64::MIN, i64::MAX]` before the `as i64` cast. Doc comment explains the realistic bound (~13.6 hours of audio at 44.1 kHz before saturation can bite) so a reader knows this isn't a live failure mode but a defensive guard.

<!-- gh-id: 3135949524 -->
#### ↳ cmk ([2026-04-24 06:50 UTC](https://github.com/cmk/agogo/pull/9#discussion_r3135949524))

Fixed — added a comment paragraph stating the round→ceil change explicitly and its rationale (more conservative for jitter-sigma, never under-estimates, agrees exactly at the CLI's actual-value inputs). No behaviour change from the intent.

<!-- gh-id: 3135950465 -->
#### ↳ cmk ([2026-04-24 06:50 UTC](https://github.com/cmk/agogo/pull/9#discussion_r3135950465))

Fixed — rewrote the comment so it describes what the current guarded code actually handles: NaN / ±∞ are rejected by `is_finite()` and map to `Pico(0)`, and finite-but-out-of-range values are separately caught by `Extended::PosInf` / `NegInf` in the match. Removed the stale mention of `Finite(-∞)` leaking through (that was the motivation for adding the guard, not a live code path anymore).

<!-- gh-id: 3135951235 -->
#### ↳ cmk ([2026-04-24 06:50 UTC](https://github.com/cmk/agogo/pull/9#discussion_r3135951235))

Fixed — the Recommendations subsection of the local review now carries `✅ Fixed in commit a41849d` annotations on each of the 2 must-fix and 3 follow-up items, with a one-line note on what shipped. The original findings stay in place as the audit trail.

<!-- gh-id: 3135951928 -->
#### ↳ cmk ([2026-04-24 06:51 UTC](https://github.com/cmk/agogo/pull/9#discussion_r3135951928))

Same fix as the previous thread — the Recommendations subsection is now annotated per-item with commit `a41849d`. The Test Coverage findings themselves stay as the original reviewer audit trail; the resolution lives in the Recommendations block so a reader walking top-to-bottom sees "found X, fixed here" without the two sections drifting.

<!-- gh-id: 3135952751 -->
#### ↳ cmk ([2026-04-24 06:51 UTC](https://github.com/cmk/agogo/pull/9#discussion_r3135952751))

Fixed — the T0 API sketch now shows `Q48_16` samples with the pico-per-bit ratio `num, den, sr` struct shape, and the implementation paragraph describes i128 arithmetic mirroring upstream's `pico_conn!` (ceil uses `ceil_div(p·den, num)`, inner uses `floor_div(s_bits·num, den)`, floor uses `floor_div(p·den + den−1, num)` as the Galois right-adjoint of inner).

<!-- gh-id: 3135953810 -->
#### ↳ cmk ([2026-04-24 06:51 UTC](https://github.com/cmk/agogo/pull/9#discussion_r3135953810))

Went with option (a). The plan's T0 proptest list now describes the five properties that actually shipped (adjoint upper/lower, monotone ceil/floor, floor≤ceil, inner-round-trip both directions) plus the `sample_tick_and_pico_sample_agree_at_120bpm_48k` spot check, with a note on why the triangle is a spot check rather than a proptest (needs a combined `(bpm, ppq, sr, tick)` bridge that agogo doesn't need outside this test).

<!-- gh-id: 3136094155 -->
### Copilot on [`crates/core/src/time/conn.rs:1303`](https://github.com/cmk/agogo/pull/9#discussion_r3136094155) (2026-04-24 07:21 UTC)

`pico_sample_inner_adjacent_bits_differ_by_at_most_ratio` uses `wrapping_sub` to compute `step`. If `inner` ever regresses to a wraparound bug (e.g. `p0=i64::MAX`, `p1=i64::MIN`), `wrapping_sub` can yield a small positive value (like `1`) and the assertion may still pass, defeating the purpose of the test. Compute the delta with a non-wrapping subtraction (e.g. cast to `i128`/`u128` and subtract) and/or explicitly assert `p1 >= p0` before checking the max-step bound.
```suggestion
            prop_assert!(
                p1 >= p0,
                "bits {}→{}: output regressed from {} to {}",
                bits, bits + 1, p0, p1
            );
            let step = (p1 as i128) - (p0 as i128);
            let max_step = (psc.num / psc.den) as i128 + 1;
            prop_assert!(
                step <= max_step,
```

<!-- gh-id: 4168652842 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-04-24 07:21 UTC](https://github.com/cmk/agogo/pull/9#pullrequestreview-4168652842))

## Pull request overview

Copilot reviewed 8 out of 9 changed files in this pull request and generated 2 comments.





<!-- gh-id: 3136094213 -->
### Copilot on [`crates/core/src/time/conn.rs:1105`](https://github.com/cmk/agogo/pull/9#discussion_r3136094213) (2026-04-24 07:21 UTC)

`arb_pico` and `arb_q48_16` bound the generator domain “to keep intermediate arithmetic safe”, but the conversions here are i64×(sr×2^16) and i64×10^12, which still fit comfortably in `i128` even for the full `i64` input domain. This bounding reduces coverage in the exact regions where saturation/wrap bugs tend to hide and also conflicts with the proptest convention added in this PR (CLAUDE.md:87-96) to default to the full input-type domain. Consider switching these to `any::<i64>()` (with weighted `Just(i64::{MIN,MAX})` arms if needed) or update the comment to justify a different reason for bounding (e.g. performance/realistic-value bias) and add explicit boundary cases accordingly.
```suggestion
    /// Use the full `i64` domain: the pico/sample conversions here do
    /// their intermediate arithmetic in `i128`, so `i64 × den` and
    /// `i64 × 10¹²` remain safely representable while still exercising
    /// boundary values where wrap/saturation bugs tend to hide.
    fn arb_pico() -> impl Strategy<Value = Pico> {
        any::<i64>().prop_map(Pico)
    }

    /// Use the full underlying Q48.16 bit domain for the same reason:
    /// the conversion intermediates fit in `i128`, and full-range
    /// generation gives much better coverage of edge cases.
    fn arb_q48_16() -> impl Strategy<Value = Q48_16> {
        any::<i64>().prop_map(Q48_16::from_bits)
```

<!-- gh-id: 3136126878 -->
#### ↳ cmk ([2026-04-24 07:28 UTC](https://github.com/cmk/agogo/pull/9#discussion_r3136126878))

Fixed — split the check into an explicit `p1 >= p0` monotonicity assertion (so a regressed wrap from `i64::MAX → i64::MIN` trips the guard rather than silently looking like a small positive step) and a non-wrapping `i128` subtraction for the bound check. Exactly the suggested shape.

<!-- gh-id: 3136128495 -->
#### ↳ cmk ([2026-04-24 07:28 UTC](https://github.com/cmk/agogo/pull/9#discussion_r3136128495))

Right on — my own CLAUDE.md rule cited against me, exactly the anti-pattern I just codified.

Fixed with a small refinement after running the expanded generator: `arb_pico` now `any::<i64>()` + explicit `Just(i64::MIN/MAX/0)` arms. `arb_q48_16` expanded the same way *did* expose a real boundary — at `|bits| ≳ i64::MAX × den / num`, `inner(s)` clamps to `Pico(i64::MIN/MAX)`, and the adjoint law degenerates at that flattening (many distinct s values all map to the same saturated pico). That's a designed behaviour of the saturating clamp, not a bug the adjoint law should catch.

So the split:
- `arb_pico`: full i64. Used by every adjoint/monotone test.
- `arb_q48_16_non_saturating`: ±10¹⁵ bits, documented *why* (stay inside the non-saturating domain) with a pointer to `pico_sample_inner_saturates_at_i64_boundaries` which covers the saturation behaviour directly.
- Full i64 Q48.16 exercises the monotonicity / adjacency tests via `any::<i64>()` inline rather than a named strategy — those tests don't depend on the adjoint law and actively want the saturation plateau exercised.

Documented the "why bounded" rationale above the `arb_q48_16_non_saturating` definition and in each proptest using it, per the amended CLAUDE.md rule.
