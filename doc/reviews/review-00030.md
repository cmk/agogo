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
   - `crates/core/src/conn/fixed.rs` — stripped the
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

## Local review (2026-04-27)

**Branch:** plan/2026-04-27-02
**Commits:** 3 (origin/main..plan/2026-04-27-02)
**Reviewer:** Claude (sonnet, independent)

---

### Commit Hygiene

Three commits in the expected order: `plan:` opener, a single `refactor:` sweep commit, and a `doc:` finalizer. Prefixes match CLAUDE.md conventions. Each is described correctly. Clean.

### Code Quality

**Missed call sites — none found.** Every renamed identifier was swept cleanly across all 14 files. Comments and docstrings updated; no stale old name in any renamed context.

**One intentional non-rename worth noting (not a defect).** In `crates/core/src/arb.rs:229`, the private constant `S48_HZ` keeps its old spelling — it is a local constant *name*, not a type alias, and the RHS correctly reads `<crate::fxp::S048 as SampleRate>::HZ`. Word-boundary regex `\bS48\b` doesn't fire because `_` is a word character. No correctness or clarity risk.

**Dropped aliases are genuinely unused.** Diff confirms zero callers for `Uni`, `Deci`, `Centi`, `Milli`, `Nano`.

**KEEP alias rationale comments are clear.** `crates/core/src/fxp.rs:62-69`: each KEEP has a one-line doc comment naming the concrete call sites, plus a section-header policy paragraph. T6 spec satisfied.

**Word-boundary regex spot-check:** Three representative sites verified clean (no `S044` fragment was disturbed by `\bS44\b`; no `S48_HZ` was matched by `\bS48\b`).

### Test Coverage

No tests were deleted. Test counts in `sync/{detect,pll,source}.rs`, `control/transport.rs`, `host-cpal/callback.rs` match pre-rename structure — every test function was renamed at the type-annotation level only. Plan's claim of 941 tests passing identically is consistent with diff content.

### Plan Conformance

T1–T6 all complete. Verification table: all build gates reported green; `git grep` completeness gate empty in `crates/`. Both documented deviations (Nano dropped instead of KEEP; docstrings rewritten) are well-justified and recorded.

### Risks

`host-link` and `host-cpal` (excluded from `[workspace].members`) both build cleanly under `--features link` / `--features cpal`. No string literals were incorrectly touched.

### Must fix before push

None.

### Follow-up (future work)

- `crates/core/src/arb.rs:229`: the private constant `S48_HZ` could be renamed to `S048_HZ` for internal consistency, though it carries no external visibility risk. Low priority; could ride with any future arb.rs touch.
- Q2 recommendations (rewriting `f64_bpm_to_tempo`, `micro_from_ms`, `tempo_to_hz` to compose lawful Conns) are correctly scoped out and tracked in the plan's Review section.

<!-- gh-id: 3151128878 -->
### Copilot on [`crates/cli/src/main.rs:971`](https://github.com/cmk/agogo/pull/30#discussion_r3151128878) (2026-04-28 01:55 UTC)

This comment describes `F064FD06` as an "upstream" conn; after Q1a/Q1b it’s defined in agogo’s vendored `time::decimal` and re-exported via `agogo_core::fxp`. Please update the wording to reflect where the type actually lives now.

<!-- gh-id: 3151128902 -->
### Copilot on [`crates/core/src/arb.rs:231`](https://github.com/cmk/agogo/pull/30#discussion_r3151128902) (2026-04-28 01:55 UTC)

`S48_HZ` now aliases `<crate::fxp::S048 as SampleRate>::HZ`, so the identifier is misleading. Rename the constant (e.g., `S048_HZ`) or make the helper generic so the name matches the actual rate type.

<!-- gh-id: 3151128915 -->
### Copilot on [`crates/core/src/fxp.rs:315`](https://github.com/cmk/agogo/pull/30#discussion_r3151128915) (2026-04-28 01:55 UTC)

