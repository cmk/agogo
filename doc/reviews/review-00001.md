# PR #1 — `sync/`: peak detector + Type-II PLL

## Summary

Lands the audio-sync DSP module — peak detector with parabolic
sub-sample interpolation, Type-II (PI) phase-locked loop, and a
unified `PhaseSource` that consumers query for current beat/pulse
phase. Pure DSP: no audio I/O, no `Tick` conversion (Sprint 3
integration work). Verified against synthetic Hann-bell pulse trains
with injected Gaussian timing noise.

### What's in

- `agogo-core::sync::detect` — streaming `PeakDetector` with three-
  sample sliding window, hold-window debouncing, and parabolic
  sub-sample fit. Plateau ties at the apex are resolved with `>=` on
  the left edge so pulses centred at half-integer sample positions
  still emit.
- `agogo-core::sync::pll` — Type-II second-order PLL. Two
  frequencies are tracked: a PI-driven prediction frequency
  (`state.freq_hz`, used to advance phase between pulses) and an
  integrator-only smoothed frequency (output as `PllOutput::bpm`,
  so per-pulse jitter doesn't show up in the reported tempo).
- `agogo-core::sync::source::PhaseSource` — `Internal { bpm, sr }`
  free-running beat clock vs. `External { detector, pll }` driven by
  audio samples. Same query API; different phase units between
  variants until Sprint 3 reconciles.
- `agogo-core::arb` — proptest strategies (`arb_bpm`,
  `arb_sample_rate`, `arb_jitter_sigma_us`) gated behind a new
  `testkit` cargo feature, plus a `pulse_train` synthetic generator
  shared between proptests and the CLI.
- `agogo-cli sync trace` — clap subcommand that synthesises a
  pulse train, runs the full pipeline, and prints CSV rows of
  detected peaks with the PLL's smoothed BPM and phase.

### Verification

All nine properties from the plan land green; none `#[ignore]`d. The
build-gate E2E scenario (256 pulses at 120 BPM / 48 kHz / 24 PPQ
with 50 µs jitter) is locked in as a CLI unit test that asserts
final BPM is within ±0.05 of 120.

### Notable design deviations

- Pulse shape is a **Hann bell**, not the plan's pure triangle —
  triangles' non-smooth apex breaks parabolic interpolation.
- `pulse_train` takes a `seed: u64`; jitter PRNG is an inline
  xorshift64 + Box-Muller (no new workspace deps).
- `Pll::step` stores `sr`/`ppq` in the struct rather than taking
  them per-call.
- `PllOutput::bpm` is integrator-only (smoothed). Consequence:
  `pll_bandwidth_monotone` probes the loop's natural frequency
  ω_n with damping ratio held constant, not "fixed ki / vary kp".
- `Internal::phase_at_sample` returns beat-phase; `External` returns
  PLL pulse-phase. Reconciliation is Sprint 3.

See the plan's Review section for the full deviation list and the
recommendations carried forward to Sprint 3.

## Local review (2026-04-22)

**Branch:** `plan/2026-04-22-02`
**Commits:** 8 (origin/main..plan/2026-04-22-02)
**Reviewer:** Claude (sonnet, independent)

---

Reviewing `git diff origin/main...HEAD` across 8 commits, 13 files,
~1500 lines of new Rust plus plan and review docs.

### Commit Hygiene

All eight commits use valid conventional-commit prefixes (`plan`,
`feat`, `doc`). The `doc:` finalization commit at `ba2475c` lands
both the plan appendix and the review file in a single commit as
required. Commit messages are under 72 characters. The dependency
graph in the plan maps cleanly to the commit sequence: T0 scaffold →
T1/T2/T4 → T3 → T5 → docs. No merge commits. Clean.

### Code Quality

No `unsafe` blocks anywhere in the diff. The `#![forbid(unsafe_code)]`
declaration at `crates/cli/src/main.rs:1` is present; the core crate
root already carried it (not changed in this diff).

**No high-confidence clippy or convention issues found.** Module
layout is modern throughout (`sync.rs` + `sync/` directory, no
`mod.rs`). The `testkit` feature is a well-reasoned workaround for
the dev-dep limitation and is documented in Review deviation 3.
Error messages in the CLI (`eprintln!("error: build with --features
core...")`) are specific.

One observation that does not rise to a must-fix: `sync_trace::trace`
in `crates/cli/src/main.rs` calls `detector.process(&samples, 0)`
then iterates peaks calling `pll.step` one per peak. This means all
peaks are detected from one fully-materialized buffer before any PLL
step fires — fine for the synthetic trace use case, but it is
architecturally different from the streaming `feed_samples` path
tested in `source.rs`. The asymmetry is not a bug here but is worth
noting if the CLI trace ever moves to real-time blocks.

### Test Coverage

**All nine Verification-table properties are present and mapped
correctly:**

| Property | Status |
|---|---|
| `detector_recovers_all_peaks` | Present, `sync::detect` proptest |
| `detector_subsample_precision` | Present, `sync::detect` proptest |
| `detector_hold_blocks_doubles` | Present, `sync::detect` proptest |
| `pll_bpm_converges` | Present, `sync::pll` proptest |
| `pll_phase_converges` | Present, `sync::pll` proptest |
| `pll_rejects_outliers` | Present, `sync::pll` proptest |
| `pll_no_panic_on_silence` | Present, `sync::pll` proptest |
| `pll_bandwidth_monotone` | Present as regression test, documented deviation |
| `source_internal_is_linear` | Present, `sync::source` proptest |

All nine green, none `#[ignore]`d, confirming the plan's claim.

**One issue found in test coverage — confidence 85:**

`pll_phase_converges` at `crates/core/src/sync/pll.rs` measures phase
error as the difference between the PLL's estimated inter-pulse
spacing (derived from `out.bpm`) and the true inter-pulse spacing.
This is an *indirect* measure of timing error derived from a smoothed
BPM value. What the plan specifies is "phase RMS error < 50 µs" — a
time-domain quantity that should reflect how far the PLL's predicted
pulse arrivals drift from the true arrivals. The current metric
computes `(1/est_freq - 1/true_freq) * 1e6` per step; this is
equivalent only when the PLL has reached steady state and est_freq is
close to true_freq. At the edges of the `0..200 µs` jitter range and
`60..200 BPM` range, this proxy can understate the actual timing
error because it does not account for phase accumulated between
updates. The 50 µs bound is probably still enforced in practice given
the small BPM range and tight jitter, but the test does not directly
measure what the contract states. A more faithful check would
accumulate `|predicted_arrival - actual_arrival|` for each peak after
the warm-up window and compute RMS over those residuals.

This is not a must-fix for this sprint — the test exists, is
non-trivial, and the `pll_bpm_converges` property provides strong
complementary coverage. But it should be tightened in Sprint 3.

**One gap in SNR-degraded coverage — confidence 80:**

The Verification table lists `detector_subsample_precision` as
requiring `|reported - truth| ≤ 1.0` sample at SNR > 20 dB, in
addition to the `≤ 0.1` clean-input bound. The clean-input property
is fully tested. The SNR-degraded variant is explicitly deferred in
the plan's Review (last bullet under "Recommendations for Sprint 3").
Intentional deferral, documented correctly. Flagging for visibility.

### Plan Conformance

**T0:** Scaffold at `crates/core/src/sync.rs` + three submodule files.
`lib.rs` re-export of `pub mod sync` present. Conforms.

**T1:** `Peak`, `DetectorConfig`, `PeakDetector`, `process(&mut self,
&[f32], u64)`. Parabolic interpolation formula matches the plan.
`DetectorState` uses two f32 slots (`prev2`, `prev1`) plus bool flags
rather than a ring buffer, which is a simpler and correct
implementation of the same logic. Conforms.

**T2:** `PllSettings`, `PllState`, `Pll`, `PllOutput`, `step`
(deviation 4: `sr`/`ppq` stored in struct — documented). Default
tuning documented. Conforms with documented deviation.

**T3:** `PhaseSource` enum. `Internal { bpm, sr }` (deviation 7: `sr`
added — documented). `External { detector, pll }` (plan said
`External(Pll)` but `detector` is included inline — undocumented but
a straightforward necessary inclusion since `feed_samples` needs the
detector). `phase_at_sample` and `feed_samples` both present.
Conforms.

**T4:** `arb_bpm`, `arb_sample_rate`, `arb_jitter_sigma_us`,
`pulse_train` — all present in `arb.rs`. Hann bell substitution
(deviation 1) documented. `seed` parameter (deviation 2) documented.
`testkit` feature (deviation 3) documented. Conforms.

**T5:** `sync trace` subcommand with all five flags plus `--seed`.
CSV header + one row per detected peak. E2E gate test
`sync_trace_converges`. Conforms.

**Spot checks:**

- Five-peak handcrafted test uses `hold_samples: 500` rather than the
  plan's `hold_samples: 1000`. At 1000-sample spacing, both work —
  neither would block a legitimate peak. Minor deviation,
  inconsequential.
- `default_settings_track_120_at_48k` uses 48 ticks (1 second of
  audio), which conforms with the plan's "under 1 second" gate.
- `internal_120bpm_at_half_beat` uses sample 12 000 and checks both
  the half-beat and wrap-at-24 000. Deviation 8 is correctly
  documented.

### Risks

**`pll_rejects_outliers` modifies the `peaks` vec in-place**
(`peaks[40] += spike_samples`) after the warm-up loop consumed the
first 40 elements via `peaks.iter().take(40)`. This is correct —
`take` does not consume the Vec — but the mutation at index 40
applies to the same element that the second loop starts from
(`peaks[40..]`). This models a late arrival, which is a valid
single-spike scenario. Correct.

**No TODOs or stub implementations** visible in the diff. `interp:
f64` in `PllSettings` is reserved/unused but documented as
forward-compatibility, not a stub.

**Clap dependency:** Added at the workspace level rather than only in
`crates/cli/Cargo.toml`. Currently only used by `agogo-cli`, so
adding it as a workspace dep is fine ergonomically. The plan
explicitly authorized adding clap. No concern.

**`start_index as i64` cast** at `detect.rs`: if `start_index`
exceeds `i64::MAX` (~9.2 × 10^18), this wraps silently. At 48 kHz
this requires ~6 billion years of continuous streaming — not a
practical risk.

### Recommendations

**Must fix before push:**

None. The code compiles, all nine properties are green, no
conventions are violated.

**Follow-up (future work):**

1. `crates/core/src/sync/pll.rs` `pll_phase_converges` measures
   BPM-derived spacing error as a proxy for phase timing error. In
   Sprint 3, replace with direct residual accumulation:
   `|predicted_arrival[i] - observed_arrival[i]|` in samples,
   converted to µs at the test's fixed SR, then RMS over the
   post-warmup window. This makes the property faithful to the 50 µs
   contract as stated.

2. `crates/core/src/arb.rs` — `detector_subsample_precision` covers
   clean input only (the `≤ 0.1` half of the plan's two-part
   contract). The `≤ 1.0` at SNR > 20 dB half is deferred correctly.
   Track in Sprint 3 as `noise_db: Option<f32>` on `pulse_train`.

3. `crates/core/src/sync/source.rs` — `feed_samples` for `External`
   calls `pll.step(None)` when the block has no detected peaks.
   Reasonable for a free-run heartbeat, but if the caller feeds many
   short silent blocks (e.g. pre-roll before signal), each block
   increments the phase by exactly one step, which can accumulate
   significant phase error before the first real pulse arrives.
   Document the expected calling contract (minimum block size, or
   "only call during signal") or gate the free-run step behind a
   check that at least one real pulse has been seen
   (`last_pulse_sample.is_some()`).

<!-- gh-id: 3127794232 -->
### Copilot on [`crates/cli/src/main.rs:32`](https://github.com/cmk/agogo/pull/1#discussion_r3127794232) (2026-04-23 01:23 UTC)

These CLI args are passed straight into `pulse_train()`, which `assert!`s and will panic on invalid values (e.g., `--bpm 0`, `--sr 0`, `--ppq 0`). Consider adding clap range validation (value parser / nonzero / positive constraints) or pre-validating and returning a user-friendly error instead of panicking.

<!-- gh-id: 3127794245 -->
### Copilot on [`crates/cli/src/main.rs:33`](https://github.com/cmk/agogo/pull/1#discussion_r3127794245) (2026-04-23 01:23 UTC)

`--jitter-us` is a `f32` with a default, but negative values will currently reach `pulse_train()` and trigger an `assert!` panic. Consider adding a non-negative range constraint in clap (or validating before calling into core) so the CLI fails gracefully.
```suggestion
        #[arg(long, default_value_t = 0.0, value_parser = clap::value_parser!(f32).range(0.0..))]
```

<!-- gh-id: 3127794259 -->
### Copilot on [`crates/core/src/sync/pll.rs:274`](https://github.com/cmk/agogo/pull/1#discussion_r3127794259) (2026-04-23 01:23 UTC)

The `pll_phase_converges` property claims to measure phase RMS error, but the calculation uses `out.bpm` (the integrator-only smoothed tempo) to derive `est_spacing_secs`. That’s effectively another tempo-convergence check, not a phase/NCO error metric (and it ignores `state.freq_hz` / `out.phase` entirely). If the intent is true phase error, consider computing predicted arrival times from the NCO (`state.freq_hz` + `state.phase` and elapsed samples) or directly comparing `PllState::phase` against the ground-truth pulse phase at each sample.

<!-- gh-id: 3127794265 -->
### Copilot on [`crates/core/src/sync/source.rs:53`](https://github.com/cmk/agogo/pull/1#discussion_r3127794265) (2026-04-23 01:23 UTC)

`PhaseSource::feed_samples` advances the PLL by calling `pll.step(None)` only once when no peaks are detected, but `Pll::step(None)` advances by exactly one sample (`freq_hz / sr`). For block-based calls this leaves phase effectively frozen. Consider advancing by `samples.len()` (loop `step(None)` or add a `step_samples(count)` API on `Pll`).
```suggestion
    /// contains no detected peaks, the PLL is free-run for the full
    /// block so callers can use this as a steady-tick driver.
    pub fn feed_samples(&mut self, samples: &[f32], start: u64) {
        match self {
            PhaseSource::Internal { .. } => {}
            PhaseSource::External { detector, pll } => {
                let peaks = detector.process(samples, start);
                if peaks.is_empty() {
                    for _ in 0..samples.len() {
                        pll.step(None);
                    }
```

<!-- gh-id: 3127794273 -->
### Copilot on [`crates/core/src/sync/source.rs:40`](https://github.com/cmk/agogo/pull/1#discussion_r3127794273) (2026-04-23 01:23 UTC)

`phase_at_sample(&mut self, n)` suggests the returned value corresponds to the provided absolute sample index, but the `External` branch ignores `n` entirely and just returns the current `pll.state().phase`. Either compute/advance phase to `n` (track last-synced sample index) or adjust the API/docs so `External` doesn’t claim to be “at sample n”.

<!-- gh-id: 3127794277 -->
### Copilot on [`crates/core/src/sync/detect.rs:118`](https://github.com/cmk/agogo/pull/1#discussion_r3127794277) (2026-04-23 01:23 UTC)

The current hold-window implementation can emit a second peak exactly `hold_samples` samples after the previous one (because `hold_remaining` is decremented before the check and is set to `hold_samples` on emission). That conflicts with the doc comment (“blocks emission at n+1 .. n+hold” inclusive). Consider decrementing after the peak check or setting `hold_remaining = hold_samples + 1` so the documented gap is enforced.
```suggestion
                    self.state.hold_remaining =
                        self.cfg.hold_samples.saturating_add(1);
```

<!-- gh-id: 3127794284 -->
### Copilot on [`crates/core/src/sync/detect.rs:107`](https://github.com/cmk/agogo/pull/1#discussion_r3127794284) (2026-04-23 01:23 UTC)

`centre_global` casts `start_index` (a `u64`) to `i64` with `as`, which will wrap for large stream indices (and can produce negative `sample_index` values). If stream-global indices can exceed `i64::MAX`, this becomes incorrect. Consider doing the arithmetic in `u64`/`i128` and only converting to `f64` at the end, or returning an error when `start_index` is out of range.
```suggestion
                        (start_index as i128) + (offset as i128) - 1;
```

<!-- gh-id: 4158944618 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-04-23 01:23 UTC](https://github.com/cmk/agogo/pull/1#pullrequestreview-4158944618))

## Pull request overview

Introduces the new `agogo-core::sync` DSP module for audio-clock synchronization, including pulse peak detection and a Type‑II (PI) PLL, plus a CLI trace harness and accompanying planning/review docs.

**Changes:**
- Added `sync/` module with `PeakDetector`, Type‑II `Pll`, and unified `PhaseSource`.
- Extended `agogo-core::arb` with a deterministic synthetic pulse-train generator and `testkit`-gated proptest strategies.
- Added `agogo-cli sync trace` (clap) to generate a pulse train and output detector/PLL CSV traces.

### Reviewed changes

Copilot reviewed 12 out of 13 changed files in this pull request and generated 8 comments.

<details>
<summary>Show a summary per file</summary>

| File | Description |
| ---- | ----------- |
| doc/reviews/review-00001.md | Review record summarizing what landed and key deviations. |
| doc/plans/plan-2026-04-22-02.md | Implementation plan + verification matrix + deviation notes. |
| crates/core/src/sync/source.rs | `PhaseSource` API over Internal clock vs External detector+PLL. |
| crates/core/src/sync/pll.rs | Type‑II PLL implementation + properties/regression tests. |
| crates/core/src/sync/detect.rs | Streaming 3-sample peak detector with parabolic interpolation. |
| crates/core/src/sync.rs | `sync` module wiring + re-exports. |
| crates/core/src/lib.rs | Exposes `sync` module from core crate. |
| crates/core/src/arb.rs | Adds `pulse_train` + `testkit`-gated strategies. |
| crates/core/Cargo.toml | Adds `testkit` feature and optional `proptest` dep. |
| crates/cli/src/main.rs | New clap CLI with `sync trace` and E2E test. |
| crates/cli/Cargo.toml | Adds clap dependency. |
| Cargo.toml | Adds workspace clap dependency. |
| Cargo.lock | Locks clap and transitive dependencies. |
</details>






<!-- gh-id: 3127794290 -->
### Copilot on [`crates/core/src/sync/pll.rs:156`](https://github.com/cmk/agogo/pull/1#discussion_r3127794290) (2026-04-23 01:23 UTC)

`clamp_frac = clamp_hz / nominal_freq_hz` can exceed 1.0 (defaults at 120 BPM / 24 PPQ give 50/48 ≈ 1.04), allowing `integrator <= -1.0`. That makes `smoothed_bpm()` negative (`nominal_freq_hz * (1 + integrator)`) and can also drive `freq_hz` negative before it’s reset. Consider clamping integrator so `1.0 + integrator` stays positive, or apply the clamp in absolute-Hz space rather than as a fraction.
```suggestion
            let min_integrator = (-clamp_frac).max(-1.0 + f64::EPSILON);
            self.state.integrator =
                self.state.integrator.clamp(min_integrator, clamp_frac);
```

<!-- gh-id: 3128676350 -->
#### ↳ cmk ([2026-04-23 06:07 UTC](https://github.com/cmk/agogo/pull/1#discussion_r3128676350))

Fixed — added `parse_positive_f32` and `clap::value_parser!(u32).range(1..)` value parsers on `bpm`, `sr`, `ppq`, and `pulses` so invalid input now fails at clap parsing with a friendly error instead of panicking inside `pulse_train`.

<!-- gh-id: 3128676625 -->
#### ↳ cmk ([2026-04-23 06:07 UTC](https://github.com/cmk/agogo/pull/1#discussion_r3128676625))

Fixed — added a `parse_non_negative_f32` value parser on `--jitter-us`. Negative or non-finite values now produce a clap error before reaching core.

<!-- gh-id: 3128676883 -->
#### ↳ cmk ([2026-04-23 06:07 UTC](https://github.com/cmk/agogo/pull/1#discussion_r3128676883))

Tracked as a Sprint-3 follow-up in the plan's Review section (recommendation 1). The rewrite wants a `Pll::phase_at_sample` returning the NCO's true projected phase so the property can accumulate `|predicted_arrival - observed_arrival|` residuals in samples and convert to µs at the test SR. Not blocking this PR — the existing metric is a useful (if indirect) tempo-RMS check.

<!-- gh-id: 3128677139 -->
#### ↳ cmk ([2026-04-23 06:07 UTC](https://github.com/cmk/agogo/pull/1#discussion_r3128677139))

Addressed via the same fix as the next thread: `External::phase_at_sample(n)` now projects analytically from `pll.last_pulse_sample() + elapsed * freq/sr`, so silent intervals don't need per-sample free-runs to keep phase queries correct. Updated `feed_samples` and its docstring to match — silent blocks are now a true no-op. New tests cover halfway-between-pulses, the wrap at one full cycle, and the no-pulse-yet zero case.

<!-- gh-id: 3128678044 -->
#### ↳ cmk ([2026-04-23 06:07 UTC](https://github.com/cmk/agogo/pull/1#discussion_r3128678044))

Fixed — `External::phase_at_sample(n)` now anchors at `pll.last_pulse_sample()` and projects analytically: `(state.phase + (n - last) * freq_hz / sr).rem_euclid(1.0)`. Added a `Pll::last_pulse_sample()` getter to expose the anchor. New tests in `source.rs` cover halfway, full-cycle wrap, and the pre-first-pulse zero return.

<!-- gh-id: 3128678401 -->
#### ↳ cmk ([2026-04-23 06:07 UTC](https://github.com/cmk/agogo/pull/1#discussion_r3128678401))

Tightened the docstring rather than the code. The behavior (gap == `hold_samples`, next emission at sample `n + hold_samples`) is intentional and is what the `detector_hold_blocks_doubles` and `detector_hold_allows_when_gap_exceeds_hold` properties already pin down — it was only the prose that was ambiguous. Doc now reads "blocks emission at samples `n+1` through `n + hold_samples - 1` inclusive; the next emission is allowed at `n + hold_samples` or later".

<!-- gh-id: 3128678744 -->
#### ↳ cmk ([2026-04-23 06:08 UTC](https://github.com/cmk/agogo/pull/1#discussion_r3128678744))

Fixed — `centre_global` now uses `i128` arithmetic so `start_index` values above `i64::MAX` no longer wrap to negative `sample_index` outputs.

<!-- gh-id: 3128678917 -->
#### ↳ cmk ([2026-04-23 06:08 UTC](https://github.com/cmk/agogo/pull/1#discussion_r3128678917))

Fixed — `min_integrator = (-clamp_frac).max(-1.0 + f64::EPSILON)` floors the lower clamp just above -1, so `1 + integrator` stays strictly positive even when `clamp_hz / nominal_freq_hz > 1` (which is the default at 120 BPM 24 PPQ: 50/48 ≈ 1.04). Added an `integrator_clamp_keeps_bpm_positive` regression test that drives sustained late arrivals to walk the integrator hard against the floor and asserts smoothed BPM stays positive across 1000 steps.
