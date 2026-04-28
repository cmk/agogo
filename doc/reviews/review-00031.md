# PR #31 — Q2: Conn-discipline sweep (audit findings M + N)

## Summary

Closes most of the audit's M-family (open-coded `× 1.0e±N` unit
arithmetic) and N-family (lossy `as` casts on fixed-point types)
findings. Adds three named helpers, rewrites several conversion
bodies to compose lawful Conns, and sweeps ~25 sites across 8
files. Three findings turned out to be **not Conn-composable** and
are documented as bounded exceptions rather than broken into
unsafe rewrites — see "Audit findings re-scoped" below.

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
