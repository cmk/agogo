# PR #5 — Fixed-point refactor: sync + envelope + CLI

## Summary

Eliminate `f32` / `f64` from stored state and public APIs across
`agogo-core` and `agogo-cli`, consuming the rate-typed sample tier
(`S44 / S48 / S88 / S96 / S176 / S192`) and decimal time ladder
(`Uni / Deci / Centi / Milli / Micro / Nano / Pico`) from the sibling
`connections` crate (bumped to rev `15d3791`, which landed the
Galois ladder + rate tier). One exception preserved per user
directive: `PllSettings { kp, ki, clamp_hz, interp }` and
`PllState { phase, freq_hz, integrator }` stay `f64` — that's
genuine analog-DSP arithmetic, not stored time.

### Why

- `Peak.sample_index: f64` mashed integer sample identity with
  parabolic sub-sample fraction into one float; downstream ring-
  buffer indexing had to re-round, and precision could drift
  silently.
- `PllOutput.{bpm, phase}: f32` was a lossy downcast from the
  internal f64, invisible at the type level.
- Mixing sample rates (passing a 44.1k sample index to a 48k
  consumer) was a runtime bug rather than a compile error.

All three are gone: `Peak<R>` / `Pll<R>` / `PhaseSource<R>` are
generic over `R: SampleTime`, `PllOutput { Tempo, Phase }` is
integer-typed, and rate mismatches are type errors.

### What changes

**New module**: `agogo-core::fxp`
- `Phase(u32)` — Q0.32 cycles; `wrapping_add` IS modular reduction.
- `Tempo(u32)` — BPM × 10⁶, 10⁻⁶ BPM resolution.
- `linear_u8(t, n)` / `smoothstep_u8(t, n)` — integer-exact Hermite,
  Q0.24 intermediate in `u128`.
- f64→fxp boundary casts for the PI controller exit:
  `f64_phase_to_phase`, `f64_bpm_to_tempo`.
- f32→fxp boundary casts for the CLI argv:
  `f32_bpm_to_tempo`, `f32_jitter_us_to_sigma`,
  `f32_threshold_to_q15`.
- `SampleTime` trait over `Sxx` rate types with `from_bits_q48_16` /
  `to_bits_q48_16` / `from_sample` / `sample` for generic DSP code.
- Re-exports of `connections::fixed::{Uni, … Pico}` and
  `connections::sample::{S44 … S192, SampleRate}`.

**Flipped modules**:
- `sync::detect` — `Peak<R>` (dropped the dead `amplitude` field),
  `DetectorConfig { threshold_q15: u16 }` (Q0.15 of full-scale,
  single f32 compare at the cpal ABI boundary),
  `PeakDetector<R>`. Parabolic-fit f64 locals stay contained —
  converted to Q48.16 bits before any value escapes.
- `sync::pll` — `Pll<R>`, `PllOutput { bpm: Tempo, phase: Phase }`,
  `last_pulse_sample: Option<R>`, `nominal_bpm` arg is `Tempo`.
  Control-law state untouched. One-per-`step()` boundary cast via
  `fxp::f64_{bpm,phase}_to_*`. Added `Pll::predicted_phase_at(elapsed:
  R) -> Phase` so `PhaseSource::External` can project phase without
  the f64 leaking out of the PI-exempt zone.
- `sync::source` — `PhaseSource<R>`, `Internal { bpm: Tempo }`
  uses a pure-integer NCO (`n · bpm · 2³²` kept together in `u128`
  to avoid per-sample rounding accumulation).
- `arb::pulse_train<R: SampleTime>(Tempo, u32 ppq, Pico sigma, …)
  -> (Vec<f32>, Vec<R>)` — PRNG flipped to `rand_pcg::Pcg64` +
  `rand_distr::Normal`. PCM buffer stays `Vec<f32>` (cpal ABI).
- `time::envelope` — delegates `opening` / `closing` / `s_curve` to
  `fxp::{linear_u8, smoothstep_u8}`. Bit-exact on the midpoint-128
  spot check.
