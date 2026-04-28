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

## Local review (2026-04-29)

**Branch:** plan/2026-04-28-03
**Commits:** 8 (origin/main..plan/2026-04-28-03)
**Reviewer:** Claude (sonnet, independent)

---

### Commit Hygiene

All eight commits use valid prefixes (`plan:`, `debt:`, `fix:`,
`doc:`); messages are conventional and accurate. The `debt:` commits
are pure structural reorgs with no logic changes — `cargo test
--workspace` passes at each commit boundary as a credible
consequence. No merge commits; history is linear.

### Code Quality

**Must-fix (round 1, addressed below): `scripts/check-floats.sh`
allowlist was stale.** Four new files (`crates/core/src/boundary.rs`,
`crates/core/src/time/float.rs`, `crates/core/src/time/tempo.rs`,
`crates/host-link/src/quantum.rs`) contained legitimate `f64` uses
(PI-exempt, vendored-Conn-machinery, FFI-parity) but weren't on the
allowlist; the deleted `crates/core/src/fxp.rs` was still listed.
The CI gate would have failed on push.

**Must-fix (round 1, addressed): `time/decimal.rs` allowlist entry
was now a false-positive stub.** After T1 split `float_conn!` out,
`decimal.rs` contains no live `f64` — only a doc-comment reference
to `ExtendedFloat<f64>` (line 146), which the gate's `//`-prefix
skip already handles. Removed from the allowlist.

**Other quality findings (all clean):**
- `finite_or_unreachable` helper in `boundary.rs` is called at
  exactly two sites (`tempo_to_f64_bpm`, `pico_to_f64_seconds`) —
  intent-preserving dedupe, not a smell.
- `time::float` re-exports `Extended` / `ExtendedFloat` so cli /
  host-link reach them via `agogo_core::time::float::*` without a
  direct `connections` dep. Plan's Review section explicitly
  documents this deviation.
- No bespoke `f64_*_to_*` helpers where a `Conn` would do —
  `f64_beats_to_quantum` and `f64_bpm_to_tempo` document why
  round-half-away-from-zero (matching Link's C++ `std::llround`)
  isn't a Galois adjoint.
- No open-coded unit arithmetic inside Conn-wrapper bodies.
- `grep -rn "agogo_core::fxp\|crate::fxp" crates/` returns 0 live
  references.

### Test Coverage

All moved tests confirmed present and gated correctly:
- T1 (float_conn): full proptest battery moved to `time/float.rs::tests`.
- T3 (SampleTickConn): 16 tests in `sync/sample_tick.rs::tests`,
  including the 8-test `mod exactness` absorbed from
  `time/exact_rates.rs`.
- T4 (Phase): 4 proptests in `sync/phase.rs::tests`. Tempo has no
  type-local tests (boundary tests live alongside the f64 helpers).
  Quantum: 4 tests in `host-link/quantum.rs::tests`.
- T5 (boundary): 8 proptests + spot checks in `boundary.rs::tests`.
- T10 (snap_offset): pre-existing proptests + spot checks survive
  the inline reshape unchanged.

Workspace test counts: 939 (core lib) + 39+39 (cli, two binaries) +
1 (doc) + 31+4 (host-link) + 10 (host-cpal) + 2 (host-midi) = 1065
tests, 0 failed.

### Plan Conformance

T1, T2, T3, T4, T5, T10 all implemented as written. The plan's
explicit deferrals (T6 `LpfPid`, T7 `TransportState`, T8
`RelativeClock`, T9 `machine/spec`, T11 CLI `main`, T12 stragglers)
are correctly out of scope for this PR. The plan's Review section
documents two design deviations (no `compose!`/`ceiling1` body
cleanups in T5; `Extended`/`ExtendedFloat` re-exported from
`time::float`) — both visible in the diff.

### Risks

**Resolved (round 1):** `Tempo::MAX_BPM_F64` was a `pub const f64`
on a non-allowlisted file. Moved to `crate::boundary::MAX_BPM_F64`
(callers updated in `cli/main.rs` and `cli/run.rs`). Keeps
`time/tempo.rs` f64-free.

**Acceptable risks:**
- `snap_intent()` signature change `Option<Quantum>` →
  `Option<Micro>` only affects test code (no production callers
  per plan and grep). Tests updated.
- The `Quantum` cross-crate move from `core` → `host-link` is the
  largest API-surface change but every caller is feature-gated on
  `link` and updates land in this commit.

No new dependencies added. No security concerns (the diff is
structural Rust moves).

### Recommendations

**Must fix before push (round 1, addressed):**

1. ✅ Update `scripts/check-floats.sh` allowlist (add
   `boundary.rs`, `time/float.rs`, `host-link/quantum.rs`; remove
   `fxp.rs`, `time/decimal.rs`). Update CLAUDE.md to match.
2. ✅ Move `Tempo::MAX_BPM_F64` to `boundary::MAX_BPM_F64` so
   `time/tempo.rs` stays f64-free and doesn't need an allowlist
   entry.

**Follow-up (future work):**

3. The `finite_or_unreachable` helper is a candidate for the
   compose!-cleanup follow-up plan once `LinkClock::snap_offset_micro`
   needs a similar one (currently inlines its own match). Potential
   single helper across `boundary.rs` and `host-link::link.rs`.
4. The plan's deferred items (T6–T8 borrow tasks; T9/T11/T12
   kitchen-sink splits) all remain reasonable post-merge follow-ups.
