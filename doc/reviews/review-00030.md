# PR #30 — Q1b: Workspace rename to canonical FD / S0xx names

## Summary

Drops the transitional aliases that Q1a (PR #29) put in place
and migrates every workspace call site to the canonical
4-character names from the vendored `time::decimal` and
`time::sample` modules. Two intentional **domain aliases**
survive at the `agogo_core::fxp` re-export layer (`Micro = FD06`,
`Pico = FD12`) — both read as time-unit words at the FFI seams
and have 20+ / 10+ call sites that would be less readable as
bare `FD06(...)` / `FD12(...)`.

This is step Q1b of the four-PR sweep ([plan]
(../plans/plan-2026-04-27-02.md)) executing audit P0a + P0b.
Q2 closes audit findings M + N (Conn-discipline sweep). Q3
closes K + L (float surface area).

### Why now

Q1a stood up the new canonical names alongside transitional
aliases so the rev bump landed without a workspace-wide rename
in the same diff (one PR, one concern). Q1b is the mechanical
follow-through. Pure refactor — zero behaviour change, all 941
tests from Q1a pass identically.

### What's in this PR

1. **Sweep renames across 14 files** via `git grep -l --null
   ... | xargs -0 perl -pi -e` with word-boundary regex:
   - `F12S44/F12S48/F12S88/F12S96/F12S176/F12S192` → `FD12S044/048/088/096/176/192`
   - `F12F00/F12F03/F12F06/F12F09` → `FD12FD00/03/06/09`
   - `F64F00..F64F12` → `F064FD00..F064FD12`
   - `S44/S48/S88/S96` → `S044/S048/S088/S096`

2. **Drop five unused alias declarations** at
   `crates/core/src/fxp.rs` — `Uni`, `Deci`, `Centi`, `Milli`,
   `Nano`. The `Nano` KEEP from the meta-plan was speculative;
   the Q1b audit confirmed zero callers (jitter math in
   `sync::pll` uses `Pico`, not `Nano`).

3. **Keep two domain aliases** with `KEEP` doc comments
   explaining the rationale:
   - `Micro = FD06` (host-link, channel::scheduler, Quantum,
     ChannelCommon::{delay, offset})
   - `Pico = FD12` (pico_to_samples, cpal seam, arb::pulse_train,
     sync::pll jitter math)

4. **Module docstring rewrites**:
   - `crates/core/src/fxp.rs` — updated to document the canonical
     names + the two KEEPs; dropped references to the upstream
     `connections::conn::{fixed, sample}` paths (those modules no
     longer exist post-Q1a).
   - `crates/core/src/time/decimal.rs` — stripped the
     `(Uni, 1 s)` style alias annotations from each FD rung,
     kept the `(1 s)` time-unit annotation.

### Verification

- `cargo test --workspace` — 941 pass; 0 fail; 2 ignored
  (pre-existing).
- `cargo clippy --all-targets -- -D warnings` — green.
- `cargo check --workspace --all-targets --all-features` — green.
- `scripts/check-floats.sh` — green.
- `cargo build -p agogo-cli --features link` — green
  (host-link picks up the new names through the `crate::fxp`
  re-export surface).
- `cargo build -p agogo-cli --features cpal` — green
  (host-cpal same).
- `git grep -wE 'S44|S48|S88|S96|F12F0?|F12S|F64F|Uni|Deci|Centi|Milli|Nano'`
  in the `crates/` tree returns empty (rename complete; the two
  KEEPs use different identifiers).

### Out of scope (this PR)

- Conn-discipline sweep (`f64_bpm_to_tempo` / `micro_from_ms` /
  `tempo_to_hz` rewrites; `Tempo::abs_diff`; `.0 as f64 / 1.0e6`
  reverse-direction casts) — Q2.
- Float surface area (`ChannelSpec.delay_ms`, `RunArgs.{bpm,
  link_quantum}`) — Q3.