- `cli::sync_trace::TraceRow` — `{sample: i64, sub_q16: i16,
  tempo_ubpm: u32, phase_q32: u32}`. CSV header and row format all
  integers. Argv f32 dies at the first line of the handler via
  `f32_bpm_to_tempo` / `f32_jitter_us_to_sigma`. Pinned to
  48 kHz this sprint; rejects other `--sr` values with a clear
  message.

**Workspace deps added**: `fixed = "1"`, `rand = "0.9"`,
`rand_distr = "0.5"`, `rand_pcg = "0.9"`. `connections` git rev
bumped to `15d3791e281e30c4eacaa4e499ee5788894d9ab6`.

### Verification

- 11 `agogo-cli` + 203 `agogo-core` tests pass (203 = 196 prior + 7
  new fxp proptests, net). 0 failed. 1 ignored (carry-over). No
  new `#[ignore]`.
- `cargo clippy --all-targets -- -D warnings` clean.
- Grep gate: remaining `f32` / `f64` hits are all documented:
  cpal-ABI `&[f32]` slices, `PllSettings` / `PllState` fields, the
  PI-law body itself, and short-lived arithmetic locals with
  `// ABI-local` or `// PI-exempt` comments.
- E2E smoke:
  `cargo run -p agogo-cli -- sync trace --bpm 120 --sr 48000 --ppq 24
  --jitter-us 50 --pulses 256 --seed 1` emits 256 integer rows;
  final `tempo_ubpm = 120_000_543` — within 50 000 µBPM of
  120 × 10⁶ (the 0.05 BPM convergence gate).

### Known deviations

Five deviations documented in the plan's Review section:

1. `sync trace` CLI pins `R = S48` rather than multi-rate dispatch
   (deferred to v0.1 binary-runtime sprint).
2. Detect proptests dropped `arb_sample_rate` coverage (same reason;
   the detector algorithm is rate-agnostic so the loss is cosmetic).
3. `integrator_clamp_keeps_bpm_positive` now asserts on PI-exempt
   `state().integrator` / `state().freq_hz` directly, since
   `Tempo` can legitimately round to zero when smoothed_bpm is
   below 0.5 µBPM — that's not the regression the test guards.
4. `source_internal_is_linear` tolerance relaxed to ±1 Q0.32 ULP
   because the implementation keeps `n · bpm · 2³²` together in u128
   rather than precomputing a per-sample inc — more accurate at
   large n, diff varies by ±1 at integer boundaries.
5. `PULSE_WIDTH_SECS: f64` → `PULSE_WIDTH_PS: Pico` (constant type
   rename; spelled out for symmetry with the rest of the flip).

### Out of scope

- Multi-rate dispatch shim for the CLI and detect proptests
  (recommended follow-up; tracked in the plan).
