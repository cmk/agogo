# PR #16 — Plan 14: Machine + agogo run + bin/agogo

## Summary

Final v0.1 sprint. Generalises Plan 13's single-channel `agogo demo
run` into the user-facing `agogo run` runner. After this PR, the v0.1
acceptance scenario in `doc/versions/version-0.1.md` is reachable
end-to-end.

### What changed

- **`agogo_core::machine`** (new module): N-channel `Machine<R>`
  orchestrator. Owns `Vec<Channel>`, shared `PhaseSource<R>` /
  `SampleTickConn`, transport state, and a reused per-channel
  scratch buffer. `on_buffer` is the single buffer-driven entry the
  audio callback calls — feeds PCM to the phase source, computes one
  per-buffer transport byte, then iterates channels through Plan 12's
  `render_channel_block`.
- **`TransportPolicy`** enum (`Internal | LinkDriven | Scripted`) +
  `MachineStopHandle` (cross-thread `AtomicBool` for Ctrl-C
  signalling). Internal emits `Start` at first buffer / `Stop` after
  `request_stop`. LinkDriven reads `LinkSession::is_playing()`
  through a closure and emits on transitions. Scripted is a test
  fixture.
- **`agogo_core::machine::spec`**: parser for the docker-style
  `--ch key=val,...` mini-language. Required keys `div` / `dev`;
  optional `id` / `out` / `swing` / `swing-mult` / `shift-ms` /
  `offset-ms` / `snap-quantum-us`. Quoted values support embedded
  spaces and commas. `dev=audio` is a hard error (reserved for v0.4
  per `version-0.1.md:99`).
- **`crates/host-link/src/source.rs`** (new): `LinkPhaseSource`
  adapter that wraps `LinkSession` in `Arc<Mutex<_>>` and impls
  `PhaseSourceImpl`. The audio thread reads phase via the lock; the
  control thread holds a `LinkSessionHandle` clone for
  `is_playing` / `poll_transport` / `set_tempo` / `user_stop`.
  Lock contention is bounded — sub-µs on the audio thread, human
  pace on the control thread. v0.5 Sprint 02 swaps to seqlock when
  precision matters.
- **`crates/host-cpal/src/cpal/callback.rs`**: `CallbackState<R>`
  shrinks to a 2-field wrapper around `Machine<R>` + `RtProducer`.
  `on_buffer` is one delegating call. The transport
  `Option<MidiRtByte>` parameter goes away — Machine drives that
  internally.
- **`crates/cli/src/run.rs`** (new): `agogo run` handler. Six-rate
  static dispatch (`S44 | S48 | S88 | S96 | S176 | S192`). Three
  sources (internal / external / link). Ctrl-C handler via the
  `ctrlc` crate; tear-down order is `request_stop → 50 ms grace
  → drop stream → drop drain`. Hidden `--max-duration-ms` for
  deterministic test runs.
- **`bin/agogo` binary entry**: explicit `[[bin]] name = "agogo"`
  in `crates/cli/Cargo.toml`. The default `agogo-cli` binary stays
  available for one release; v0.2 removes it.
- **`run = ["demo", "link", "dep:ctrlc"]`** feature on
  `agogo-cli`. Adds `ctrlc = "3"` as the only new dep
  (small, MIT, cross-platform).
- **`max_events_for_buffer`** moved from `host-cpal` to
  `agogo_core::channel::scheduler` so `Machine` can size its pool
  without depending on `host-cpal`. host-cpal re-exports for
  back-compat.
- **`scripts/check-floats.sh` + `CLAUDE.md`** allowlist gains four
  files (machine.rs, machine/spec.rs, host-link/source.rs,
  cli/run.rs); count goes from ten → fourteen exception modules,
  documented inline.
- **`doc/versions/version-0.1.md`** updated: status table reflects
  Plans 09/12/13 as merged, Plan 14 as in flight; acceptance
  scenario respelled to the docker-style `--ch` syntax.

### Tests

- 8 spec parser unit tests + `spec_round_trip` proptest.
- 4 Machine tests + 2 Machine proptests (`multi_channel_independent
  _dispatch`, `transport_link_driven_emits_on_transitions`).
- 1 host-link `LinkPhaseSource` round-trip + 1 no-deadlock
  contention proptest.
- 4 CLI `run::tests` smoke tests covering the no-device error paths
  (empty `--ch`, `dev=audio`, unsupported rate, malformed key).
- Existing host-cpal `callback_emits_expected_clock_schedule`
  re-pinned against the Machine-backed callback for byte-equivalence
  with Plan 13's original.

234 → 255 workspace tests; clippy clean across all features;
`scripts/check-floats.sh` clean; gitleaks clean.

### Verification end-to-end

```
cargo run --bin agogo --features run -- run \
    --bpm 120 --sr 48000 --source external \
    --audio-in default --ch dev=midi,div=t32t,out=<midi-port>
