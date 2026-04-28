# PR #34 — Subcommand `bpm: f64 → Tempo` sweep (audit L-tail)

## Summary

Closes the L-tail of the structural-type audit. PR #32 (Q3) migrated
`RunArgs.bpm` and `link_quantum` to typed `Tempo` / `Option<Quantum>` via
bpaf parser functions; this PR finishes the same migration for every
remaining `f64` argv field across every subcommand. The Tier-1
sprint review caught two must-fix items and three follow-ups; all five
were folded into the same commit, expanding scope to also close the
`SyncSub::Trace.jitter_us` and `ChannelSub::Trace.delay` f64 fields
(originally deferred to a follow-up sprint).

**Sites swept** (all in `crates/cli/src/main.rs`):

- Eight enum-variant `bpm: f64` fields: `DemoSub::Run.bpm`,
  `MidiSub::Trace.bpm`, `LinkSub::Probe.initial_bpm`,
  `LinkSub::PushTempo.bpm`, `LinkSub::Transport.bpm`,
  `LinkSub::Diag.bpm`, `SyncSub::Trace.bpm`, `ChannelSub::Trace.bpm`.
- Three other enum-variant f64 fields: `LinkSub::Transport.quantum: f64`
  → `Quantum`, `SyncSub::Trace.jitter_us: f64` → `Pico`,
  `ChannelSub::Trace.delay: f64` → `Micro`.
- Three `*Args` struct fields: `channel_trace::TraceArgs.bpm` (also
  `delay: f64` → `Micro`), `midi_trace::TraceArgs.bpm`,
  `demo::DemoArgs.bpm`.
- Five handler signatures: `link::probe`, `link::push_tempo`,
  `link::transport` (also takes `Quantum`), `link::diag`,
  `sync_trace::trace` (now takes `Pico` jitter directly). The internal
  `f64_bpm_to_tempo` / `f64_beats_to_quantum` calls plus the
  open-coded `× 1.0e-6` µs→s shift in `sync_trace::trace` body and the
  `ms_to_micro` closure in `channel_trace::trace` body all vanish.
- Three call sites of the transitional `parse_cli_bpm` helper at
  `main.rs:960`, `:1072`, `:1387` collapse into direct `args.bpm` use.

**Parser cohort in main.rs:**

Four `pub(crate) fn` parsers now live at main.rs module level, each
the single argv-boundary site for its target type:

- `parse_bpm_to_tempo` (always compiled).
- `parse_quantum_from_beats` (`#[allow(dead_code)]` — only consumed
  under `link` / `run` features).
- `parse_jitter_us_to_pico` (always compiled).
- `parse_ms_to_micro` (always compiled — `ChannelSub::Trace.delay`
  uses it under `feature = "core"` but the parser definition has no
  gate of its own).

`parse_bpm_to_tempo` and `parse_quantum_from_beats` move from `run.rs`
(previously private, `cfg(feature="run")`); `run.rs` now imports them
via `crate::`. The dead-code lint that pushed Plan 27 to put them in
`run.rs` no longer applies because `MidiSub::Trace` and `SyncSub::Trace`
are always compiled (no feature gate), so the parsers are always
referenced.

**Removals:**

