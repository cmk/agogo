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
