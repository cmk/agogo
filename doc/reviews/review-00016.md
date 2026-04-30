# PR #16 — Plan 14: Machine + agogo run + bin/agogo

## Summary

Final v0.1 sprint. Generalises Plan 13's single-channel `agogo demo
run` into the user-facing `agogo run` runner. After this PR, the v0.1
acceptance scenario in `doc/versions/version-0.1.md` is reachable
end-to-end.

### What changed

- **`agogo_core::control`** (new module): N-channel `Machine<R>`
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
- **`agogo_core::channel::spec`**: parser for the docker-style
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
  `agogo_core::control::event` so `Machine` can size its pool
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

> **NOTE — historical snapshot.** This section records the
> Tier-1 review state *before* the round-1 fix commit `47ad128
> fix: Address /sprint-review must-fixes + follow-ups`. Both
> must-fix items called out below were resolved in that commit:
> `ChannelSpec::Display` now quotes values with whitespace /
> commas / `=` (regression tests added), and
> `run_help_lists_all_six_rates` is now explicitly documented as
> deferred in the plan's Review section. The commit also
> addressed the two follow-ups (proptest generator-bound docs
> and the `transport_scripted_replays_schedule` comment).

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

<!-- gh-id: 3141397303 -->
### Copilot on [`crates/core/src/control.rs:114`](https://github.com/cmk/agogo/pull/16#discussion_r3141397303) (2026-04-25 04:07 UTC)

The `running` field docs say it becomes `false` after the first `Stop` is emitted “regardless of policy”, but `running` is only flipped in the `stop_pending` (Ctrl-C) path—`LinkDriven` and `Scripted` can emit `Stop` without changing `running`. Please update the comment to reflect the actual semantics (local stop/teardown latch vs. policy-emitted Stop bytes).
```suggestion
/// Wraps [`TransportPolicy`] with a local stop/teardown latch used
/// to suppress further transport-byte emission after a host-driven
/// stop request has been observed.
#[derive(Debug)]
pub struct TransportState {
    pub policy: TransportPolicy,
    /// `true` until a local stop/teardown request is consumed via
    /// `stop_pending`; `false` thereafter.
    /// While `false`, this state machine emits no further transport
    /// bytes and the stream stays clock-only. Policy-generated
    /// `Stop` bytes (for example from `LinkDriven` or `Scripted`) do
    /// not, by themselves, clear this flag.
```

<!-- gh-id: 3141397340 -->
### Copilot on [`crates/core/src/control.rs:169`](https://github.com/cmk/agogo/pull/16#discussion_r3141397340) (2026-04-25 04:07 UTC)

`MachineStopHandle`’s docs claim that after `request_stop()` the callback “falls silent — clock and transport bytes alike”, but `Machine::on_buffer` still schedules/renders per-channel clock events after emitting `MidiRtByte::Stop` (and `TransportState` docs earlier even say the stream becomes clock-only). Please reconcile the documentation with the intended behavior (either update the docs, or gate clock emission after a stop request if silence is required).

<!-- gh-id: 3141397365 -->
### Copilot on [`doc/versions/version-0.1.md:19`](https://github.com/cmk/agogo/pull/16#discussion_r3141397365) (2026-04-25 04:07 UTC)

This status summary contradicts the rest of the page: it says “All v0.1 sprints have shipped” and that the acceptance scenario is already reachable end-to-end, but Plan 14 is still listed under “In flight” below. Please reword to reflect whether Plan 14 is shipped or still pending (and keep the acceptance claim consistent with that).
```suggestion
Most v0.1 sprints have shipped. Plan 14 (Machine + `agogo run` +
`bin/agogo`) is still the final in-flight piece; once it lands, the
v0.1 acceptance scenario below will be reachable end-to-end.
```

<!-- gh-id: 3141397377 -->
### Copilot on [`doc/reviews/review-00016.md:213`](https://github.com/cmk/agogo/pull/16#discussion_r3141397377) (2026-04-25 04:07 UTC)

This review record contains several now-stale factual claims that contradict the current code in this PR (e.g., it says `ChannelSpec::Display` doesn’t quote values and breaks round-tripping, says the `spec_round_trip` bounds are undocumented, and says `run_help_lists_all_six_rates` is missing/not deferred). If this doc is meant to describe the final PR state, it should be updated to note these items as resolved (or moved to a historical “initial findings” section with follow-up).

<!-- gh-id: 3141397389 -->
### Copilot on [`doc/plans/plan-2026-04-24-03.md:473`](https://github.com/cmk/agogo/pull/16#discussion_r3141397389) (2026-04-25 04:07 UTC)

The Verification table still lists `run_help_lists_all_six_rates` under “Properties (must pass)”, but the Review section later in this same doc says that property was deferred/replaced. Please make the table consistent (e.g., remove it from “must pass”, mark it as deferred, or update the invariant/test plan).
```suggestion
| `run_help_lists_all_six_rates` | `agogo_cli::tests::run_smoke` | Deferred/replaced in review: no longer a must-pass property for this plan revision. |
```

<!-- gh-id: 3141397406 -->
### Copilot on [`crates/host-link/src/source.rs:59`](https://github.com/cmk/agogo/pull/16#discussion_r3141397406) (2026-04-25 04:07 UTC)

In `phase_at_sample`'s comment, “Lock contention is uncontested” looks like a typo; the intended term is “uncontended”.
```suggestion
        // Lock acquisition is uncontended on the audio thread because
```

<!-- gh-id: 4174961233 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-04-25 04:07 UTC](https://github.com/cmk/agogo/pull/16#pullrequestreview-4174961233))

## Pull request overview

Implements Plan 14’s v0.1 “end-to-end runner” by introducing an N-channel `Machine` orchestrator, wiring Ableton Link as a `PhaseSource`, and adding a user-facing `agogo run` command plus a top-level `agogo` binary entry.

**Changes:**
- Add `agogo_core::control::{Machine, TransportPolicy, ChannelSpec}` and move `max_events_for_buffer` into core for shared sizing.
- Add Link adapter (`LinkPhaseSource`) and refactor the cpal callback to delegate to `Machine::on_buffer`.
- Add CLI `agogo run` (six-rate static dispatch, internal/external/link sources, Ctrl-C stop) and update docs / float-allowlist accordingly.

### Reviewed changes

Copilot reviewed 16 out of 17 changed files in this pull request and generated 7 comments.

<details>
<summary>Show a summary per file</summary>

| File | Description |
| ---- | ----------- |
| scripts/check-floats.sh | Allowlist new float-ABI/argv-boundary modules added by Plan 14. |
| doc/versions/version-0.1.md | Update v0.1 status + acceptance scenario to the new `--ch` syntax. |
| doc/reviews/review-00016.md | Add a PR review record for Plan 14. |
| doc/plans/plan-2026-04-24-03.md | Add the Plan 14 design/verification plan and review notes. |
| crates/host-link/src/source.rs | New `LinkPhaseSource` adapter + contention/no-deadlock tests. |
| crates/host-link/src/session.rs | Expose `phase_at_sample` shim on `LinkSession` for the adapter. |
| crates/host-link/src/lib.rs | Export the new `source` module types under `rusty-link`. |
| crates/host-cpal/src/cpal/callback.rs | Shrink callback state and delegate buffer work to `Machine`. |
| crates/core/src/channel/spec.rs | Implement the docker-style `--ch key=val,...` spec parser + Display round-trip. |
| crates/core/src/control.rs | Implement the N-channel `Machine` orchestrator + transport policy + tests/proptests. |
| crates/core/src/lib.rs | Export the new `machine` module. |
| crates/core/src/control/event.rs | Move `max_events_for_buffer` into core scheduler for reuse. |
| crates/cli/src/run.rs | New `agogo run` handler (rate dispatch, sources, Ctrl-C teardown, smoke tests). |
| crates/cli/src/main.rs | Add `run` subcommand behind the `run` feature; adapt demo to `Machine`. |
| crates/cli/Cargo.toml | Add `run` feature, `ctrlc` dep, and explicit `agogo` + `agogo-cli` bin targets. |
| Cargo.lock | Lock `ctrlc` and its transitive dependencies. |
| CLAUDE.md | Update float-allowlist documentation to include the new Plan 14 exception files. |
</details>






<!-- gh-id: 3141397415 -->
### Copilot on [`crates/host-link/src/session.rs:9`](https://github.com/cmk/agogo/pull/16#discussion_r3141397415) (2026-04-25 04:07 UTC)

`PhaseSourceImpl` is imported but not used anywhere in this file, which will trigger `unused_imports` warnings when the `rusty-link` feature is enabled. Please remove the import (or add the missing use/impl if it was intended).
```suggestion

```

<!-- gh-id: 3141414175 -->
### Copilot on [`crates/cli/src/run.rs:292`](https://github.com/cmk/agogo/pull/16#discussion_r3141414175) (2026-04-25 04:14 UTC)

This startup line prints `--midi-out {}` but `agogo run` no longer has a `--midi-out` flag (routing is via `--ch ... out=<port>`). This is user-facing output and can be confusing; consider renaming it to something like "MIDI out"/"midi port" (or print the selected `out=` value) without implying a CLI flag.

<!-- gh-id: 3141414193 -->
### Copilot on [`doc/versions/version-0.1.md:73`](https://github.com/cmk/agogo/pull/16#discussion_r3141414193) (2026-04-25 04:14 UTC)

The acceptance command here is missing `--features run`. `agogo run` is `#[cfg(feature = "run")]` (and `agogo-cli` defaults to `features = ["core"]`), so `cargo run --bin agogo -- run ...` will build a binary without the `run` subcommand unless the feature is enabled.
```suggestion
- `cargo run --features run --bin agogo -- run --audio-in <device> --bpm 120
```

<!-- gh-id: 3141414202 -->
### Copilot on [`doc/plans/plan-2026-04-24-03.md:529`](https://github.com/cmk/agogo/pull/16#discussion_r3141414202) (2026-04-25 04:14 UTC)

The E2E acceptance snippet uses `cargo run --bin agogo -- run ...` but `agogo run` is feature-gated (`#[cfg(feature = "run")]` in `crates/cli/src/main.rs`). To be copy/paste runnable it should include `--features run` (as in the PR description’s verification command).

<!-- gh-id: 3141414207 -->
### Copilot on [`doc/reviews/review-00016.md:255`](https://github.com/cmk/agogo/pull/16#discussion_r3141414207) (2026-04-25 04:14 UTC)

This review record appears out of sync with the PR’s final state. It still marks `run_help_lists_all_six_rates` as missing and calls out `ChannelSpec::Display` as non-round-trippable, but the Plan doc now explicitly defers the help-text property and `spec.rs` currently quotes `out=` values. Since this file is added in the same PR, consider updating it to reflect what was actually fixed vs. deferred (or clearly label it as an earlier snapshot) to avoid leaving misleading documentation in-tree.
```suggestion
**Resolved / deferred in this PR:**

1. **`run_help_lists_all_six_rates`: deferred, not missing.** The
   Verification table item is not implemented in this PR, but the Plan
   doc now records the help-text property as deferred, so this is no
   longer a must-fix before push.

2. **`ChannelSpec::Display` quoting issue: fixed.** The final
   implementation now quotes `out=` values when needed, so the earlier
   non-round-trippable `Display` concern is resolved.
```

<!-- gh-id: 4174988173 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-04-25 04:14 UTC](https://github.com/cmk/agogo/pull/16#pullrequestreview-4174988173))

## Pull request overview

Copilot reviewed 16 out of 17 changed files in this pull request and generated 4 comments.





<!-- gh-id: 3141517926 -->
#### ↳ cmk ([2026-04-25 05:32 UTC](https://github.com/cmk/agogo/pull/16#discussion_r3141517926))

Fixed in fd54435 — `request_stop()` now short-circuits the clock-render path as well as transport bytes. `TransportState.running` doc rewritten as a "local stop/teardown latch": flips to false *only* via `request_stop()` (the `stop_pending` arm), and `Machine::on_buffer` emits no transport bytes **and** no clock events while it's false. Policy-driven Stop bytes (LinkDriven/Scripted) do NOT clear the latch.

<!-- gh-id: 3141517973 -->
#### ↳ cmk ([2026-04-25 05:32 UTC](https://github.com/cmk/agogo/pull/16#discussion_r3141517973))

Fixed in fd54435 — kept the docs as written and made the code match: `Machine::on_buffer` now early-returns before the per-channel render loop when `transport.running` is false. The `transport_internal_emits_start_then_stop` test gained a clock-suppression assertion pinning the contract: at T4/120 BPM/48 kHz, only the buffer-0 clock survives — the natural clock at sample 24,000 (inside the stop-buffer) is now suppressed.

<!-- gh-id: 3141518095 -->
#### ↳ cmk ([2026-04-25 05:32 UTC](https://github.com/cmk/agogo/pull/16#discussion_r3141518095))

Fixed in fd54435 — reworded the status banner to "All v0.1 foundation + output-chain sprints have shipped. Plan 14 is the **final in-flight piece**; once it lands the v0.1 acceptance scenario below is reachable end-to-end." Acceptance reachability now correctly conditioned on Plan 14 landing rather than claimed already-shipped.

<!-- gh-id: 3141518169 -->
#### ↳ cmk ([2026-04-25 05:32 UTC](https://github.com/cmk/agogo/pull/16#discussion_r3141518169))

Fixed in fd54435 — added a "historical snapshot" banner above the Local-review section pointing at the round-1 fix commit 47ad128 that resolved the must-fix items. Both Display-quoting and run_help_lists_all_six_rates deferral are explicitly named as resolved; the two follow-ups (proptest generator-bound docs, scripted-Stop comment) too. The section stays as the audit trail of what the Tier-1 reviewer found, just clearly labelled as a frozen pre-fix snapshot.

<!-- gh-id: 3141518332 -->
#### ↳ cmk ([2026-04-25 05:32 UTC](https://github.com/cmk/agogo/pull/16#discussion_r3141518332))

Fixed in fd54435 — struck through the `run_help_lists_all_six_rates` row in the Verification table with a one-line pointer to the Review section's "Verification-table property deferred" rationale. Rationale: the existing `run_rejects_unsupported_rate` smoke test already enumerates all six rates in its rejection-error message, and `run_with_rate::<R>` is monomorphised six ways at the dispatch site so a typo in any rate's pipeline breaks the build.

<!-- gh-id: 3141518405 -->
#### ↳ cmk ([2026-04-25 05:32 UTC](https://github.com/cmk/agogo/pull/16#discussion_r3141518405))

Fixed in fd54435 — "uncontested" → "uncontended" in the lock-acquisition comment.

<!-- gh-id: 3141518570 -->
#### ↳ cmk ([2026-04-25 05:32 UTC](https://github.com/cmk/agogo/pull/16#discussion_r3141518570))

False positive — `PhaseSourceImpl` provides the `phase_at_sample` method on `LinkClock` that the session-level shim delegates to (`self.clock.phase_at_sample(n)`). Removing the import breaks `cargo build --features rusty-link` with E0599 ("method `phase_at_sample` not found"). Verified locally by removing the import and re-running the build. Added an inline comment to the import in fd54435 naming the false positive so future automated reviewers don't re-flag it.

<!-- gh-id: 3141518615 -->
#### ↳ cmk ([2026-04-25 05:33 UTC](https://github.com/cmk/agogo/pull/16#discussion_r3141518615))

Fixed in fd54435 — banner now prints `(midi port: <name>)` instead of `--midi-out <name>`, removing the implication of a non-existent CLI flag.

<!-- gh-id: 3141518721 -->
#### ↳ cmk ([2026-04-25 05:33 UTC](https://github.com/cmk/agogo/pull/16#discussion_r3141518721))

Fixed in fd54435 — `--features run` added to the v0.1 acceptance command. Confirmed copy-pastable.

<!-- gh-id: 3141518810 -->
#### ↳ cmk ([2026-04-25 05:33 UTC](https://github.com/cmk/agogo/pull/16#discussion_r3141518810))

Fixed in fd54435 — `--features run` added to the E2E acceptance snippet in the plan doc, matching the version-0.1.md command shape.

<!-- gh-id: 3141518857 -->
#### ↳ cmk ([2026-04-25 05:33 UTC](https://github.com/cmk/agogo/pull/16#discussion_r3141518857))

Fixed in fd54435 (same banner approach as the earlier review-00016 thread) — the Local-review section now opens with a "historical snapshot" banner naming the round-1 fix commit (47ad128) and explicitly listing the four resolved items: ChannelSpec::Display quoting (fixed), run_help_lists_all_six_rates (deferred with documented rationale), proptest generator-bound docs (added), scripted-Stop comment (rewritten).
