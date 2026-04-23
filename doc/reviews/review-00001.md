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
