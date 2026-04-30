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

1. **CLAUDE.md float-exception count is wrong.** Update "fourteen exception modules" → "sixteen" and list the two new files (`crates/core/src/conn/fixed.rs`, `crates/core/src/conn/sample.rs`).

2. **`impl_sample_time!` should use canonical names.** Change lines 134-137 of `crates/core/src/fxp.rs` from `S44/S48/S88/S96` to `S044/S048/S088/S096`. Same file, same place — but Q1b's grep sweep won't catch alias-shape uses inside the alias definition file.

**Follow-up (future work):**

1. Remove the dead `ExtendedFloat` import + the `as _` suppression from `time/arb.rs`. Track for Q1b cleanup.
2. Stale paragraph in plan-2026-04-27-01.md Verification section (says arb still lives upstream — it doesn't anymore). Note for Q1b's plan doc.
3. Confirm `time` crate (0.3.45) passes `cargo deny check`.

<!-- gh-id: 3151014104 -->
### Copilot on [`crates/core/src/conn/fixed.rs:93`](https://github.com/cmk/agogo/pull/29#discussion_r3151014104) (2026-04-28 01:13 UTC)

`inner` multiplies `x.0 * PREC` in `i64`, which will overflow and wrap for large coarse values (e.g., `FD12FD00.inner(FD00(i64::MAX))`), contradicting the doc claim that `inner` is an “exact embedding” and potentially breaking Conn laws. Consider using a wider intermediate (e.g. i128) with explicit clamping/saturation, or switching to an `Extended<...>` target/source for out-of-range values.
```suggestion
                let product = i128::from(x.0) * i128::from(PREC);
                let clamped = product.clamp(i128::from(i64::MIN), i128::from(i64::MAX));
                $Fine(clamped as i64)
```

<!-- gh-id: 3151014125 -->
### Copilot on [`crates/core/src/conn/sample.rs:170`](https://github.com/cmk/agogo/pull/29#discussion_r3151014125) (2026-04-28 01:13 UTC)

`rate_conn!` computes intermediate values in `i128` but then casts `bits as i64` when constructing `Q48_16`. For large inputs this can exceed `i64::MAX`/`MIN` (e.g. converting near-max Coarse bits with NUM/DEN > 1), causing wrapping truncation and breaking monotonicity/Galois laws. Please clamp/saturate the computed bits into the `i64` range (or return an extended/saturating type) before calling `from_bits`.

<!-- gh-id: 3151014134 -->
### Copilot on [`crates/core/src/conn/sample.rs:260`](https://github.com/cmk/agogo/pull/29#discussion_r3151014134) (2026-04-28 01:13 UTC)

`pico_conn!` has the same `i128` → `i64` unchecked casts (`bits as i64` / `... as i64`) as `rate_conn!`. Large FD12 or sample values can produce out-of-range `bits`, which will wrap and violate the Conn’s stated rounding/adjoint semantics. Please clamp/saturate prior to constructing `FD12`/`Q48_16` so overflow can’t silently wrap in release builds.
```suggestion
            fn saturating_i128_to_i64(value: i128) -> i64 {
                if value > i64::MAX as i128 {
                    i64::MAX
                } else if value < i64::MIN as i128 {
                    i64::MIN
                } else {
                    value as i64
                }
            }

            fn ceil(p: FD12) -> $Rate {
                let n: i128 = p.0 as i128 * DEN;
                let q = n.div_euclid(NUM);
                let r = n.rem_euclid(NUM);
                let bits = if r != 0 { q + 1 } else { q };
                $Rate(Q48_16::from_bits(saturating_i128_to_i64(bits)))
            }

            fn inner(s: $Rate) -> FD12 {
                let n: i128 = s.0.to_bits() as i128 * NUM;
                FD12(saturating_i128_to_i64(n.div_euclid(DEN)))
            }

            fn floor(p: FD12) -> $Rate {
                let n: i128 = p.0 as i128 * DEN + (DEN - 1);
                $Rate(Q48_16::from_bits(saturating_i128_to_i64(
                    n.div_euclid(NUM),
                )))
```

<!-- gh-id: 3151014144 -->
### Copilot on [`CLAUDE.md:140`](https://github.com/cmk/agogo/pull/29#discussion_r3151014144) (2026-04-28 01:13 UTC)

The float-allowlist sentence opens with a single parenthetical but appears to end with a double-close `))` (“…same reason)). The …”). Please fix the punctuation/parenthesis balance so the prose reads correctly.
```suggestion
  upstream-allowlisted for the same reason). The
```

