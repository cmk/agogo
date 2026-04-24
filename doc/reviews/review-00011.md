# PR #11 — Plan 11: boundary sweep + grep gate + rules

## Summary

Completes Plan 11 (post-fxp enforcement continuation of Plan 10 /
PR #9). PR #10 landed the plan doc + upstream rev bump as the
sprint opener; this PR ships the actual migrations + the CI gate
+ the codified rules, on top of that base.

### What ships

- **T2 — `Channel` state flips to `Micro`.** `shift_ms: f32` /
  `offset_ms: f32` / `MAX_SHIFT_MS: f32` become `shift: Micro` /
  `offset: Micro` / `MAX_SHIFT: Micro` (300 000 µs). Both
  `transform` and `scheduler` route through a shared
  `micro_to_samples(Micro, sr) → i64` helper that composes
  upstream `F12F06` (Micro → Pico, exact embed) with
  `PicoSampleConn::ceil` (Pico → Q48.16 samples, lawful Galois)
  and rounds to whole samples. The `ms × sr_f / 1000.0` formulae
  are gone.
- **T3/T4 — CLI argv f64; `ProbeRow` drops stored floats.** All
  bpaf float args are f64 now (`sync trace --bpm`, `--jitter-us`,
  `channel trace --shift-ms`, `--offset-ms`). Each handler
  converts on the first line via a named upstream Conn or helper
  (`f64_bpm_to_tempo`, `F64F06.ceil(ExtendedFloat::Finite(...))`,
  `F64F12.ceil(...)`). The three now-unused `parse_*_f32` helpers
  are deleted. `link_probe::ProbeRow.tempo_bpm: f64` becomes
  `tempo: Tempo`; `phase: f64` becomes `phase: Phase`. The f64
  conversion moves to `println!` time (display-only, f64 dies in
  the format string).
- **T5 — `LinkClock` surface exposes `Tempo`.** `pub fn new(...,
  initial_bpm: f64, ...)` → `initial_bpm: Tempo`. `pub fn
  tempo(&mut self) -> f64` → `-> Tempo`. The AblLink C++ ABI stays
  `f64` but is contained to two `// Link FFI`-commented lines
  inside the impl.
- **T7 — `scripts/check-floats.sh` + CI wire.** New bash script
  walks every `crates/*/src/**/*.rs` and flags any unannotated
  `f32` / `f64` outside seven allowlisted modules (PI controller,
  PCM audio, parabolic-fit, fxp helpers, test fixtures, Link FFI,
  CLI argv). Wired to CI as a `floats` job parallel to `test` /
  `clippy` / `deny`, and into the pre-commit hook so local
  commits that violate the rule fail before reaching CI.
- **T8 — CLAUDE.md + review-calibration.md rules.** New bullets
  under Repository conventions: no-stored-float rule with
  glossary (PI, PI-exempt, ABI-local, argv, Link FFI, PCM ABI)
  and enumerated exception list; every-conversion-is-a-Conn rule;
  compose-don't-hardcode rule with concrete good/bad examples.
  Patterns 9 / 10 / 11 appended to `review-calibration.md` in the
  existing diff/comment/why format.

### Why the stack is shaped this way

T2 is the structural change; T3/T4 consume the new `Channel`
shape; T5 is independent (Link surface); T7 needs the earlier
migrations to actually pass the gate; T8 references the types
T2–T5 introduce. Each commit leaves the repo buildable, green,
and gate-clean on its own.

## Test plan

- [x] `cargo build --workspace` and `cargo build --workspace
  --features link` — both clean.
- [x] `cargo test --workspace --features link` — 235 passed (223
  core + 12 cli/host-link combined); zero failures.
- [x] `cargo test --workspace` (no link feature) — 234 passed.
- [x] `cargo clippy --all-targets --features link -- -D warnings`
  — clean.
- [x] `scripts/check-floats.sh` — OK.
- [x] Negative test on the gate: adding a canary `fn
  _canary(_x: f32)` to a non-allowlisted file produces the
  expected `FAIL` output; restoring the file re-greens it.
- [x] CSV output of `agogo link probe` is character-for-character
  identical to pre-T4 (format preserved; only the intermediate
  type is stricter).