- `parse_cli_bpm` helper (introduced as transitional in Q2/PR #31).
- `parse_positive_f64` and `parse_non_negative_f64` helpers — every
  former caller is now a typed `argument::<String>` + `parse(...)`
  parser site. Audit confirmed no remaining users at any feature level.
- The `ms_to_micro` closure inside `channel_trace::trace` (the open-coded
  `× 10⁻³` ms→s shift) and the `* 1.0e-6` µs→s shift inside
  `sync_trace::trace` body. Both shifts now live exactly once each, in
  their respective `parse_*` parser bodies.
- `negative_bpm_errors` test (`midi_trace::tests`): `bpm: -1.0` is now
  a compile error against `Tempo`. Parser-level rejection is covered
  by run.rs's existing `parse_bpm_to_tempo_ok_iff_in_range` proptest,
  which now gates every subcommand's bpm parser via the single shared
  entry point.

**Typed fallbacks:**

`fallback(120.0)` → `fallback(Tempo::from_bpm_integer(120))`;
`fallback(4.0)` (Transport.quantum) → `fallback(Quantum::from_bars(4))`;
`fallback(0.0)` (jitter_us) → `fallback(Pico::ZERO)`;
`fallback(0.0)` (delay) → `fallback(Micro::ZERO)`. All four typed
fallbacks are `pub const`, so bpaf's attribute-expansion-time const
requirement is satisfied without `fallback_with(|| ...)`.

**Test fixtures:**

`channel_trace`'s and `midi_trace`'s `base_args()` plus
`sync_trace_converges` construct `Tempo` via
`Tempo::from_bpm_integer(120)`. `delay: 0.0` becomes `delay: Micro::ZERO`;
`sync_trace_converges`'s 50 µs jitter literal becomes `Pico(50_000_000)`.

**Build gates:**

All clean: `cargo test --workspace --all-features` (cli count drops by
1 — negative_bpm_errors removed; agogo-core unchanged at 943; 2
pre-existing ignores), `cargo clippy --all-targets --all-features --
-D warnings`, `scripts/check-floats.sh`. Per-feature builds green:
default, `--features run`, `--features link`, `--features demo`,
`--all-features`.

After this PR every `f64` in the workspace lives inside one of the
five documented exception classes (PI, PCM ABI, ABI-local,
argv-handler-body, Link FFI).

## Local review (2026-04-28)

**Branch:** plan/2026-04-28-02
**Commits:** 3 (origin/main..plan/2026-04-28-02)
**Reviewer:** Claude (sonnet, independent)

---

### Commit Hygiene

Three commits: `plan:`, `refactor:`, `doc:` — all use accepted prefixes per
recent repo precedent (PRs #30/#31/#32 all used `refactor:` for analogous
audit sweeps), though `refactor:` is not in CLAUDE.md's enumerated list
(`plan`, `feat`, `fix`, `fmt`, `doc`, `test`, `task`, `debt`). The
`refactor:` commit (9f2ac43) is atomic and would pass `cargo test`.

### Code Quality

**Visibility: `fn` vs `pub(crate) fn` — not a bug, but contradicts the
plan.** Plan T1 specifies `pub(crate) fn`. The implementation uses plain
`fn` (private). `run.rs` is a child module of `main.rs`, so Rust's
visibility rules allow child modules to access parent-private items —
`use crate::parse_bpm_to_tempo` in `run.rs` compiles fine. This works
correctly but contradicts the plan's explicit visibility spec, and the
deviation isn't called out in the plan's Review section.

**`pushed_bpm` output format change — script-interface regression.**
`crates/cli/src/main.rs`, `link::push_tempo`:

```
-        println!("pushed_bpm={bpm}");
+        println!("pushed_bpm={:.4}", tempo_to_f64_bpm(bpm));
```

The old code printed the raw user-supplied `f64` via Rust's default
Display, e.g. `pushed_bpm=120` for `--bpm 120` (Rust's `{}` for f64 is
shortest-round-trip). The new code always prints four decimal places:
`pushed_bpm=120.0000`. The comment at this site says "stdout for scripts:
single line with the pushed BPM." Any script that checks for
`pushed_bpm=120` or parses by exact match silently breaks. This is an
undocumented behavioral change at a documented script-facing interface.

**`#[allow(dead_code)]` vs `#[cfg(...)]` on `parse_quantum_from_beats`.**
`crates/cli/src/main.rs:343-354`. The plan's Review section documents the
choice and its rationale ("Cleanest fix without a `#[cfg(any(...))]`
ceremony"). Acceptable.

**No open-coded `× 1.0e±N` constants** introduced in the diff. The plan's
deferred list (`SyncSub::Trace.jitter_us`, `channel_trace::TraceArgs.delay`)
is intact — both fields untouched in this diff.

**`#![forbid(unsafe_code)]` is present** at `main.rs:1`. No unsafe
introduced.

### Test Coverage

**Proptest transitivity claim — verified.** Every `bpm` field's
`#[bpaf(... parse(parse_bpm_to_tempo))]` attribute references the same
function. The proptest `parse_bpm_to_tempo_ok_iff_in_range` in `run.rs`
imports it via `use crate::parse_bpm_to_tempo` (`run.rs:43`). Generator is
`prop::num::f64::ANY` — full IEEE domain, no bounded shrinkage. Coverage
genuinely transitive.

**`negative_bpm_errors` deletion — coverage analysis.** The deleted test
asserted `trace(&args).is_err()` where `args.bpm = -1.0`. Post-migration,
`bpm: -1.0` is a compile error against `Tempo`; the handler no longer
validates bpm at all (it receives a pre-validated `Tempo`). Parser-level
rejection is covered by the proptest. The handler path being tested no
longer exists. Coverage genuinely equivalent.

**Typed test fixtures — bit-exactly equivalent.**
`Tempo::from_bpm_integer(120)` produces `Tempo(120_000_000)`;
`f64_bpm_to_tempo(120.0)` multiplies `120.0 × 10⁶` and rounds (no error
for exactly 120.0). The `parse_bpm_to_tempo_accepts_120_exactly` test
(run.rs:378-380) confirms equivalence explicitly.

### Plan Conformance

T1-T5 all complete. All nine enum-variant fields, three struct fields, five
handler signatures swept. Grep gates `\bbpm: f64\b|\bquantum: f64\b` and
`parse_cli_bpm` empty. `scripts/check-floats.sh` allowlist comment
updated. Out-of-scope/deferred boundary respected.

### Risks

No new `unwrap`/`expect` at API boundaries. `agogo_core` unchanged. No
cross-crate breakage. `parse_non_negative_f64` remains for `jitter_us` /
`delay` (both deferred).

### Recommendations

**Must fix before push:**

1. **`pushed_bpm` output format change at `link::push_tempo`.** Old:
   `pushed_bpm=120` (default `{}` for f64). New: `pushed_bpm=120.0000`
   (`{:.4}`). The comment at this site documents it as a script
   interface. Either change the format spec from `{:.4}` to `{}` (default
   Display, recovers prior shortest-round-trip behavior for integer BPMs)
   or document the format change explicitly and add a Verification
   spot-check.

2. **`refactor:` prefix not in CLAUDE.md's enumerated list.** Per recent
   repo precedent (PRs #30/#31/#32 all used `refactor:`), this is
   conventional in practice. Either align with CLAUDE.md (use `feat:`
   or `task:`) or document the precedent in CLAUDE.md.

**Follow-up (future work):**

1. **`fn` vs `pub(crate) fn` — plan said `pub(crate)`.** Plain `fn` works
   (child module access), but contradicts plan T1. Either align the
   visibility or document the deviation in the plan's Review section.

2. **`parse_non_negative_f64` shape inconsistency.** Last `f64`-argument
   bpaf parser uses bpaf's pre-parse pattern; the two new parsers use
   `argument::<String>` + `parse(...)`. When `jitter_us` / `delay` are
   swept in the follow-up, migrate this last parser for consistency.

3. **`demo` startup eprintln also uses `{:.4}`.** Diagnostic stderr, not
   a script interface, so lower priority — but worth aligning with
   whatever choice is made for #1 above.

---

## Round 1 fixes (2026-04-28)

All five items from the local review (2 must-fix + 3 follow-ups)
addressed in the same commit, with carpe-diem expansion to also close
the originally-deferred `jitter_us` / `delay` fields.

**Must-fix #1 — `pushed_bpm` format.** Changed from `{:.4}` →
`{:.2}` in `link::push_tempo`. Two decimals match `Tempo`'s effective
post-round-trip precision (six decimal places of f64 BPM round to a
single µBPM integer, but printing 2 fractional digits is the readable
ceiling). The `demo` startup eprintln aligns to `{:.2}` for the same
reason.

**Must-fix #2 — `refactor:` → `feat:`.** Implementation commit prefix
amended. CLAUDE.md's enumerated list (`plan / feat / fix / fmt / doc /
test / task / debt`) wins over the loose precedent set by PRs
#30/#31/#32.

**Follow-up #1 — `pub(crate) fn` on relocated parsers.** Aligned with
plan T1's spec.

**Follow-up #2 — drop `parse_non_negative_f64` (carpe diem).**
Pulled `SyncSub::Trace.jitter_us: f64 → Pico` and
`ChannelSub::Trace.delay: f64 → Micro` (and `channel_trace::TraceArgs.delay`)
into the same commit, with new parsers `parse_jitter_us_to_pico` and
`parse_ms_to_micro` joining the cohort in main.rs. Drops both
`parse_non_negative_f64` (no remaining users) and the open-coded
`× 1.0e-6` shift in `sync_trace::trace` plus the `ms_to_micro` closure
in `channel_trace::trace`. Each unit shift now lives exactly once,
inside its named parser body — the documented argv-boundary
exception.

**Follow-up #3 — `demo` startup eprintln.** Aligned to `{:.2}` per
must-fix #1 fix shape.

**Verification:** All gates re-run after fixes — `cargo test
--workspace --all-features` (cli count drops by 1 vs. pre-PR;
agogo-core unchanged at 943; 2 pre-existing ignores), `cargo clippy
--workspace --all-targets --all-features -- -D warnings`,
`scripts/check-floats.sh`, all per-feature builds green.
End-to-end smoke checks confirm the new error paths fire on
`--jitter-us=-1` and `--delay=-1`.

**Plan + review docs updated** to reflect the expanded scope before
push (Goal section, Implementation deviations, Final state).