<!-- gh-id: 3151014161 -->
### Copilot on [`doc/reviews/review-00029.md:177`](https://github.com/cmk/agogo/pull/29#discussion_r3151014161) (2026-04-28 01:13 UTC)

This review record’s “Must fix before push” section appears stale relative to the current PR state (e.g., `CLAUDE.md` count and `impl_sample_time!(S044/...)` are already updated, and the referenced `time/arb.rs` `ExtendedFloat` import doesn’t exist). Please revise the section to either mark these as fixed (with commit refs) or remove them, so the archived review doesn’t contradict the merged code.
```suggestion
None outstanding at merge time.

**Follow-up (future work):**

1. Stale paragraph in plan-2026-04-27-01.md Verification section (says arb still lives upstream — it doesn't anymore). Note for Q1b's plan doc.
2. Confirm `time` crate (0.3.45) passes `cargo deny check`.
```

<!-- gh-id: 3151014169 -->
### Copilot on [`crates/core/src/conn/sample.rs:15`](https://github.com/cmk/agogo/pull/29#discussion_r3151014169) (2026-04-28 01:13 UTC)

The “Integer range” estimate looks off by ~1000×: ±2^47 samples at 48 kHz is about ±93 years (≈2.9e9 s), not ±93,000 years. Please correct the unit conversion so readers don’t overestimate the safe range.
```suggestion
//! - **Integer range**: ±2⁴⁷ samples — at 48 kHz that is about ±93 years.
```

<!-- gh-id: 3151014177 -->
### Copilot on [`doc/plans/plan-2026-04-27-01.md:13`](https://github.com/cmk/agogo/pull/29#discussion_r3151014177) (2026-04-28 01:13 UTC)

`Ple` plan link uses a contributor-local absolute path (`/Users/...`) which won’t resolve for other developers or CI artifacts. Please replace it with a repo-relative document reference or inline the relevant audit context so the plan is self-contained.
```suggestion
P0b. For reference, the sequence is:
```

