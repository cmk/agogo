# PR #31 — Q2: Conn-discipline sweep (audit findings M + N)

## Summary

Closes most of the audit's M-family (open-coded `× 1.0e±N` unit
arithmetic) and N-family (lossy `as` casts on fixed-point types)
findings. Adds four named helpers (`Tempo::abs_diff`,
`tempo_to_f64_bpm`, `pico_to_f64_seconds`,
`SampleTime::samples_f64`), rewrites several conversion bodies to
compose lawful Conns, and sweeps ~25 sites across 8 files. Three
findings turned out to be **not Conn-composable** and are
documented as bounded exceptions rather than broken into unsafe
rewrites — see "Audit findings re-scoped" below.

This is step Q2 of the four-PR sweep ([plan]
(../plans/plan-2026-04-27-03.md)). Q3 closes K + L (float
surface area) next.

### New helpers in `agogo_core::fxp`

- **`Tempo::abs_diff`** — `|self - other|` as `u32` via
  `u32::abs_diff`. Replaces 5 PLL sites that hand-coded
  `(a.0 as i64 - b.0 as i64).unsigned_abs()` (closes N3).

- **`tempo_to_f64_bpm`** — `Tempo` (u32 microBPM) → f64 BPM via
  the lawful `F064FD06.inner ∘ I064U032.inner` composition.
  Replaces 7 reverse-direction call sites (M5, M6, parts of N1)
  in CLI display code, host-link FFI, and arb fixtures.

- **`pico_to_f64_seconds`** — `Pico` → f64 seconds via
  `F064FD12.inner`. Replaces 2 arb-fixture sites.

- **`SampleTime::samples_f64`** — Q48.16 sample position → f64
  fractional samples (binary scale, intrinsic to representation).
  Replaces 3 detect.rs sites (closes part of N4).

### Bodies rewritten with lawful Conn composition

- `Pico(jitter_us as i64 * 1_000_000)` → `FD12FD06.inner(FD06(...))`
  in 2 PLL sites (closes M7).
- detect.rs:252,290 — inline `tempo_to_hz` body (`bpm.0 as f64 /
  1.0e6 * ppq / 60`) → call `tempo_to_hz(bpm, ppq)` (closes N1).

### CLI cleanup

- 3 duplicate `(args.bpm * 1.0e6).round()` blocks (10 lines each)
  collapse to a `parse_cli_bpm()` helper that wraps
  `f64_bpm_to_tempo` with the "explicit Err on out-of-range" CLI
  contract. The helper uses `tempo_to_f64_bpm(Tempo(u32::MAX))`
  for the upper-bound message — no open-coded scale factor.
  (Closes M4 + N5; the helper itself is transitional, Q3 will
  obsolete it via bpaf parser.)
- 2 `Tempo`-import-only declarations cleaned up after the
  collapse.

### Audit findings re-scoped (documented exceptions, not rewrites)

Three findings turned out to be **not Conn-composable**:

1. **`F064FDxx` interprets f64 as canonical seconds, not as the
   rung's unit.** So the audit's "rewrite `micro_from_ms` to
   compose `F064FD03(ms)`" is wrong: that interprets a
   millisecond input as seconds. The leading `× 10⁻³` (or
   `× 10⁻⁶` for µs) is a user-unit-to-canonical-seconds shift
   with no Conn equivalent — the F-ladder is rooted in seconds.
   M1, M2, and the `cli/main.rs` µs-jitter site keep their user-
   unit shifts with new "argv-boundary user-unit-to-canonical-
   seconds shift" annotations per CLAUDE.md exception 4.

2. **Round-half-away-from-zero (Link FFI requirement) is not a
   Galois adjoint.** `F064FD06.ceil` rounds up;
   `F064FD06.floor` rounds down. Neither matches Link's C++
   `std::llround`, which `f64_beats_to_quantum` and
   `f64_bpm_to_tempo` must agree with bit-exactly at the FFI
   seam. Both bodies keep `(x * 1_000_000.0).round()` with new
   "FFI-parity exception" annotations. The auto-captured
   proptest seed for `q = 944307.2541834672` is now committed
   as a regression gate against future "let's just switch to
   ceil" attempts. The `i64 → u32` narrowing IS lawful and
   goes through `I064U032.ceil` (the saturating-cast piece of
   N5).