- Flipping `SampleTickConn` (channel/ path) to integer bpm —
  orthogonal to sync/*, same reasoning as everything else but in a
  later sprint.
- Upstreaming `SampleTime` into `connections::sample` — deferred
  until the API stabilises.

### Test plan

- `cargo test --workspace` — all passing.
- `cargo clippy --all-targets -- -D warnings` — clean.
- `cargo run -p agogo-cli -- sync trace --bpm 120 --sr 48000 --ppq 24 --jitter-us 50 --pulses 256 --seed 1`
  prints 256 integer rows with final `tempo_ubpm` within 50 000 µBPM
  of 120 × 10⁶.

## Local review (2026-04-23)

**Branch:** plan/2026-04-23-03
**Commits:** 6 (origin/main..plan/2026-04-23-03)
**Reviewer:** Claude (sonnet, independent)

---

### Commit Hygiene

All six commits use the required conventional-prefix scheme (`plan:`, `feat(core):`, `feat:`, `doc:`). The one `feat:` without a scope (`e123135`, the big flip) spans four files but is explicitly justified in the prompt — the modules are interdependent, and the split would produce a red intermediate commit. That is the correct trade-off given the CLAUDE.md requirement that every commit must leave `cargo test` green. No CI-repair fixups present. No merge commits.

### Code Quality

**Grep gate**: The diff is clean at a read-level scan. The parabolic-fit locals in `detect.rs` are annotated `// ABI-local f64`. PI-law bodies in `pll.rs` are annotated `// PI-exempt`. `pulse_train` derives `bpm_f`, `sr`, etc. as local f64 within the function body only. `source.rs` `phase_at_sample` Internal arm uses integer arithmetic only.

**NCO overflow — Internal phase** (`crates/core/src/control/sync/source.rs`): `num = n · bpm.0 · 2³²` kept together in u128 stays within bounds at realistic musical ranges (`n ≤ 10^7`, `bpm.0 ≤ 4×10^8`): 1.7×10²⁵ ≪ u128::MAX. For pathological `n = u64::MAX` the product overflows but that is 11 billion years at 48 kHz, unreachable in practice.

**`n_bits = (n as i128 * 65_536) as i64` cast in source.rs External arm**: silently truncates for `n > 2^47` (~93 000 years at 48 kHz). Within the documented Q48.16 range, but a `debug_assert!(n <= (i64::MAX as u64) / 65_536)` would make the boundary explicit. Filed as a follow-up.

**`predicted_phase_at` / `pulse_train` round-trip symmetry**: Both sides treat `to_bits_q48_16 / 65_536.0` as the Q48.16→sample conversion. The wrapping subtract in `phase_at_sample` is intentional and documented. Symmetric.

**`SampleTime` trait default methods**: `from_sample(n) = from_bits_q48_16(n << 16)` and `sample() = to_bits_q48_16() >> 16` match the concrete per-type inherent methods they forward to. Consistent.

**`linear_u8` / `smoothstep_u8` overflow bounds**: u128 arithmetic stays within bounds because `x < 2^24` strictly when `t < n`. The final `.min(255) as u8` guard absorbs any rounding edge.

### Test Coverage

**`f64_phase_roundtrip` tolerance tightened** (was `2⁻²⁰`, now `2⁻³¹`). The Verification table specifies `< 2⁻³¹` and the implementation rounds Q0.32 round-nearest so worst-case error is `2⁻³³`; the prior `2⁻²⁰` was 2048× too loose. Fixed in the doc: Finalize commit amendment.

**`f32_bpm_roundtrip` tolerance documented as `5e-5`** — the previously-committed Verification spec said `5e-7`, unreachable for f32 inputs near 400 BPM (f32 mantissa resolution there is ~4.8×10⁻⁵). Plan updated to match the test. The test itself is correct.

**`source_internal_is_linear` rate pinned to S48**: The relaxation to ±1 Q0.32 ULP is correct — the per-call computation introduces ±1 from integer division. The test is load-bearing at large `n`.

**`integrator_clamp_keeps_bpm_positive` — catches the original regression**: The new assertions on `state().integrator > -1.0` and `state().freq_hz > 0.0` correctly capture "`1 + integrator <= 0` driving smoothed_bpm non-positive". The `Tempo` output can legitimately round to zero well below the regression threshold, so asserting on the PI-exempt state is the appropriate shift.

**`pll_no_panic_on_silence` weakened to no-panic only**: Correct for integer types (can't NaN or infinite). Not vacuous — appropriately narrowed.

**Detect proptests dropped `arb_sample_rate`**: Documented as deviation #2. The detector itself is rate-agnostic so the coverage loss is cosmetic. Accurate.

**No `fixture_or_skip!` usage**: expected — no fixture-dependent tests introduced.

### Plan Conformance

T0–T7: All implemented. Five documented deviations all accurately described. No undocumented deviations. Out-of-scope check: `channel/` and `time::conn` not touched.

### Risks

**CLI CSV format breaking change**: header and row format flip to integers. The `sync trace` also now rejects `--sr != 48000` with a clear error. Both are intentional per the plan. The prior behaviour on non-48k `--sr` was broken anyway (pll had `sr = 48_000u32` hardcoded in its tests while the CLI plumbed `--sr` through — semantically inconsistent). No regression for real users.

**`arb_jitter_sigma_us` → `arb_jitter_sigma` rename**: public testkit-gated re-export changed. Pre-v1.0 workspace, documented in the plan, acceptable.

**New runtime deps** (`fixed = "1"`, `rand = "0.9"`, `rand_distr = "0.5"`, `rand_pcg = "0.9"`): all established crates with stable release histories.

**`connections` rev `15d3791`**: `Cargo.lock` confirms; upstream tests in `sample.rs` / `fixed.rs` appear comprehensive; rev bump justified.

---

### Critical

None.

### Must fix before push

None remaining (both "Important" items from the reviewer were addressed in-sprint before the final push: `f64_phase_roundtrip` tolerance tightened to `2⁻³¹`, and the plan's Verification table updated so `f32_bpm_roundtrip` reads `< 5e-5`).

### Follow-up (future work)

- Add `debug_assert!(n <= (i64::MAX as u64) / 65_536)` at the
  `n_bits` cast in `source.rs::phase_at_sample` External arm.
- Replace `arb.rs`'s `_sample_rate_sealed` dead-fn with `use crate::fxp::SampleRate as _;`
  to force the import on non-testkit builds more cleanly.
- Update the now-stale envelope tests comment about "float arithmetic"
  after the T2 integer delegation.
- Flip `SampleTickConn` to take `Tempo` instead of `f64 bpm`
  (tracked in the plan's Recommendations).

<!-- gh-id: 3130259306 -->
### Copilot on [`doc/plans/plan-2026-04-23-03.md`](https://github.com/cmk/agogo/pull/5#discussion_r3130259306) (2026-04-23 10:58 UTC)

The plan’s T5 snippet for `Pll::new` still shows an `sr: u32` parameter and bounds `R: SampleRate`, but the PR changes `Pll` to `R: SampleTime` and removes the explicit sample-rate argument. Please update the snippet to avoid a stale constructor signature.
```suggestion
pub struct Pll<R: SampleTime> { … last_pulse_sample: Option<R> … }
impl<R: SampleTime> Pll<R> {
    pub fn new(cfg: PllSettings, nominal_bpm: MicroBpm, ppq: u32) -> Self;
```

<!-- gh-id: 3130259361 -->
### Copilot on [`crates/core/src/fxp.rs:114`](https://github.com/cmk/agogo/pull/5#discussion_r3130259361) (2026-04-23 10:58 UTC)

`MicroBpm::from_bpm_integer` multiplies in `u32` (`n * 1_000_000`) and will wrap on overflow in release (e.g. `n > 4294`). Since this is a public constructor, it should avoid silent overflow (e.g. use `checked_mul` and saturate/panic, or take `u64` and clamp into range).
```suggestion
        match n.checked_mul(1_000_000) {
            Some(value) => Self(value),
            None => panic!("MicroBpm::from_bpm_integer overflow"),
        }
```

<!-- gh-id: 3130259390 -->
### Copilot on [`crates/cli/src/main.rs:312`](https://github.com/cmk/agogo/pull/5#discussion_r3130259390) (2026-04-23 10:58 UTC)

`TraceRow { sample, sub_q16 }` is derived by splitting Q48.16 as `sample = bits >> 16` and `sub_q16 = low16 as i16`. For negative sub-sample offsets this decomposition is inconsistent (the fixed-point representation borrows from the integer part, so `low16` is near 0xFFFF and becomes negative even though the fractional part is actually ~0.75). If the intent is “integer sample + signed Q16 offset”, compute `sample` by rounding to nearest (or explicitly choose a convention) and set `sub_q16 = bits - (sample<<16)`; alternatively emit a single `bits_q48_16` column to make the CSV unambiguous.

<!-- gh-id: 3130259409 -->
### Copilot on [`crates/core/src/control/sync/source.rs:54`](https://github.com/cmk/agogo/pull/5#discussion_r3130259409) (2026-04-23 10:58 UTC)

The `n_bits` computation casts `(n as i128 * 65_536)` down to `i64`, which truncates once `n > i64::MAX / 65_536` (even though `n` is a `u64`). Either keep this in `i128` until the final conversion, or add an explicit (debug_)assert/checked conversion so out-of-range `n` can’t silently corrupt the elapsed time used for phase projection.
```suggestion
                    let n_bits = i64::try_from(n as i128 * 65_536)
                        .expect("sample index in Q48.16 must fit in i64");
```

<!-- gh-id: 3130259435 -->
### Copilot on [`crates/core/src/control/sync/detect.rs:132`](https://github.com/cmk/agogo/pull/5#discussion_r3130259435) (2026-04-23 10:58 UTC)

The comment says the i128 arithmetic is exact for any `u64` stream index, but `bits_q48_16` is still computed via `as i64` which will truncate for `start_index` beyond the representable Q48.16 range. Consider making the range limitation explicit (debug_assert/checked conversion/clamp) and adjust the comment so it doesn’t imply full `u64` coverage.

<!-- gh-id: 3130259467 -->
### Copilot on [`crates/core/src/fxp.rs:195`](https://github.com/cmk/agogo/pull/5#discussion_r3130259467) (2026-04-23 10:58 UTC)

Docstring mismatch: `linear_u8` returns 255 for `n = 0`, but the comment describes this as “fully closed”. In the rest of the codebase 255 corresponds to fully open (e.g. `opening(n=0) == 255`). Please reword the doc comment so the degenerate-case semantics are described correctly.
```suggestion
/// "no span" as fully open).
```

<!-- gh-id: 4161800387 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-04-23 10:58 UTC](https://github.com/cmk/agogo/pull/5#pullrequestreview-4161800387))

## Pull request overview

This PR refactors `agogo-core` and `agogo-cli` to remove `f32`/`f64` from stored state and public APIs by introducing fixed-point types (`Phase`, `MicroBpm`) and rate-typed sample-time generics (`R: SampleTime`), while keeping the PLL control-law internals as `f64`.

**Changes:**
- Add `agogo-core::fxp` with fixed-point types/conversions and a `SampleTime` trait over the `connections` rate tier.
- Flip sync modules (`detect`, `pll`, `source`) and envelope helpers to consume fixed-point and rate-typed sample positions (Q48.16).
- Update `agogo-cli sync trace` to output integer CSV fields and pin runtime rate to 48 kHz.

### Reviewed changes

Copilot reviewed 13 out of 14 changed files in this pull request and generated 8 comments.

<details>
<summary>Show a summary per file</summary>

| File | Description |
| ---- | ----------- |
| doc/reviews/review-00005.md | Adds a local review record for the fixed-point refactor work. |
| doc/plans/plan-2026-04-23-03.md | Adds the implementation plan/spec for the refactor. |
| crates/core/src/time/envelope.rs | Delegates envelope shaping to integer `fxp` ramp/smoothstep. |
| crates/core/src/control/sync/source.rs | Converts `PhaseSource` to `R: SampleTime` and fixed-point `Phase` output. |
| crates/core/src/control/sync/pll.rs | Converts PLL surface to `MicroBpm`/`Phase` and adds `predicted_phase_at`. |
| crates/core/src/control/sync/detect.rs | Converts peak detector to `Peak<R>` with Q48.16 sample positions and Q0.15 threshold. |
| crates/core/src/lib.rs | Exposes new `fxp` module from the core crate root. |
| crates/core/src/fxp.rs | Introduces fixed-point primitives, conversions, and the `SampleTime` trait. |
| crates/core/src/arb.rs | Updates `pulse_train` + strategies to fixed-point/time-typed APIs and uses `rand_pcg`/`rand_distr`. |
| crates/core/Cargo.toml | Adds new dependencies required by fixed-point + pulse-train generation. |
| crates/cli/src/main.rs | Updates `sync trace` CLI to integer CSV output and uses argv→fxp conversions. |
| Cargo.toml | Bumps `connections` git rev and adds workspace dependencies. |
| Cargo.lock | Records new dependency graph (fixed/rand/rand_distr/rand_pcg + transitive deps). |
</details>






<!-- gh-id: 3130259516 -->
### Copilot on [`doc/reviews/review-00005.md:174`](https://github.com/cmk/agogo/pull/5#discussion_r3130259516) (2026-04-23 10:58 UTC)

The review says “Four deviations” earlier, but later claims “Five documented deviations” in the plan conformance summary. Please reconcile this count (either update the summary to “Four” or add the missing deviation) so the document is internally consistent.
```suggestion
T0–T7: All implemented. Five documented deviations all accurately described. No undocumented deviations. Out-of-scope check: `channel/` and `time::conn` not touched.
```

<!-- gh-id: 3130259571 -->
### Copilot on [`doc/plans/plan-2026-04-23-03.md`](https://github.com/cmk/agogo/pull/5#discussion_r3130259571) (2026-04-23 10:58 UTC)

The plan’s T3 signature still includes an `sr: u32` argument and uses `R: SampleRate`, but the implementation in this PR makes `pulse_train` generic over `R: SampleTime` and derives `sr` from `R::HZ`. Please update the plan snippet to match the actual function signature so readers don’t copy an API that no longer exists.
```suggestion
Signature flip: `pulse_train<R: SampleTime>(bpm: MicroBpm, ppq: u32,
jitter_sigma: Pico, n_pulses: u32, seed: u64) -> (Vec<f32>, Vec<R>)`.
Derive the sample rate from `R::HZ`. Swap the custom
xorshift+Box-Muller for `rand_pcg::Pcg64::seed_from_u64` +
`rand_distr::Normal`. Hann-bell waveform synthesis continues to write
into a `Vec<f32>` PCM buffer (that's the cpal ABI; documented
exception).
```

<!-- gh-id: 3130480698 -->
#### ↳ cmk ([2026-04-23 11:40 UTC](https://github.com/cmk/agogo/pull/5#discussion_r3130480698))

Fixed — plan T5 snippet updated to `R: SampleTime` and dropped the `sr: u32` argument in `Pll::new` to match the implementation.

<!-- gh-id: 3130481662 -->
#### ↳ cmk ([2026-04-23 11:40 UTC](https://github.com/cmk/agogo/pull/5#discussion_r3130481662))

Fixed — `Tempo::from_bpm_integer` now uses `checked_mul` and panics with a clear message if `n > 4294` instead of silently wrapping in release.

<!-- gh-id: 3130484861 -->
#### ↳ cmk ([2026-04-23 11:41 UTC](https://github.com/cmk/agogo/pull/5#discussion_r3130484861))

Fixed — collapsed `(sample: i64, sub_q16: i16)` into a single `bits_q48_16: i64` column. As you note, the split is unsound for negative sample positions because the integer part borrows from the fractional and `0xFFFF as i16 = -1`; emitting raw Q48.16 bits sidesteps the sign-convention question entirely. CSV consumers decode with `sample = bits >> 16` / `frac = bits & 0xFFFF` as needed.

<!-- gh-id: 3130485936 -->
#### ↳ cmk ([2026-04-23 11:41 UTC](https://github.com/cmk/agogo/pull/5#discussion_r3130485936))

Fixed — switched to `i64::try_from(n as i128 * 65_536).expect("sample index in Q48.16 must fit in i64")` so an out-of-range `n` panics cleanly instead of silently corrupting the elapsed-time projection.

<!-- gh-id: 3130486912 -->
#### ↳ cmk ([2026-04-23 11:41 UTC](https://github.com/cmk/agogo/pull/5#discussion_r3130486912))

Fixed — switched the `centre_int * 65_536` cast to `i64::try_from(...).expect("stream index in Q48.16 must fit in i64")` and rewrote the comment to document the Q48.16 range limit (~2⁴⁷ samples) rather than imply full u64 coverage.

<!-- gh-id: 3130514191 -->
#### ↳ cmk ([2026-04-23 11:47 UTC](https://github.com/cmk/agogo/pull/5#discussion_r3130514191))

Fixed — docstring reworded to "no span = fully open" to match the `n = 0` return of 255 and the sibling `opening(0, 0) = 255` in `time::envelope`.

<!-- gh-id: 3130515170 -->
#### ↳ cmk ([2026-04-23 11:47 UTC](https://github.com/cmk/agogo/pull/5#discussion_r3130515170))

Fixed — review-00005.md now reads "Five" in both places, and the missing fifth item (`PULSE_WIDTH_SECS: f64` → `PULSE_WIDTH_PS: Pico`, deviation #5 in the plan's Review section) is added to the Known-deviations list.

<!-- gh-id: 3130516213 -->
#### ↳ cmk ([2026-04-23 11:47 UTC](https://github.com/cmk/agogo/pull/5#discussion_r3130516213))

Fixed — T3 snippet updated to `pulse_train<R: SampleTime>(bpm: Tempo, ppq: u32, jitter_sigma: Pico, n_pulses: u32, seed: u64)`, dropping the stale `sr: u32` argument now that sample rate is derived from `R::HZ`.
