# PR #32 — Q3: Float surface-area sweep (audit findings K + L)

## Summary

Final PR of the four-PR Conn-discipline + float-surface sweep
([plan](../plans/plan-2026-04-27-04.md)). Eliminates the last
public-surface `f64` storage positions:

- **K — `ChannelSpec.delay_ms: f64 → ChannelSpec.delay: Micro`**
- **L — `RunArgs.bpm: f64 → Tempo` + `RunArgs.link_quantum:
  Option<f64> → Option<Quantum>`**

Plus the deferred Q2 follow-up: tightens the `Bot/Top` arms in
`tempo_to_f64_bpm` and `pico_to_f64_seconds` from silent
`f64::INFINITY` fallback to `unreachable!()`. The Q2-added full-
domain proptests prove the assertion holds across `any::<u32>()`
/ `any::<i64>()`.

After Q3, every `f64` in the workspace lives inside one of the
five documented exception classes (PI, PCM ABI, ABI-local,
argv-handler-body, Link FFI). No `f64` persists on a public type;
none cross the parser → into_channel boundary.

### What's in this PR

1. **`crates/cli/src/run.rs`** — two new bpaf parser fns
   (`parse_bpm_to_tempo`, `parse_quantum_from_beats`) live inside
   `mod run` (gated behind `--features run`); `RunArgs.bpm`
   becomes `Tempo`; `RunArgs.link_quantum` becomes
   `Option<Quantum>`. Sheds the `f64_bpm_to_tempo(args.bpm)` call
   at the handler entry (args.bpm is already typed). Sheds the
   manual `f64_beats_to_quantum(quantum_beats)` + finite-check
   in the link arm — collapses to
   `args.link_quantum.unwrap_or(Quantum::from_bars(4))`.

2. **`crates/core/src/machine/spec.rs`** —
   `delay_ms: f64 → delay: Micro`. Parser tries an i64-ms exact
   path first (no f64 round-trip drift), falls back to f64 +
   `micro_from_ms` only for fractional inputs. `into_channel`
   sheds the `micro_from_ms` call (just clamps to MAX_DELAY).
   `Display` formats integer ms as `delay=N`, fractional ms as
   `delay=N.fff` (parser-stable). `arb_spec` generator yields
   `Micro(n × 1_000)` for `n ∈ [0, 300]`.

3. **`crates/core/src/fxp.rs`** — `tempo_to_f64_bpm` and
   `pico_to_f64_seconds` Bot/Top arms become `unreachable!()`
   with explicit assertion messages.

4. **`crates/core/proptest-regressions/machine/spec.txt`** —
   committed regression seed at `delay: FD06(51000)` pinning the
   integer-ms exact-parse contract against future "let's just
   call micro_from_ms" attempts.

### The integer-ms exact-parse subtlety

`micro_from_ms(51.0)` returns `Micro(51_001)`, not `Micro(51_000)`,
because the f64 representation of `0.051` overshoots `51_000 /
10⁶` by ~5 × 10⁻¹², and `F064FD06.ceil` faithfully rounds the
overshoot up. The original `delay_ms: f64` never noticed because
the spec stored the f64 verbatim and did the conversion only
once on the `into_channel` output path. Once delay is stored as
`Micro`, every parse-then-Display-then-reparse cycle goes through
that ceil and drifts by 1 µs per round-trip.

Fix: parser tries `i64`-ms parse first. `delay=51` parses
exactly to `Micro(51_000)`; `delay=10.5` still goes through f64
+ `micro_from_ms`. The spec_round_trip proptest auto-captured
the failing seed at the 51 ms shrink — committed as a permanent
regression gate.

### Verification

- `cargo test --workspace` — 945 pass; 0 fail; 2 ignored
  (pre-existing).
- `cargo clippy --all-targets -- -D warnings` — green.
- `cargo check --workspace --all-targets --all-features` — green.
- `scripts/check-floats.sh` — green.
- `cargo build -p agogo-cli --features link` — green.
- `cargo build -p agogo-cli --features cpal` — green.
- The auto-saved `spec_round_trip` seed at `delay: FD06(51000)`
  is the regression gate for the integer-ms exact-parse contract.

### Out of scope (this PR)