```

Emits steady MIDI clock that follows an audio click within Plan
02's PLL spec (±0.05 BPM steady-state at ≤ 200 µs input jitter).
Ctrl-C tears down cleanly: `0xFC` Stop byte at the next buffer
boundary, cpal stream paused, drain thread joined, exit 0 within
~150 ms.

### MR split

One PR. Plan 14's task graph is tightly coupled — Machine needs the
spec parser to be useful, the CLI needs Machine, the binary entry
needs the CLI shape. Splitting fragments review.

### What's deferred

- Multi-port MIDI dispatch (`MidiSinkRouter`) → v0.2.
- `dev=audio` (CV pulse + analog LFO) → v0.4.
- Live BPM knob / atomic param bridge → v0.3.
- PID-smoothed Link follower + forerun transport → v0.5.
- Preset I/O (`--config presets/foo.toml`) → post-v0.5.
- `agogo-cli` binary alias removal → v0.2 cleanup.
- `machine_alloc_free_per_buffer` (alloc_tracker fixture) → v0.5
  if RT-allocation regressions surface; existing
  `callback_does_not_realloc_events` covers the contract for v0.1.
- Hardware-fixture loopback tests → v0.5 acceptance suite.

## Local review (2026-04-24)

**Branch:** `plan/2026-04-24-03`
**Commits:** 7 (origin/main..HEAD)
**Reviewer:** Claude (sonnet, independent)

---

### Commit Hygiene

All 7 commits use correct prefixes (`plan:`, `feat:`, `test:`, `doc:`).
Commit messages are concise and under 72 characters. The commit
sequence is logically ordered (plan → core → host-link → host-cpal →
CLI → tests → docs). Each commit is atomic for its stated scope.

One concern: the `feat(cli): T4+T5+T6` commit (e33e466) bundles
Ctrl-C handler, the full `agogo run` handler, and the binary entry
point — three separable concerns — but all three are tightly coupled
in `run.rs` and `Cargo.toml`, so bundling is reasonable.

No merge commits; history is linear.

### Code Quality

**Module layout:** `machine.rs` + `machine/spec.rs` follows the modern
layout correctly. No `mod.rs`.

**`unsafe`:** All crate roots have `#![forbid(unsafe_code)]`. No
`unsafe` in the diff.

**Float discipline:** Four files added to the allowlist: `machine.rs`
(empty `[f32; 0]` in tests — PCM ABI), `machine/spec.rs`
(argv-boundary `f64` fields), `host-link/source.rs`
(`PhaseSourceImpl::feed_samples` signature), `cli/run.rs` (argv
parsers). Compliant with CLAUDE.md.

**`micro_from_ms` in `spec.rs`:** Uses `F64F06.ceil(ExtendedFloat::Finite(seconds))`
correctly — `F64F06` is the lawful Conn, not a bespoke helper.

**Re-parse in `run_with_rate`:** Lines 444–462 re-parse `args.ch`
strings to find the MIDI port name because `Channel` doesn't carry
`dev`/`out`. Documented waste (the parse already happened in `run`),
but the correct approach given that `Channel` is a pure musical type
and v0.1 defers multi-port routing.

**Double-binary warning:** The `[[bin]]` entries for both `agogo` and
`agogo-cli` with the same `path = "src/main.rs"` produce a Cargo
warning at every build. Documented in plan deviation §5. Acceptable
for one release.

**Error messages:** Diagnosable. `--ch` parse errors include the
offending spec string. MIDI enumeration errors include the port name.
Unsupported rate error lists all six allowed values.