3. **N2 (scheduler.rs `.0 as i64`) was a misdiagnosis.** The
   audit assumed `stc.floor(swung_lo).0` was a `SampleTime`
   Q48.16 unwrap. It's a `Tick` (u32) → i64 lossless widening
   — `Tick` has no `to_bits_q48_16` method (wrapping u32
   counter, not Q48.16). Not a Conn-discipline violation.

### Verification

- `cargo test --workspace` — 941 pass; 0 fail; 2 ignored
  (pre-existing).
- `cargo clippy --all-targets -- -D warnings` — green.
- `cargo check --workspace --all-targets --all-features` — green.
- `scripts/check-floats.sh` — green.
- `cargo build -p agogo-cli --features link` — green.
- `cargo build -p agogo-cli --features cpal` — green.
- `f64qnt_matches_link_beats` proptest passes with the
  newly-saved regression seed (round-half-away-from-zero
  contract pinned).

### Out of scope (this PR)

- Q3 — Float surface area (`ChannelSpec.delay_ms: f64 → FD06`,
  `RunArgs.{bpm: f64 → Tempo, link_quantum: Option<f64> →
  Option<Quantum>}`). Will obsolete the `parse_cli_bpm` helper
  introduced here.
- `host-link/src/link.rs:262` — samples → microseconds via
  `× 10⁶ / Hz`. Different shape than M1-M7 (rate-aware, not a
  ladder rung). Surfaced by the Q2 grep but not in original
  audit. Defer.

## Local review (2026-04-27)

**Branch:** plan/2026-04-27-03
**Commits:** 3 (origin/main..plan/2026-04-27-03)
**Reviewer:** Claude (sonnet, independent)

---

### Commit Hygiene
Three commits (plan/refactor/doc), conventional, atomic, each green.

### Code Quality
- All four new helpers correctly named, well-documented, and at the right level of abstraction.
- `Tempo::abs_diff`, `tempo_to_f64_bpm`, `pico_to_f64_seconds`, `SampleTime::samples_f64`: composition correct, `Bot/Top` arms documented as unreachable.
- `parse_cli_bpm` boundary message uses `tempo_to_f64_bpm(Tempo(u32::MAX)) ≈ 4294.967295` correctly; unifies the inconsistent `(0, X]` vs `(0, X)` brackets into one consistent `(0, max_bpm]`.
- FFI-parity exception comments (`f64_beats_to_quantum`, `f64_bpm_to_tempo`) are detailed and explicit; the committed proptest seed for `q = 944307.2541834672` is the right mechanism per CLAUDE.md.
- User-unit-shift exception comments (`micro_from_ms`, `ms_to_micro`, µs jitter) clearly explain the F-ladder-is-rooted-in-seconds constraint.
- Three plan deviations all hold up: (1) `F064FDxx` interprets f64 as seconds, not the rung's unit; (2) round-half-away-from-zero is not a Galois adjoint; (3) N2 misdiagnosis — `Tick.0 as i64` is a benign u32 widening.

### Test Coverage
- Existing 941 tests still pass.
- Auto-saved `f64qnt_matches_link_beats` regression seed correctly committed.
- **One coverage gap flagged:** `tempo_to_f64_bpm` has no direct proptest over `any::<u32>()`. CLAUDE.md mandates property tests for transformers; this helper is now the canonical Tempo→f64 with 7+ call sites.

### Plan Conformance
T1-T5 all completed (with three documented deviations). Plan's Review section accurately captures all deviations.