The implementation dispatches to `FD12S0xx` conns, but the nearby comments still refer to `F12Sxx` and "upstream". Please update the doc/comments above this match so they reference the canonical `FD12Sxxx` names (and avoid implying these conns still live upstream).

<!-- gh-id: 4185297114 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-04-28 01:55 UTC](https://github.com/cmk/agogo/pull/30#pullrequestreview-4185297114))

## Pull request overview

Removes the Q1a transitional workspace aliases and migrates call sites to the canonical `FDxx` / `S0xx` fixed-point type names, while retaining the two intentional domain aliases (`Micro`, `Pico`) at the `agogo_core::fxp` re-export layer.

**Changes:**
- Sweeps workspace call sites from legacy alias names (`S48`, `F64F06`, etc.) to canonical `S048`, `F064FD06`, `FD12S0xx`, etc.
- Drops unused alias re-exports in `crates/core/src/fxp.rs`, keeping only `Micro = FD06` and `Pico = FD12`.
- Updates module/docs prose to reflect the canonical naming.

### Reviewed changes

Copilot reviewed 16 out of 16 changed files in this pull request and generated 7 comments.

<details>
<summary>Show a summary per file</summary>

| File | Description |
| ---- | ----------- |
| doc/reviews/review-00030.md | Adds PR review record describing the rename sweep and alias removals. |
| doc/plans/plan-2026-04-27-02.md | Adds execution plan/audit notes for Q1b rename + alias cleanup. |
| crates/host-link/src/link.rs | Updates imports/usages to `F064FD06` and associated commentary. |
| crates/host-cpal/src/cpal/callback.rs | Renames `S48` → `S048` in docs/tests and generic instantiations. |
| crates/core/src/conn/fixed.rs | Cleans module doc ladder annotations to match canonical FD names. |
| crates/core/src/control/sync/source.rs | Renames `S48` → `S048` across tests. |
| crates/core/src/control/sync/pll.rs | Renames `S48` → `S048` across tests and helper math. |
| crates/core/src/control/sync/detect.rs | Renames `S48` → `S048` across tests and proptests. |
| crates/core/src/channel/spec.rs | Updates float-bridge conn name `F64F06` → `F064FD06` in docs and code. |
| crates/core/src/control.rs | Renames `S48` → `S048` across machine tests. |
| crates/core/src/fxp.rs | Removes transitional alias surface; keeps `Micro`/`Pico`; updates `pico_to_samples` dispatch to canonical `FD12Sxxx`. |
| crates/core/src/channel/time.rs | Updates conn name `F12F06` → `FD12FD06` in docs and conversion path. |
| crates/core/src/control/event.rs | Updates transform/scheduler composition comment to `FD12FD06`. |
| crates/core/src/arb.rs | Renames `S48` → `S048` in tests and sample-rate sealed helper. |
| crates/cli/src/run.rs | Renames static sample-rate dispatch types to `S044/S048/S088/S096`. |
| crates/cli/src/main.rs | Renames trace/demo paths to `S048` and `F064FD06/F064FD12`; updates related comments. |
</details>






<!-- gh-id: 3151128932 -->
### Copilot on [`doc/plans/plan-2026-04-27-02.md:52`](https://github.com/cmk/agogo/pull/30#discussion_r3151128932) (2026-04-28 01:55 UTC)

The "Affected files (12)" list looks incomplete relative to this PR: e.g. it doesn’t include `crates/host-link/src/link.rs` or `crates/core/src/channel/spec.rs`, both of which are updated for the rename sweep. Also, the hardcoded line range in `crates/core/src/fxp.rs:55-83` (line 28) will be stale once the rename lands; consider referencing the alias block by symbol/snippet instead of line numbers.

<!-- gh-id: 3151128940 -->
### Copilot on [`crates/core/src/fxp.rs:586`](https://github.com/cmk/agogo/pull/30#discussion_r3151128940) (2026-04-28 01:55 UTC)

The test comment still says `pico_to_samples` dispatches to upstream `F12Sxx` conns / proptests, but the code now uses canonical `FD12Sxxx` conns from agogo’s vendored modules. Please update the wording so it matches the current naming/ownership.

<!-- gh-id: 3151128957 -->
### Copilot on [`crates/host-link/src/link.rs:206`](https://github.com/cmk/agogo/pull/30#discussion_r3151128957) (2026-04-28 01:55 UTC)

The comment claims the 10^6 unit shift lives inside `F064FD06`'s definition in the `connections` crate, but `F064FD06` is defined in agogo's vendored `crate::conn::fixed` module (and re-exported via `agogo_core::fxp`). Please update the wording to avoid pointing readers at the wrong crate/module.

<!-- gh-id: 3151128965 -->
### Copilot on [`crates/cli/src/main.rs:879`](https://github.com/cmk/agogo/pull/30#discussion_r3151128965) (2026-04-28 01:55 UTC)

These comments describe `F064FD12` as an "upstream" conn; after Q1a/Q1b it’s defined in agogo’s vendored `time::decimal` and re-exported via `agogo_core::fxp`. Please adjust the wording so readers don’t go looking for this type in upstream crates.

<!-- gh-id: 3151144458 -->
#### ↳ cmk ([2026-04-28 02:00 UTC](https://github.com/cmk/agogo/pull/30#discussion_r3151144458))

Fixed in ab1e2f2 — comment now reads "the lawful F064FD06 conn from agogo_core::conn::fixed (re-exported via agogo_core::fxp)" instead of pointing at upstream.

<!-- gh-id: 3151144818 -->
#### ↳ cmk ([2026-04-28 02:00 UTC](https://github.com/cmk/agogo/pull/30#discussion_r3151144818))

Fixed in ab1e2f2 — renamed S48_HZ → S048_HZ at both the def site and the use site for internal consistency with the canonical S048 type name. The constant is private to arb.rs so the rename has no external visibility risk.

<!-- gh-id: 3151145281 -->
#### ↳ cmk ([2026-04-28 02:01 UTC](https://github.com/cmk/agogo/pull/30#discussion_r3151145281))

Fixed in ab1e2f2 — pico_to_samples docstring now describes the dispatch as "the lawful FD12Sxxx Conn for that rate (defined in crate::conn::sample, re-exported above)" and references the per-rate Galois-law battery at crate::conn::sample::tests instead of pointing at upstream.

<!-- gh-id: 3151145620 -->
#### ↳ cmk ([2026-04-28 02:01 UTC](https://github.com/cmk/agogo/pull/30#discussion_r3151145620))

Fixed in ab1e2f2 — corrected "Affected files (12)" to (14) and added the two missing entries (channel/spec.rs and host-link/link.rs). The brittle `crates/core/src/fxp.rs:55-83` line range is now a symbol-anchored description ("the section between the canonical re-exports and the SampleTime trait declaration") so it doesn't break if the alias block moves.

<!-- gh-id: 3151145926 -->
#### ↳ cmk ([2026-04-28 02:01 UTC](https://github.com/cmk/agogo/pull/30#discussion_r3151145926))

Fixed in ab1e2f2 — pico_to_samples test rationale now reads "the lawful FD12Sxxx conns from crate::conn::sample" with a pointer at the per-rate Galois battery at time::sample::tests::p_fd12s0??, instead of attributing the proptests to upstream.

<!-- gh-id: 3151146494 -->
#### ↳ cmk ([2026-04-28 02:01 UTC](https://github.com/cmk/agogo/pull/30#discussion_r3151146494))

Fixed in ab1e2f2 — snap_offset_micro comment now points at "F064FD06's definition (agogo_core::conn::fixed, re-exported via crate::fxp)" so readers don't go looking for the conn in upstream.

<!-- gh-id: 3151146790 -->
#### ↳ cmk ([2026-04-28 02:01 UTC](https://github.com/cmk/agogo/pull/30#discussion_r3151146790))

Fixed in ab1e2f2 — channel_trace jitter comment now reads "the lawful F064FD12 conn from agogo_core::conn::fixed (re-exported via agogo_core::fxp)" instead of upstream.