- **Subcommand `*Args` structs** (`channel_trace::TraceArgs`,
  `midi_trace::TraceArgs`, `demo::DemoArgs`) still expose
  `bpm: f64`. Migrating them requires parallel updates to the
  four bpaf-derived enum variants that feed them
  (`SyncSub::Trace`, `ChannelSub::Trace`, `MidiSub::Trace`,
  `DemoSub::*`) plus their inner-module function signatures —
  a separate small sprint. The transitional `parse_cli_bpm`
  helper introduced in Q2 stays in main.rs to serve them.
- **`host-link/src/link.rs:262`** (samples → microseconds via
  `× 10⁶ / Hz`) — different shape (rate-aware, not a ladder
  rung); deferred since Q2. Probably wants a
  `samples_to_micro_at_rate` helper as part of v0.4 audio output
  planning.

### Audit progress (after Q3)

The four-PR sweep closes findings A–L modulo the documented
deferrals:

| Finding | Status |
|---------|--------|
| A | closed by P3 (PR #26) + P4 (PR #27) |
| B | closed by P3 |
| C | closed by P3 |
| D | closed by P1 (PR #22) |
| E | deferred (low marginal value) |
| F | closed by P3 |
| G | deferred to v0.4 |
| H | partially closed by P5 + Q2 (M5 sites) |
| I | partially closed by P2 + P5 |
| J | closed by P2 (PR #25) |
| K | **closed by Q3 (this PR)** |
| L | **closed by Q3 (this PR) for RunArgs; subcommand `*Args` deferred** |
| M (M1-M7) | mostly closed by Q2 (PR #31); user-unit-shifts documented as exceptions; FFI-parity rounding documented as exception |
| N (N1-N5) | mostly closed by Q2; N2 was a misdiagnosis |

## Local review (2026-04-27)

**Branch:** plan/2026-04-27-04
**Commits:** 3 (origin/main..plan/2026-04-27-04)
**Reviewer:** Claude (sonnet, independent)

---

### Commit Hygiene
Three commits (plan/refactor/doc), conventional, atomic.

### Code Quality
All four planned changes implemented correctly. Bpaf parsers correctly placed inside `mod run`. Integer-ms exact-parse fallback uses `checked_mul` for overflow safety. The `unreachable!()` arms are appropriate given the full-domain proptests prove they hold.

### Test Coverage
`spec_round_trip` proptest catches the integer-ms drift; new regression seed pinned at `delay: FD06(51000)`.

### Plan Conformance
T1-T5 all completed, with three documented deviations (Plan's Review section).

### Risks
`unreachable!()` panics if upstream Conn drift; acceptable given proptest coverage. `args.link_quantum.unwrap_or(Quantum::from_bars(4))` equivalent to old `f64_beats_to_quantum(4.0)`.

### Must fix before push

1. **Display loses sign for `Micro(-500)`** — `delay=0.500` instead of `delay=-0.500`. Currently dormant (clamped before output) but Display should be total. **Addressed in this round** — added explicit `ms_int == 0 && us < 0` guard with comment.

### Follow-up (future work)

1. ~~`parse_bpm_to_tempo` / `parse_quantum_from_beats` lack proptests.~~ **Addressed in this round** — added 11 unit tests + 2 proptests (`parse_bpm_to_tempo_ok_iff_in_range`, `parse_quantum_from_beats_ok_iff_in_range`) covering the validation contract over `prop::num::f64::ANY`.
2. Subcommand `*Args` migration (channel_trace/midi_trace/demo) — already in plan's Recommendations.

### Bug discovered while writing the proptests

`parse_bpm_to_tempo("5000")` returned `Ok(Tempo(u32::MAX))` instead of `Err(out of range)`. Root cause: `tempo_to_f64_bpm(Tempo(u32::MAX))` does NOT return `~4294.967295` — it returns `~9.22 × 10¹²`. The `I064U032.inner(u32::MAX)` step saturates u32::MAX up to `i64::MAX` (per the upstream Conn contract), so the F-ladder inverse computes `i64::MAX / 10⁶`. Using `tempo_to_f64_bpm(Tempo(u32::MAX))` as "max valid BPM" is semantically wrong.

Fix: added `Tempo::MAX_BPM_F64: f64 = (u32::MAX as f64) / 1_000_000.0` const in `agogo_core::fxp` and use it from both bpaf parsers in `run.rs` and the legacy `parse_cli_bpm` in `main.rs`. Documented inline why this open-coded `× 10⁻⁶` is a one-off domain-boundary constant rather than the M-family violation it superficially looks like.

The new `parse_bpm_to_tempo_ok_iff_in_range` proptest catches this — without it, the bug would have shipped silently.