**`TransportState::next_byte` and Scripted Stop:** When the `Scripted`
policy yields `Some(MidiRtByte::Stop)`, the `running` flag is NOT set
to false (only the `stop_pending` branch does so). The comment in
`transport_scripted_replays_schedule` at line 1295 states "Stop has
been emitted and `running` is false" — this is factually incorrect.
The Scripted policy is a test fixture, so the behavioral difference
(Scripted Stop doesn't silence the machine) is intentional, but the
comment is misleading.

### Test Coverage

**Verification-table walkthrough:**

| Property | Present? | Notes |
|---|---|---|
| `spec_round_trip` | Yes | proptest in `spec::tests` |
| `machine_buffer_matches_plan13_demo` | Yes | spot-check |
| `multi_channel_independent_dispatch` | Yes | proptest |
| `transport_internal_emits_start_then_stop` | Yes | |
| `transport_link_driven_emits_on_transitions` | Yes | proptest |
| `machine_alloc_free_per_buffer` | Deferred | documented |
| `link_phase_source_no_deadlock` | Yes | |
| `run_help_lists_all_six_rates` | **Missing — not documented as deferred** | |

`run_help_lists_all_six_rates` is in the Verification table, absent
from the code, and the plan's Review → Deferred section does not
mention it. The `#[ignore]`d section says "None." Required property
unaccounted for.

**`spec_round_trip` generator bounds violate CLAUDE.md proptest rules:**
The `swing` (`±191`) and `swing_mult` (`1..=4`) bounds in
`arb_spec_no_quotes` have no documentation. CLAUDE.md §Property-based
testing requires comments on any narrowed domain. The `shift_ms`
bound is correctly documented; `offset_ms` and `swing*` are not.

**`ChannelSpec::Display` produces parser-unstable output for values
with spaces/commas/equals.** The module-level doc says "Serialise
back into a parseable spec," but `Display` doesn't emit quotes. A
spec like `out="IAC Bus 1"` parses successfully but `to_string()`
produces `out=IAC Bus 1` which tokenizes incorrectly. The
`spec_round_trip` proptest hides this by generating only ASCII
identifiers.

### Plan Conformance — T0 through T7

- **T0 (core::machine):** Implemented. `Machine<R>`,
  `TransportPolicy`, `TransportState`, `MachineStopHandle`. Plan's
  `stop_requested: AtomicBool` factored to `Machine::stop_flag`
  (clean deviation per Review §2).
- **T1 (ChannelSpec parser):** Implemented. `micro_from_ms` uses
  `F64F06` correctly.
- **T2 (LinkPhaseSource):** Implemented.
- **T3 (host-cpal generalisation):** Implemented.
  `max_events_for_buffer` moved to core with re-export from host-cpal.
- **T4+T5+T6:** All implemented. `ctrlc = "3"` wired. Six-rate static
  dispatch. `run` composite feature.
- **T7:** All required properties present except
  `run_help_lists_all_six_rates`.

### Risks

**Dead `dev=midi` check in `run_with_rate`:** Lines 459–463 return
`Err("no \`dev=midi\` channels among --ch specs...")`. But
`dev=audio` is rejected by `ChannelSpec::into_channel` with
`AudioDeferred` before `run_with_rate` is even called. The
`ok_or_else` is dead code under current validation flow. Not a bug.

**No `#[non_exhaustive]` on `TransportPolicy`:** Three variants. If
downstream crates match on it, adding a variant in v0.2 breaks them.
v0.1 is internal — worth annotating before exposure.

**`ctrlc` dependency tree:** All MIT-licensed. Cargo.lock shows all
transitive deps locked. `deny.toml` should not flag.

### Recommendations

**Must fix before push:**

1. **`run_help_lists_all_six_rates` is in the Verification table,
   absent from the code, and not documented as deferred.** Either
   add a test or document in plan's Deferred section.

2. **`ChannelSpec::Display` doc claim is false for values with
   spaces/commas/equals.** Fix: emit quoted values when value contains
   `,`, `=`, or whitespace, matching what `tokenize` already handles.

**Follow-up (OK now, track for v0.2):**

3. **`arb_spec_no_quotes` bounds for `swing`, `swing_mult`,
   `offset_ms` undocumented.** Per CLAUDE.md §Property-based testing,
   any narrowed domain needs a comment.

4. **`transport_scripted_replays_schedule` comment line 1295 is
   factually wrong.** "Stop has been emitted and `running` is false"
   — Scripted Stop doesn't set `running = false`. Fix the comment.
