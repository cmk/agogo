# PR #29 — Q1a: Vendor decimal/sample types + bump connections rev

## Summary

Vendors the staged `decimal.rs` and `sample.rs` modules from
`doc/notes/` into `crates/core/src/time/`, bumps the
`connections` pin to current `origin/main` HEAD (`6c88862`),
and stands up backward-compat aliases at `agogo_core::fxp` so
every existing call site keeps compiling. **No behaviour change.
No workspace-wide rename. No Conn-discipline fixes.**

This is step Q1a of a four-PR sweep ([plan]
(../plans/plan-2026-04-27-01.md)) executing audit P0a + P0b.
Q1b will land the workspace rename `Uni..Pico → FD00..FD12`
and `S44..S96 → S044..S096` (preserving `Micro`/`Pico`/`Nano`
as intentional domain aliases at FFI seams). Q2 closes audit
findings M + N (Conn-discipline sweep). Q3 closes K + L (float
surface area).

### Why the rev bump now

- Connections deleted `conn::sample` (publish-prep T10) and the
  `conn::std::i64::decimal` ladder (sprint/remove-decimal,
  merged to main). The CHANGELOG rationale: *"Domain-specific
  numeric ladders belong in downstream crates ([agogo](
  https://gitlab.com/cmk/agogo) for audio); this crate ships the
  algebra plus the per-host-crate cast families."* agogo is the
  intended downstream owner.
- The `compose!` macro and the `conn::std::{i8..u128}` integer
  narrowing/non-widening Conns landed upstream. Q2 needs these
  to rewrite `f64_bpm_to_tempo` and friends without open-coded
  unit arithmetic.

### What's in this PR

1. **Vendor `time/decimal.rs`** — `FD00..FD12` decimal SI rungs,
   21 intra-decimal `Conn`s, 7 `F064FD??` float→fixed bridges.
   Full Galois law battery via macro.

2. **Vendor `time/sample.rs`** — `Q48_16` alias, `SampleRate`
   trait, six rate newtypes `S044..S192`, 15 rate↔rate `Conn`s
   (6 integer-ratio + 9 rational-ratio with Galois
   `floor_div`/`ceil_div` adjoints), 6 `FD12↔Sxxx`
   pico→sample bridges. Full Galois law battery.

3. **Vendor `time/arb.rs`** — twelve proptest strategies
   (`fixed_*`, `extended_fdNN`, `rate_*`, `pico_*`) extracted
   verbatim from `connections @ d1ac1ead:src/property/arb.rs`.
   Connections deleted these alongside the type families they
   served; vendoring them keeps the staged law batteries green.

4. **Bump connections rev** — `d1ac1ead → 6c888626` in
   workspace `Cargo.toml`.

5. **Drop upstream re-exports + add transitional aliases** at
   `crates/core/src/fxp.rs`. Drops
   `pub use connections::conn::fixed::*` and
   `conn::sample::*` (those module paths no longer exist
   upstream). Adds `pub use crate::time::{decimal::*,
   sample::*}` under canonical 4-character names, plus a
   transitional alias module for `Uni..Pico` and `S44..S96`
   so every existing call site compiles unchanged. Q1b will
   drop all aliases except the three intentional KEEPs
   (`Micro`, `Pico`, `Nano` at FFI seams).

6. **Vendor a local `Ple` trait** at
   `crates/core/src/preorder.rs`. Connections deleted
   `lattice::Ple` in commit `9aa426b` (the lawful framework
   now uses standard `Eq + PartialOrd` directly). Agogo has
   73 `.ple(&x)` call sites, including `Grid`'s load-bearing
   **divisibility preorder** which is *not* the natural order
   on its component fields. Vendoring the one-method trait
   locally preserves the divisibility semantics without
   touching every call site.

7. **`ExtendedFloat::Finite` → `ExtendedFloat::Extend`
   rename** at six call sites. Upstream API rename in lockstep
   with the rev bump.

8. **Three `connections::conn::fixed::*` import re-routes**
   through `crate::fxp::{Micro, Pico}` (channel/role.rs,
   channel/scheduler.rs, channel/transform.rs, time/conn.rs
   test). The upstream paths no longer exist.

9. **Allowlist `time/{decimal,sample}.rs` in
   `scripts/check-floats.sh`** — the `F064FD?? float_conn!`
   macro body is intrinsically f64-internal (it walks the
   f64-mantissa precision plateau to make ceil/floor exact);
   the same files were upstream-allowlisted for the same
   reason.

### Verification

- `cargo test --workspace` — 941 pass; 0 fail; 2 ignored
  (pre-existing).
- `cargo clippy --all-targets -- -D warnings` — green.
- `cargo doc --workspace --no-deps` — green except for
  pre-existing unresolved-link warnings unrelated to this PR.
- `cargo check --workspace --all-targets --all-features` —
  green.
- `scripts/check-floats.sh` — green.
- `scripts/check-pii.sh` — green.
- `cargo run -p agogo-cli -- --bpm 120 --ch dev=midi,grid=t32t,out=default --probe`
  produces the same probe output as before the rev bump.

### Out of scope (this PR)

- Workspace rename (Q1b).
- Conn-discipline sweep — `f64_bpm_to_tempo`, `micro_from_ms`,
  the three `(args.bpm * 1.0e6).round()` duplicates, `Tempo
  abs_diff`, `.0 as f64 / 1.0e6` reverse-direction (Q2).
- Float surface area — `ChannelSpec.delay_ms`,
  `RunArgs.{bpm, link_quantum}` (Q3).

## Local review (2026-04-27)

**Branch:** plan/2026-04-27-01
**Commits:** 3 (origin/main..plan/2026-04-27-01)
**Reviewer:** Claude (sonnet, independent)

---

### Commit Hygiene

Three commits, each atomic and correctly prefixed (`plan:`, `feat(core):`, `doc:`). The single implementation commit is large (2,410 lines) but justifiably so — it is mechanically inseparable: vendoring the two modules plus their arb strategies, rewiring imports, and bumping the rev all break each other if split. Commit messages are conventional and within 72 characters. No merge commits. No issues here.

### Code Quality

**`#[allow(unused_imports)] use ExtendedFloat as _;` in `time/arb.rs:303-305`**

This is a code smell worth understanding. `ExtendedFloat` is imported at the module level because the comment says it would otherwise be flagged unused, but no function in `arb.rs` actually constructs `ExtendedFloat`. The real fix is to delete the `use connections::conn::float::ExtendedFloat;` import. Confidence: 80. Test-only file with no functional consequence, but will confuse Q1b authors.

**Plan Verification table paragraph is stale**

`plan-2026-04-27-01.md` lines 338-344 say arb strategies "live in `connections::property::arb` and continue to ship in connections post-decimal-removal." This is false — they were deleted upstream and are now vendored in `crates/core/src/time/arb.rs`. The paragraph contradicts what was implemented. Confidence: 85.

**`impl_sample_time!` macro instantiations use old alias names (`S44`, `S48`, `S88`, `S96`)**

`crates/core/src/fxp.rs` lines 134-139 invoke `impl_sample_time!(S44)`, etc. Works today via the aliases, but T5 explicitly says to use the canonical `S044`, `S048`, etc. Using transitional alias names inside the file that defines those aliases creates a forward-dependency: when Q1b removes the aliases, these four macro calls will break as collateral rather than being swept by the grep rename. Confidence: 85.

**`check-floats.sh`: CLAUDE.md not updated to reflect new allowlist entries**

CLAUDE.md states "the script encodes fourteen exception modules." The updated `scripts/check-floats.sh` now lists 16 files. CLAUDE.md count is wrong; script header (line 26) explicitly requires CLAUDE.md and the script stay in sync. Confidence: 90.

**`preorder.rs` — vendoring a one-method trait: rationale is compelling.** `Grid`'s divisibility preorder is genuinely not the natural order on its fields. The U7/U4 case is documented as trait-bound uniformity for laws. No issue.

**`fxp.rs` alias section — three KEEPs vs. eighteen transitional aliases.** Structure is appropriate for a staged migration. The alias block is 50 lines, manageable. No issue.

### Test Coverage

All verification table entries are satisfied: 21 decimal Galois batteries, 15 sample-rate Galois batteries, 6 FD12↔Sxxx batteries, 7 `F064FD??` float batteries, all spot-check tests present. Generator domains follow CLAUDE.md anti-pattern rule (full-domain for non-closure properties, bounded variant for closure). The `roundtrip_*_integer_ratio` runtime guard pattern is documented and acceptable.

### Plan Conformance

T1-T6 + T2.5 all implemented as specified. Out-of-plan additions (Ple trait, ExtendedFloat::Finite→Extend rename, three import re-routes, allowlist update) are documented in the plan's Review section. No unplanned scope creep beyond what's acknowledged.

### Risks

**Workspace members unchanged.** `host-cpal`, `host-link`, `host-midi` remain excluded but pin `agogo-core` via path dep. The `ExtendedFloat::Extend` rename was correctly applied to `host-link/src/link.rs`.

**New transitive dependencies.** `Cargo.lock` shows `half`, `proptest`, `time` (0.3.45), `time-core`, `deranged`, `num-conv`, `powerfmt` as new entries. The `time` crate is connections's `F064DURN`/`F032DURN` Duration-bridge dep. Run `cargo deny check` before merge. Confidence: 82.

No TODOs or stubs in vendored code. No unsafe. No stored f32/f64 outside allowlisted modules.

### Recommendations

**Must fix before push:**

1. **CLAUDE.md float-exception count is wrong.** Update "fourteen exception modules" → "sixteen" and list the two new files (`crates/core/src/time/decimal.rs`, `crates/core/src/time/sample.rs`).

2. **`impl_sample_time!` should use canonical names.** Change lines 134-137 of `crates/core/src/fxp.rs` from `S44/S48/S88/S96` to `S044/S048/S088/S096`. Same file, same place — but Q1b's grep sweep won't catch alias-shape uses inside the alias definition file.

**Follow-up (future work):**

1. Remove the dead `ExtendedFloat` import + the `as _` suppression from `time/arb.rs`. Track for Q1b cleanup.
2. Stale paragraph in plan-2026-04-27-01.md Verification section (says arb still lives upstream — it doesn't anymore). Note for Q1b's plan doc.
3. Confirm `time` crate (0.3.45) passes `cargo deny check`.