<!-- gh-id: 4185150046 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-04-28 01:13 UTC](https://github.com/cmk/agogo/pull/29#pullrequestreview-4185150046))

## Pull request overview

This PR vendors the upstream-removed decimal fixed-point ladder and sample-rate time types into `agogo-core`, bumps the pinned `connections` git revision, and adds compatibility re-exports/aliases so existing call sites keep compiling.

**Changes:**
- Vendor `time::{decimal,sample,arb}` into `crates/core/src/time/` and expose them from `agogo_core::fxp`.
- Bump `connections` pin and update call sites for upstream API changes (`ExtendedFloat::Finite → Extend`, removed module paths).
- Add a local `Ple` preorder trait and reroute imports; update float allowlist and related docs.

### Reviewed changes

Copilot reviewed 24 out of 25 changed files in this pull request and generated 10 comments.

<details>
<summary>Show a summary per file</summary>

| File | Description |
| ---- | ----------- |
| scripts/check-floats.sh | Allowlist new vendored float-internal time modules. |
| doc/reviews/review-00029.md | Adds PR review record for this step (Q1a). |
| doc/plans/plan-2026-04-27-01.md | Adds implementation plan + verification checklist for Q1a. |
| crates/host-link/src/link.rs | Update `ExtendedFloat` variant name after rev bump. |
| crates/core/src/time/tick.rs | Switch `Ple` import to local vendored trait. |
| crates/core/src/time/tbase.rs | Switch `Ple` import to local vendored trait. |
| crates/core/src/conn/sample.rs | New vendored sample-rate typed time module + Conns + tests. |
| crates/core/src/time/grid.rs | Remove upstream `Ple` import; use local trait. |
| crates/core/src/conn/fixed.rs | New vendored decimal SI ladder + Conns + float bridges + tests. |
| crates/core/src/time/conn.rs | Reroute `Ple` and fixed-type imports after upstream removals. |
| crates/core/src/time/arb.rs | New vendored time-tier proptest strategies for law batteries. |
| crates/core/src/time.rs | Declare new `decimal`, `sample`, and test-only `arb` modules. |
| crates/core/src/preorder.rs | New local `Ple` trait to preserve divisibility preorder semantics. |
| crates/core/src/conn/midi.rs | Switch `Ple` import to local vendored trait. |
| crates/core/src/channel/spec.rs | Update `ExtendedFloat` variant name after rev bump. |
| crates/core/src/lib.rs | Export new `preorder` module from crate root. |
| crates/core/src/fxp.rs | Drop upstream fixed/sample re-exports; re-export vendored modules + transitional aliases. |
| crates/core/src/channel/time.rs | Reroute fixed-type imports through `crate::fxp`. |
| crates/core/src/control/event.rs | Reroute fixed-type imports through `crate::fxp`. |
| crates/core/src/channel/role.rs | Reroute fixed-type imports through `crate::fxp`. |
| crates/core/Cargo.toml | Enable `connections/testing` for property-law modules in dev-deps. |
| crates/cli/src/main.rs | Update `ExtendedFloat` variant name after rev bump. |
| Cargo.toml | Bump `connections` git rev pin. |
| Cargo.lock | Update lockfile for bumped `connections` and new transitive deps. |
| CLAUDE.md | Update float allowlist documentation to include newly allowlisted files. |
</details>






<!-- gh-id: 3151014186 -->
### Copilot on [`doc/plans/plan-2026-04-27-01.md:343`](https://github.com/cmk/agogo/pull/29#discussion_r3151014186) (2026-04-28 01:13 UTC)

The Verification section claims the arb strategies “live in `connections::property::arb` and continue to ship”, but this PR vendors them into `crates/core/src/time/arb.rs` (and the vendored modules’ tests import from `crate::time::arb`). Please update these lines to match the implemented location/ownership to avoid misleading future readers.
```suggestion
`rate_safe_fine`, `pico_safe`) are vendored in this PR into
`crate::time::arb` (`crates/core/src/time/arb.rs`) — they remain
crate-agnostic generators (i64 ranges with ULP guards), not
```

<!-- gh-id: 3151014189 -->
### Copilot on [`crates/core/src/conn/sample.rs:241`](https://github.com/cmk/agogo/pull/29#discussion_r3151014189) (2026-04-28 01:13 UTC)

The FD12↔rate section comments are internally contradictory about relative “bits/sec” and which side is Fine/Coarse (lines 206–208 vs 221–223). Please correct the explanation to match the actual `Conn<FD12, Sxxx>` definition in this file so readers don’t invert the directionality when using the constants.
```suggestion
// These constants are `Conn<FD12, Sxx>`, so FD12 is Fine and Sxx is Coarse.
// Therefore `inner` goes from samples to picoseconds (`Sxx → FD12`), while
// `ceil` and `floor` go from picoseconds back to samples (`FD12 → Sxx`).
// The reduced ratio satisfies NUM·sample_bit = DEN·pico.

macro_rules! pico_conn {
    ($CONN:ident, $Rate:ident, $num:expr, $den:expr) => {
        pub const $CONN: Conn<FD12, $Rate> = {
            // For `Conn<FD12, Sxx>`, Fine=FD12 and Coarse=Sxx.
            // One Sxx bit spans NUM/DEN picoseconds, so:
            //   inner(s: Sxx) = floor_div(s_bits · NUM, DEN) picoseconds
            //   ceil(p: FD12) = ceil_div(p · DEN, NUM) Sxx-bits
            //   floor(p: FD12) = floor_div(p · DEN + DEN − 1, NUM) Sxx-bits
```

<!-- gh-id: 3151014195 -->
### Copilot on [`crates/core/src/conn/sample.rs:372`](https://github.com/cmk/agogo/pull/29#discussion_r3151014195) (2026-04-28 01:13 UTC)

The test-module comment says the rate strategies live in `crate::property::arb`, but the code imports them from `crate::time::arb`. Please update the comment to the correct module path to avoid confusion when moving/renaming test utilities.
```suggestion
    // in `crate::time::arb`.
```

<!-- gh-id: 3151050688 -->
#### ↳ cmk ([2026-04-28 01:26 UTC](https://github.com/cmk/agogo/pull/29#discussion_r3151050688))

Push back: this is the upstream Conn body, vendored byte-for-byte, and the i64-bounded `inner` is the existing connections contract. The proptest strategies in `time::arb::fixed_coarse(prec)` clamp coarse-side inputs to `i64::MAX / prec` precisely so `inner(c) = c · PREC` stays in range; that bound is the public domain of the Conn. Switching to saturating-i128 inside `inner` would break the strict-embed Galois law (the kernel test `closure_l` expects `ceil(inner(c)) == c`, which requires exact embedding). If we want saturating semantics on this rung, that's a Conn-contract change and belongs upstream first.

<!-- gh-id: 3151050896 -->
#### ↳ cmk ([2026-04-28 01:26 UTC](https://github.com/cmk/agogo/pull/29#discussion_r3151050896))

Push back: same shape as the decimal.rs:93 thread — the macro is vendored verbatim, and `time::arb::rate_coarse(num)`/`rate_fine(den, num)` clamp the generator to the i64-safe domain by design. The `bits as i64` cast is the Conn's documented contract; saturating it would invalidate `monotone_l` at the saturation plateau. If the saturation contract should change, the right place is the upstream macro definition.

<!-- gh-id: 3151051049 -->
#### ↳ cmk ([2026-04-28 01:26 UTC](https://github.com/cmk/agogo/pull/29#discussion_r3151051049))

Push back: identical reasoning to the decimal.rs:93 and sample.rs:170 threads — the `pico_conn!` body is vendored verbatim, and `time::arb::pico_coarse(num, den)` clamps to `i64::MAX · den / num` to keep the i128→i64 cast in range. Saturating inside the macro would diverge from upstream and break the closure law battery on the saturation plateau. Defer the contract change to upstream.

<!-- gh-id: 3151051257 -->
#### ↳ cmk ([2026-04-28 01:26 UTC](https://github.com/cmk/agogo/pull/29#discussion_r3151051257))

Fixed in 4f2e071 — dropped the extra `)` so the "upstream-allowlisted for the same reason" parenthetical balances the outer "the script encodes sixteen exception modules (..." group correctly.

<!-- gh-id: 3151051431 -->
#### ↳ cmk ([2026-04-28 01:26 UTC](https://github.com/cmk/agogo/pull/29#discussion_r3151051431))

That section is the *local* sprint-review record from before push (Tier 1 of the two-tier review workflow); both items were addressed in commit c0bc198 *before* the original push, and the "Must fix" header is the historical record of what the local reviewer flagged. The archived reviews keep the original wording so the audit trail is complete — the suggestion to rewrite it post-hoc would erase that history. The current round's resolution is captured in this PR's reply threads + the 4f2e071 fix commit.

<!-- gh-id: 3151051584 -->
#### ↳ cmk ([2026-04-28 01:27 UTC](https://github.com/cmk/agogo/pull/29#discussion_r3151051584))

Fixed in 4f2e071 — corrected to ±93 years and added the inline arithmetic (`2⁴⁷ / 48 000 ≈ 2.93 × 10⁹ s`) so a future reader can spot-check the unit conversion without recomputing.

<!-- gh-id: 3151051784 -->
#### ↳ cmk ([2026-04-28 01:27 UTC](https://github.com/cmk/agogo/pull/29#discussion_r3151051784))

Fixed in 4f2e071 — replaced the `/Users/...` path with a self-contained four-PR description (Q1a/Q1b/Q2/Q3) so the plan stays reproducible for any reader without depending on a contributor-local file.

<!-- gh-id: 3151051956 -->
#### ↳ cmk ([2026-04-28 01:27 UTC](https://github.com/cmk/agogo/pull/29#discussion_r3151051956))

Fixed in 4f2e071 — rewrote the Verification paragraph to point at the vendored `crate::time::arb` location (matches the implementation), and clarified that `extended_float_f64` is the one strategy that stays upstream.

<!-- gh-id: 3151052126 -->
#### ↳ cmk ([2026-04-28 01:27 UTC](https://github.com/cmk/agogo/pull/29#discussion_r3151052126))

Fixed in 4f2e071 — collapsed the contradictory "FD12 is Coarse" / "FD12 is Fine" comments into a single coherent block. Per the `Conn<FD12, $Rate>` declaration FD12 is Fine (10¹² bits/s), Sxxx is Coarse, so `inner: Coarse → Fine` and `ceil/floor: Fine → Coarse`. The previous "Wait —" passage was a draft remnant.

<!-- gh-id: 3151052244 -->
#### ↳ cmk ([2026-04-28 01:27 UTC](https://github.com/cmk/agogo/pull/29#discussion_r3151052244))

Fixed in 4f2e071 — updated the test-module comment to reference `crate::time::arb` (the vendored path), with a brief note that the strategies were lifted from `connections @ d1ac1ead`'s `property::arb` alongside the type families they generate for.
