# PR #35 — Reorg fxp / time / sync; delete fxp; clean host-link FFI containment

## Summary

Workspace organizational cleanup. The `crates/core/src/fxp.rs` kitchen
sink (837 lines doing seven jobs) is **deleted outright**, and its
contents migrate to their proper homes. Two adjacent files
(`time/conn.rs`, `time/decimal.rs`) shrink to single-concern shapes,
plus a small fix to `host-link` keeps Link-FFI floats inside CLAUDE.md
exception 5's "1–2 lines of the FFI call" window.

This PR ships the **core reorg** half of [Plan
2026-04-28-03](../plans/plan-2026-04-28-03.md). The borrow tasks
(T6–T8 — `LpfPid`, `TransportState`, `RelativeClock`) and the
audit-driven kitchen-sink splits (T9 `machine/spec`, T11 `cli/main`)
are deferred to follow-up plans; this PR is already meaty.

### What moves where

| Source | Destination | Why |
|---|---|---|
| `fxp.rs` `Phase` (Q0.32 NCO state) | `crate::sync::phase` | NCO controller state — wrapping_add IS the controller math |
| `fxp.rs` `Tempo` (BPM × 10⁶) | `crate::time::tempo` | musical-time noun: BPM is a rate over time, pairs with Tick / Time / Sxxx |
| `fxp.rs` `Quantum` + `f64_beats_to_quantum` + `parse_quantum_from_beats` | `agogo_host_link::quantum` | Link-FFI only — pulled out of `core` entirely |
| `fxp.rs` `SampleTime` trait + `samples_f64` | `crate::time::sample` | rate-typed; lives over the `Sxxx` family |
| `fxp.rs` `SampleTickConn` (was in `time::conn`) | `crate::sync::sample_tick` | tempo-coupled — violated `time/`'s "no tempo coupling" invariant |
| `time::conn` SampleTickConn tests | `sync::sample_tick::tests` | follow the type |
| `time::exact_rates` (200 lines of `#[cfg(test)] mod tests`) | `sync::sample_tick::tests::exactness` | misnamed + misplaced; folds into the type's test block |
| `time::decimal` `float_conn!` macro + 7 `F064FDxx` Conns | `time::float` (new) | qualitatively different from integer `fix_fix!` — separate proof obligations |
| `fxp.rs` f64 boundary helpers (`f64_*`, `tempo_to_*`, `pico_to_*`, `bits_q48_16_*`, `pico_to_samples`) | `crate::boundary` (new) | the f64↔fxp seam, separate from type definitions |
| `fxp.rs` ramps (`linear_u8`, `smoothstep_u8`) | `crate::time::envelope` | envelope curves, not arithmetic primitives |
| `fxp.rs` `Pico` / `Micro` domain aliases | `crate::time::decimal` | with the FD types they alias |

### Other changes

- **`time/exact_rates.rs` deleted.** 200 lines of test material masquerading as a top-level module file; folded into `sync::sample_tick::tests::exactness`. The misleading name (suggested audio-rate exactness in general; was specifically `SampleTickConn::inner` rounding) disappears with it.
- **`machine::spec::snap_intent()` signature changes** `Option<Quantum>` → `Option<Micro>`. `core` should not produce host-link-shaped types; the orchestrator wraps the raw microbeat count into `Quantum` at the host-link boundary.
- **`finite_or_unreachable` helper** in `boundary` dedupes the two `match ExtendedFloat::Extend(_) => x, Bot|Top => unreachable!()` patterns previously open-coded in `tempo_to_f64_bpm` and `pico_to_f64_seconds`.
- **`host-link::next_quantum_boundary_us` inlined** into `snap_offset_micro` (T10). The helper put `(current_beat / q_f64).ceil() * q_f64` 4–6 lines away from each FFI call, violating CLAUDE.md exception 5. Each FFI call now sits adjacent to its f64-domain math with an explicit `// Link FFI` marker.
- **`host-link::source.rs::feed_samples`** gains the missing `// PCM ABI` annotation on its `&[f32]` parameter (file already allowlisted in `scripts/check-floats.sh`, but the reviewer-facing marker was missing).
- **`time::float` re-exports `Extended` / `ExtendedFloat`** so downstream `cli` / `host-link` can reach them through `agogo_core::time::float::*` without a direct `connections` dependency.

### Why no transitional shim

The library is pre-v0.1; nothing public depends on it. ~30 import
sites across `core`, `cli`, `host-link`, `host-cpal` get rewritten in
the same commit that does the move (T5). Build stays green;
`grep -rn "agogo_core::fxp\|crate::fxp" crates/` returns 0 live
references after the rewrite (only doc-comment historical citations).

### Verification

| Check | Result |
|---|---|
| `cargo build --workspace --all-features` | green |
| `cargo test --workspace --all-features` | 939 + 39 + 39 + 1 = 1018 (core + cli + cli + doc), 2 ignored, 0 failed |
| `cargo test -p agogo-host-link --features rusty-link` | 31 + 4 = 35, 0 failed |
| `cargo test -p agogo-host-cpal` | 10, 0 failed |
| `cargo test -p agogo-host-midi` | 2, 0 failed |
| `time.rs`'s "no tempo coupling" invariant | restored (SampleTickConn moved to `sync/`) |
| `fxp.rs` exists | no |

### What's deferred

- **T6 `LpfPid`** — the `clocked` rolling-avg + LERP'd PI controller wrapper. Adds new behaviour; deserves its own review.
- **T7 `TransportState<S>`** — typestate skeleton for v0.5 transport FSM.
- **T8 `RelativeClock`** — calibration helper for future MIDI input.
- **T9 `machine/spec.rs` split** (1299-line kitchen sink → 4 files).
- **T11 CLI `main.rs` extraction** (1727 lines → 7 sibling modules).
- **T12 small stragglers** (`MidiRtByte::Continue` `#[allow(dead_code)]`, `micro_from_ms` rename, `channel.rs` re-export hub audit).

These ride along in follow-up plans; the deferred section of
plan-2026-04-28-03.md tracks them.
