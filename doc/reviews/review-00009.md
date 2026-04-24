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