### Risks
- `Bot/Top` arms in `tempo_to_f64_bpm` and `pico_to_f64_seconds` return silent `f64::INFINITY` on (genuinely unreachable) lift failures. Latent silent-corruption risk if upstream Conn contracts change. Tracked for Q3 ("revisit and tighten" per plan's Review).

### Must fix before push

None.

### Follow-up (future work)

1. ~~Add a `tempo_to_f64_bpm` proptest over `any::<u32>()`.~~ **Addressed in this round** (`tempo_to_f64_bpm_full_domain` proptest, 5-line addition; CLAUDE.md mandates it for transformers).
2. Consider `unreachable!()` for the `Bot/Top` arms in `tempo_to_f64_bpm` / `pico_to_f64_seconds` — defer to Q3 per plan's own Review.
3. ~~`Tempo::abs_diff` doc count "four" → "five" sites.~~ **Addressed in this round** (1-word doc fix).

<!-- gh-id: 3151305631 -->
### Copilot on [`crates/core/src/fxp.rs:288`](https://github.com/cmk/agogo/pull/31#discussion_r3151305631) (2026-04-28 02:59 UTC)

`f64_bpm_to_tempo`’s doc comment says negative/NaN inputs saturate to `ZERO`, but the implementation returns `Tempo::ZERO` for all non-finite inputs (including `+∞`). Consider clarifying the doc to match the actual non-finite handling.

<!-- gh-id: 3151305653 -->
### Copilot on [`crates/core/src/fxp.rs:343`](https://github.com/cmk/agogo/pull/31#discussion_r3151305653) (2026-04-28 02:59 UTC)

`pico_to_f64_seconds` is a new numeric transformer used by multiple call sites, but there’s no direct property test exercising its full `i64` input domain (similar to `tempo_to_f64_bpm_full_domain`). Given CLAUDE.md’s requirement for property tests on transformers, please add a proptest that checks the helper matches an independent reference across `any::<i64>()` (including negative values).

<!-- gh-id: 4185540854 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-04-28 02:59 UTC](https://github.com/cmk/agogo/pull/31#pullrequestreview-4185540854))

## Pull request overview

This PR advances the Q2 audit sweep by replacing a set of open-coded SI scaling and lossy casts with named, “Conn-disciplined” helpers/compositions, and updates downstream call sites (CLI, host-link, fixtures, and tests) to use those helpers. It also records the sprint plan + local review artifacts and pins an additional proptest regression case for Link rounding parity.

**Changes:**
- Add/extend `agogo_core::fxp` helpers (`Tempo::abs_diff`, `tempo_to_f64_bpm`, `pico_to_f64_seconds`, `SampleTime::samples_f64`) and refactor conversions to compose lawful Conns where applicable.
- Sweep call sites in host-link, CLI output/parsing, arb fixtures, and sync tests to remove open-coded `× 1e±N`/casts and call the helpers instead.
- Document/annotate bounded “not Conn-composable” exceptions and commit an additional proptest regression seed for Link rounding parity.

### Reviewed changes

Copilot reviewed 10 out of 10 changed files in this pull request and generated 5 comments.

<details>
<summary>Show a summary per file</summary>

| File | Description |
| ---- | ----------- |
| doc/reviews/review-00031.md | Adds PR review record describing changes, exceptions, and verification. |
| doc/plans/plan-2026-04-27-03.md | Adds Q2 sweep plan doc and post-implementation deviation notes. |
| crates/core/src/fxp.rs | Introduces new helpers + updates conversion logic/tests to use Conn composition where lawful. |
| crates/core/src/machine/spec.rs | Documents argv-boundary user-unit→seconds shift for ms→Micro conversion. |
| crates/core/src/arb.rs | Refactors fixture math to use new Conn-inverse helpers for Tempo/Pico → f64. |
| crates/core/src/sync/detect.rs | Updates tests to use `samples_f64()` and `tempo_to_hz()` instead of open-coded scaling. |
| crates/core/src/sync/pll.rs | Updates tests to use `Tempo::abs_diff` and `FD12FD06.inner(FD06(..))` for jitter. |
| crates/host-link/src/link.rs | Uses `tempo_to_f64_bpm` for the Tempo→f64 BPM Link FFI boundary conversions. |
| crates/cli/src/main.rs | Adds `parse_cli_bpm` helper and replaces repeated BPM scaling blocks + display conversions. |
| crates/core/proptest-regressions/fxp.txt | Adds a saved regression seed for Link rounding parity. |
</details>






<!-- gh-id: 3151305667 -->
### Copilot on [`doc/plans/plan-2026-04-27-03.md:27`](https://github.com/cmk/agogo/pull/31#discussion_r3151305667) (2026-04-28 02:59 UTC)

In the Context bullet list, the parentheticals for `F064FD03`/`F064FD06`/`F064FD12` appear to describe the f64 input as ms/µs/ps, but later in this doc (and the examples in the Review section) indicate these conns interpret f64 as canonical seconds. Consider updating these bullets to reflect the seconds-based convention to avoid a 1000×/1e6× misunderstanding.
```suggestion
- `F064FD03: Conn<ExtendedFloat<f64>, Extended<FD03>>` (f64 seconds → milliseconds)
- `F064FD06: Conn<ExtendedFloat<f64>, Extended<FD06>>` (f64 seconds → microseconds)
- `F064FD12: Conn<ExtendedFloat<f64>, Extended<FD12>>` (f64 seconds → picoseconds)
```

<!-- gh-id: 3151305681 -->
### Copilot on [`doc/plans/plan-2026-04-27-03.md:29`](https://github.com/cmk/agogo/pull/31#discussion_r3151305681) (2026-04-28 02:59 UTC)

The `FD06FD03`/`FD12FD06` bullets describe the conversion as “lossless … ↔ …”, but (per `time/decimal.rs`’s `fix_fix!` macro) only the `.inner` direction is exact; `.ceil`/`.floor` round when going from fine→coarse. Consider rewording these two bullets to avoid implying the round-trip is lossless.
```suggestion
- `FD06FD03: Conn<FD06, FD03>` (`inner` exactly scales ms → µs; fine → coarse uses rounding via `.ceil`/`.floor`)
- `FD12FD06: Conn<FD12, FD06>` (`inner` exactly scales µs → ps by ×10⁶; fine → coarse uses rounding via `.ceil`/`.floor`)
```

<!-- gh-id: 3151305689 -->
### Copilot on [`doc/reviews/review-00031.md:11`](https://github.com/cmk/agogo/pull/31#discussion_r3151305689) (2026-04-28 02:59 UTC)

nit: Summary says “Adds three named helpers”, but this review later lists four new `agogo_core::fxp` helpers/methods (including `SampleTime::samples_f64`). Consider adjusting the summary wording so the helper count matches the detailed list.

<!-- gh-id: 3151325166 -->
#### ↳ cmk ([2026-04-28 03:06 UTC](https://github.com/cmk/agogo/pull/31#discussion_r3151325166))

Fixed in 0824e19 — doc now reads "Non-finite (NaN / ±∞) and non-positive inputs saturate to ZERO" so it matches the impl's `!b.is_finite() || b <= 0.0` guard.

<!-- gh-id: 3151325340 -->
#### ↳ cmk ([2026-04-28 03:06 UTC](https://github.com/cmk/agogo/pull/31#discussion_r3151325340))

Fixed in 0824e19 — added `pico_to_f64_seconds_full_domain` proptest over `any::<i64>()` (mirror of the `tempo_to_f64_bpm_full_domain` proptest from the prior round). Independent reference: `raw / 1.0e12`. Uses a relative tolerance because i64→f64 loses precision for |raw| beyond 2⁵³.

<!-- gh-id: 3151325535 -->
#### ↳ cmk ([2026-04-28 03:06 UTC](https://github.com/cmk/agogo/pull/31#discussion_r3151325535))

Fixed in 0824e19 — bullets now read "f64 seconds → milliseconds / microseconds / picoseconds" so the f64 side is correctly identified as canonical seconds. This is exactly the misreading I made when first writing `micro_from_ms`'s body (the sync_trace_converges test caught it), so worth correcting in the plan to prevent the same trap for future readers.

<!-- gh-id: 3151326229 -->
#### ↳ cmk ([2026-04-28 03:07 UTC](https://github.com/cmk/agogo/pull/31#discussion_r3151326229))

Fixed in 0824e19 — bullets now spell out which direction is exact: `.inner` is the lossless ms→µs / µs→ps embed; `.ceil`/`.floor` round at the rung boundary in the reverse direction.

<!-- gh-id: 3151326389 -->
#### ↳ cmk ([2026-04-28 03:07 UTC](https://github.com/cmk/agogo/pull/31#discussion_r3151326389))

Fixed in 0824e19 — Summary now says "four named helpers" and names them inline (`Tempo::abs_diff`, `tempo_to_f64_bpm`, `pico_to_f64_seconds`, `SampleTime::samples_f64`) so the count and the helper list can't drift apart.
