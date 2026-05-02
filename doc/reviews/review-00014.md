# PR #14 — Plan 12: `out/midi` — MidiSink trait + MIDI clock byte emission

## Summary

First of the three remaining v0.1 output-chain sprints. Adds
pure-logic MIDI clock rendering: the `MidiSink` trait contract (per
`doc/agogo.md` §5), a synthetic `TestSink` for unit/proptest use,
and a byte-emission pipeline that maps per-channel `ScheduledEvent`
slices to `MidiSink::send_at(&[0xF8], at_sample)` calls.

### What ships

- **`crates/core/src/sink/midi.rs`** — the new module:
  - `MidiSink: Send` trait with
    `fn send_at(&self, msg: &[u8], at_sample: u64)`. Implementations
    may allocate or take locks (midir does both); Plan 13's
    `rt/control.rs` will wire an `rtrb` drain thread so the audio
    callback enqueues without calling `send_at` directly.
  - `MIDI_CLOCK` / `MIDI_START` / `MIDI_CONTINUE` / `MIDI_STOP` byte
    constants matching MIDI 1.0 §System Real-Time Messages.
  - `TestSink` (Mutex-backed) implementing `MidiSink` — for tests
    and the CLI tracer only, not RT-safe by design.
  - `MidiRtByte { Start, Continue, Stop }` enum for the three
    single-byte transport messages with `.status_byte() -> u8`.
  - `render_clock_block`, `render_buffer`, `render_channel_block` —
    the three rendering layers, from zero-alloc byte emission up to
    the per-channel dispatch entry point. `render_channel_block`
    matches exhaustively on `ChannelMode`; the non-clock variants
    (`Din` / `AnalogPulse` / `AnalogLfo` / `MidiCc`) are explicit
    no-ops, their rendering paths parked in v0.2+ per their stub
    doc-comments.
- **`crates/cli/src/main.rs`** — `agogo midi trace` subcommand
  mirroring the existing `sync trace` / `channel trace` / `link
  probe` shape. Runs `tick_stream` + `render_channel_block` against
  a `TestSink` for N buffers and dumps `at_sample,byte_hex` CSV.
  `--start` and `--stop-on-exit` flags inject `MidiRtByte::Start` /
  `MidiRtByte::Stop` on the first / last buffer respectively.

### Why

`doc/versions/version-0.1.md` scopes this as a prerequisite for
Plan 13 (`host-cpal` + `host-midi` + RT plumbing). Plan 13's
audio-callback hot loop will call `render_channel_block`; Plan 13's
midir backend will implement `MidiSink`. Shipping the pure-logic
contract + a synthetic sink first lets Plan 13 be entirely about
integration, not abstraction design.

### Naming

`MidiRtByte` (not `TransportEvent`) because Plan 14's transport FSM
(`doc/designs/transport.md:66-68`) owns a higher-level
`TransportEvent` enum — `{ Play, Stop, Locate, PhaseSourceStart,
PhaseSourceStop }` — whose transitions the Playhead maps down to
these real-time bytes per buffer. `MidiRtByte` names exactly what it
is: a member of MIDI 1.0's System Real-Time Messages category.

### Verification

All six Verification-table properties from the plan green:

| Property | Module |
|---|---|
| `clock_every_event_produces_one_record` | `core::out::midi::tests` |
| `clock_sample_order_preserved` | `core::out::midi::tests` |
| `midi_rt_byte_in_expected_range` | `core::out::midi::tests` |
| `render_buffer_emits_rt_byte_first` | `core::out::midi::tests` |
| `non_clock_modes_are_noop` | `core::out::midi::tests` |
| `block_render_matches_scheduler` | `core::out::midi::tests` |

Plus 9 unit tests in `out::midi::tests` (byte constants, `TestSink`
FIFO + `Send`-across-threads behaviour, spot checks on each of the
three renderers, `MidiRtByte::status_byte()` exhaustive check,
`render_channel_block` MidiClock routing) and 5 in `cli::midi_trace`
(quarter-note cadence at 120 / 48 k / t4, `--start` byte placement,
`--stop-on-exit` byte placement on the final buffer, bad-divider +
negative-BPM rejection).

Workspace totals: **238 tests, all green** (up from 224). Clippy
clean on both `agogo-core` and `agogo-cli`. The pre-existing
scheduler `scheduler_block_equivalence` property still passes
unchanged — the composition is verified end-to-end by the new
`block_render_matches_scheduler`.

### E2E smoke

```
$ cargo run -q -p agogo-cli -- midi trace \
    --bpm 120 --sr 48000 --divider t4 \
    --frames 24000 --buffers 4 \
    --start --stop-on-exit
at_sample,byte
0,0xFA       ← Start on buffer 0 (buffer_start_sample = 0)
0,0xF8       ← clock event at master tick 0
24000,0xF8   ← quarter note at 120 BPM / 48 k
48000,0xF8
72000,0xFC   ← Stop on last buffer (buffer_start_sample = 72000)
72000,0xF8
```

### Design notes

- `render_buffer` and `render_channel_block` take `&dyn MidiSink`
  rather than `&impl MidiSink` so Plan 13's dispatcher can hold a
  boxed sink (e.g. `Arc<dyn MidiSink>`) without monomorphising the
  render path per backend.
- MIDI 1.0 pins clock at 24 PPQN — one `0xF8` every `PPQN/24` master
  ticks. At agogo's 192 PPQN that's every 8 master ticks, which is
  `TBase::T32t` (32nd-note triplet). The E2E example uses `t4` (one
  byte per beat) for human readability; pick `t32t` for a
  spec-compliant 24 PPQN stream. Plan 14's `agogo run` will default
  to `t32t` for the same reason. `doc/designs/output.md:37-41` is
  the design source.
- No new external dependencies. Zero `unsafe` code
  (`#![forbid(unsafe_code)]` already set crate-wide).
- No stored floats added outside the existing CLI argv-boundary
  pattern that `scripts/check-floats.sh` will enforce once Plan 11
  MR #2 lands.

### What's deferred

Real backends (midir, CoreMIDI, JACK, WinMM, ALSA-MIDI), non-clock
`ChannelMode` rendering, Song Position Pointer, SysEx, per-format
latency compensation, the RT-safe enqueue path, and transport-FSM
integration — all named with their target version in the plan's
Deferred section.

### Next

- **Plan 13** (`host-cpal` + `host-midi` + `rt/`): draft already in
  the working tree at `doc/plans/plan-2026-04-24-02.md`; kicks off
  once this lands. Adds the cpal audio host + midir sink + rtrb
  control plane + `agogo demo` CLI.

## Local review (2026-04-24)

**Branch:** plan/2026-04-24-01
**Commits:** 7 (origin/main..plan/2026-04-24-01)
**Reviewer:** Claude (sonnet, independent)

---

### Commit Hygiene

Seven commits, all with valid conventional-commit prefixes (`plan:`,
`feat(out):`, `feat(cli):`, `doc:`). Subjects are under 72
characters. The task-to-commit mapping is clean: one commit per task
(T1–T5), a sprint-opener, and a finalization commit. No CI-repair
commits or merge commits are present. This section is clean.

### Code Quality

**Module layout.** `` as a file alongside a
`src/out/` directory is correct modern Rust module layout.
`#![forbid(unsafe_code)]` is already set crate-wide. No `mod.rs`
used. All conventions followed.

**`TestSink` is public in `agogo-core`.** `TestSink`, `TestRecord`,
and `render_clock_block` / `render_buffer` are all `pub`. This means
they are part of `agogo-core`'s public API surface. For a test-only
type, this is appropriate since downstream crates (Plan 13's midir
integration tests) will use it — the plan anticipates this. Worth
noting for Plan 13 when the real `MidiSink` implementors land.

**`r.bytes[0]` index in `midi_trace::trace`.** The `midi_trace::trace`
path in `crates/cli/src/main.rs` indexes `r.bytes[0]` without a
length check. The surrounding comment acknowledges the assumption.
At present safe because `render_clock_block` and `render_buffer`
only call `send_at` with one-byte slices. A future empty-slice
`send_at` would panic here. Adequate for now; see Follow-up.

**No dead code, no redundant logic, no clippy-visible issues** from
reading the diff. `clear()` is used in the test suite. All public
items have doc comments.

**No new external dependencies.** Verified — no `Cargo.toml`
changes.

### Test Coverage

**All six Verification-table properties present**, each mapped
cleanly to a named proptest:

| Plan property | Test name | Present |
|---|---|---|
| `clock_every_event_produces_one_record` | same | yes |
| `clock_sample_order_preserved` | same | yes |
| `transport_byte_in_expected_range` | `midi_rt_byte_in_expected_range` | yes |
| `render_buffer_emits_transport_first` | `render_buffer_emits_rt_byte_first` | yes |
| `non_clock_modes_are_noop` | same | yes |
| `block_render_matches_scheduler` | same | yes |

**Generator domains.** `clock_every_event_produces_one_record` and
`clock_sample_order_preserved` use `any::<u64>()` — full domain,
correct per CLAUDE.md. `render_buffer_emits_rt_byte_first` uses
`any::<u64>()` for both `buffer_start` and sample values — correct.

`block_render_matches_scheduler` bounds `buffer_start in
0u64..=1_000_000` and `frames in 1usize..=8_192`. These bounds are
not full-domain, and CLAUDE.md states bounding to avoid arithmetic
is an anti-pattern. Here the bounds are set to stay within
`tick_stream`'s own tested domain (the render path itself does no
arithmetic on these values), so this is a defensible judgment call,
but an inline comment would prevent a future reader from flagging
it as coverage-faking. See Follow-up.

**`non_clock_modes_are_noop` uses a fixed `MidiCc` value.**
`prop::sample::select` draws from a slice containing only
`ChannelMode::MidiCc { cc: 74, range: (0, 127) }`. Since the no-op
arm is `ChannelMode::MidiCc { .. } => {}`, the actual cc/range
values are irrelevant — the test passes for any `MidiCc` by the
match arm alone. Fine.

**`TestSink` concurrency spot check** (`test_sink_send_across_threads`)
uses `thread::scope`, which handles join correctly. Well written.

**No fixture-gated tests** in this diff; no `fixture_or_skip!`
needed.

**One missing CLI spot check.** Plan's spot-check list includes
`--frames 4096 --buffers 16 --start` producing 16 rows at multiples
of 24 000. The implemented tests use 24 000-sample buffers rather
than 4 096 (to hit whole-beat positions cleanly). Covers the
substance; deviation is defensible but unacknowledged. Not a
must-fix.

### Plan Conformance

**Task-to-commit mapping exact.**

- T1 (trait + constants + TestSink): `6b9f904` — API verbatim.
- T2 (render_clock_block): `df54198` — signature matches.
- T3 (MidiRtByte + render_buffer): `acebc72` — enum + signature
  match.
- T4 (render_channel_block): `4033ed5` — exhaustive match, same
  arms.
- T5 (lib.rs wire-up + CLI): `accda88` — `pub mod out;` added;
  `agogo midi trace` with all planned flags.

**`TransportEvent` → `MidiRtByte` rename** is consistent across
code and docs; no stale references.

**One CLI flag type deviation.** Plan specifies `--buffers <usize>`,
implementation declares `u32` in `MidiSub::Trace` and
`TraceArgs.buffers`. Harmless narrowing (on a 64-bit host `usize`
and `u32` overlap below 2^32) but undocumented. See Follow-up.

### Risks

**`r.bytes[0]` panic path.** Noted above. CLI diagnostic tool, so
impact is a confusing crash rather than production outage.

**`ChannelMode` exhaustiveness.** Match is exhaustive (`Din |
AnalogPulse | AnalogLfo | MidiCc { .. } => {}`). Future `ChannelMode`
variants produce a compile error — correct design.

**`Mutex::lock().unwrap()` in `TestSink`.** Poisoning only propagates
across a shared instance, and `TestSink` is always fresh per test
(no global state). `unwrap()` is appropriate.

**BPM range inconsistency.** Range check uses `..u32::MAX`
(exclusive) but the error message prints `]` (inclusive). No user
impact at musical BPMs (~4294 BPM boundary) but latent doc bug.
See Follow-up.

### Recommendations

**Must fix before push:**

None. No convention violations, broken tests, or bugs block
landing.

**Follow-up (future work):**

1. **`r.bytes[0]` index in `midi_trace::trace`**
   (`crates/cli/src/main.rs`). When Plan 14 widens the CSV schema
   for multi-byte messages, replace the unchecked index with a
   length guard that returns an error rather than panicking.
2. **`block_render_matches_scheduler` generator bounds**
   (`crates/core/src/sink/midi.rs`). Add an inline comment
   explaining the bounds stay within `tick_stream`'s tested domain,
   not to avoid arithmetic in the render path.
3. **BPM error message / range inconsistency** in
   `midi_trace::trace` (`crates/cli/src/main.rs`). Fix either the
   range to `..=u32::MAX` or the message to `)`.
4. **`--buffers` type deviation from plan** (`usize` → `u32`).
   Document the choice in a comment at the field declaration or in
   the plan's Review section.

<!-- gh-id: 3140417069 -->
### Copilot on [`crates/cli/src/main.rs:1007`](https://github.com/cmk/agogo/pull/14#discussion_r3140417069) (2026-04-24 21:35 UTC)

When `--start` and `--stop-on-exit` are both set and `--buffers` is 1, this match emits only `Start` and never emits `Stop` because the `(true, _)` arm wins. If the flags are meant to be independent, handle the `buffers == 1` case explicitly (emit both bytes at `start_sample`) or document the precedence.
```suggestion
            let emit_start = args.start && b == 0;
            let emit_stop = args.stop_on_exit && b == last;
            let transport = match (emit_start, emit_stop, args.buffers == 1) {
                // With a single buffer both flags target the same block.
                // Keep Start precedence here because this trace path passes
                // only one transport byte into `render_channel_block`.
                (true, true, true) => Some(MidiRtByte::Start),
                (true, _, _) => Some(MidiRtByte::Start),
                (_, true, _) => Some(MidiRtByte::Stop),
```

<!-- gh-id: 3140417108 -->
### Copilot on [`doc/reviews/review-00014.md:169`](https://github.com/cmk/agogo/pull/14#discussion_r3140417108) (2026-04-24 21:35 UTC)

This review note contains stale line references (e.g. `crates/cli/src/main.rs:781-782` / `:728` no longer match the current file; `r.bytes[0]` is much later in the new `midi_trace` module). Consider updating these to current line numbers or, better, referencing the symbol/section name so the note stays accurate as the file grows. Also, `output.md:37-41` should likely be referenced as `doc/designs/output.md:37-41` to avoid ambiguity.
```suggestion
The `midi_trace::trace` path in `crates/cli/src/main.rs` indexes
`r.bytes[0]` without a length check. The surrounding comment
acknowledges the assumption. At present safe because
`render_clock_block` and `render_buffer` only call `send_at` with
one-byte slices. A future empty-slice `send_at` would panic here.
Adequate for now; see Follow-up.
```

<!-- gh-id: 4173542395 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-04-24 21:35 UTC](https://github.com/cmk/agogo/pull/14#pullrequestreview-4173542395))

## Pull request overview

Adds the first v0.1 output-chain layer for MIDI by introducing a backend-agnostic `MidiSink` contract, pure logic rendering for MIDI clock/transport bytes, and a CLI tracer that exercises the scheduler + renderer end-to-end.

**Changes:**
- Added `agogo-core::out::midi` with `MidiSink`, MIDI real-time byte constants, `TestSink`, and rendering functions (`render_clock_block` / `render_buffer` / `render_channel_block`).
- Wired the new `out` module into `agogo-core` and added `agogo midi trace` to emit `(at_sample, byte)` CSV from a synthetic run.
- Added/updated plan + review docs for Plan 12.

### Reviewed changes

Copilot reviewed 5 out of 6 changed files in this pull request and generated 4 comments.

<details>
<summary>Show a summary per file</summary>

| File | Description |
| ---- | ----------- |
| doc/reviews/review-00014.md | Adds a PR review record for Plan 12 changes. |
| doc/plans/plan-2026-04-24-01.md | Adds the Plan 12 implementation plan and verification checklist. |
| crates/core/src/sink/midi.rs | Implements the MIDI sink trait, test sink, and clock/transport renderers + tests. |
|  | Introduces `out` module root and exports `midi`. |
| crates/core/src/lib.rs | Exposes `pub mod out;` from `agogo-core`. |
| crates/cli/src/main.rs | Adds `midi trace` subcommand and a `midi_trace` module to run scheduler+renderer and print CSV. |
</details>






<!-- gh-id: 3140417133 -->
### Copilot on [`doc/plans/plan-2026-04-24-01.md:245`](https://github.com/cmk/agogo/pull/14#discussion_r3140417133) (2026-04-24 21:35 UTC)

The CLI synopsis in T5 is out of sync with the implementation: `--buffers` is documented as `<usize>` but the code uses `u32`, and the `--bpm` comment mentions `F64TMP` even though the implementation does a manual `f64 -> Tempo` conversion. Also `output.md:37-41` is ambiguous given the file lives at `doc/designs/output.md`; consider updating the reference to the full path. (There’s also a potential inconsistency with the later Review/Outcome commit count vs the local review doc.)

<!-- gh-id: 3140417154 -->
### Copilot on [`crates/cli/src/main.rs:954`](https://github.com/cmk/agogo/pull/14#discussion_r3140417154) (2026-04-24 21:35 UTC)

The `--bpm` range check excludes `u32::MAX` (`(0.0..u32::MAX as f64)`), but the error message says the upper bound is inclusive (`(0, ...]`). This is internally inconsistent and rejects the value the message claims is allowed; adjust either the range (`..=`) or the message (use `)` / clarify exclusivity).
```suggestion
                    "--bpm {} out of range (expected (0, {}) BPM)",
```

<!-- gh-id: 3140447420 -->
#### ↳ cmk ([2026-04-24 21:41 UTC](https://github.com/cmk/agogo/pull/14#discussion_r3140447420))

Fixed — added a comment on the match documenting Start-wins precedence for the single-buffer case. `render_buffer` emits at most one transport byte per call and 'stop before start' has no musical meaning, so the two-arm form with an explanatory comment stays simpler than threading a third tuple value.

<!-- gh-id: 3140447802 -->
#### ↳ cmk ([2026-04-24 21:42 UTC](https://github.com/cmk/agogo/pull/14#discussion_r3140447802))

Fixed — replaced the brittle line-number refs (`main.rs:781-782`, `:728`, `midi.rs:449-450`) with symbol-level refs (`midi_trace::trace`, `block_render_matches_scheduler`) in the local review section, and expanded `output.md:37-41` to `doc/designs/output.md:37-41`.

<!-- gh-id: 3140448061 -->
#### ↳ cmk ([2026-04-24 21:42 UTC](https://github.com/cmk/agogo/pull/14#discussion_r3140448061))

Fixed — dropped the stale `F64TMP` reference (no such Conn on main; the sprint opener was aspirational), corrected `--buffers` in the synopsis to `<u32>`, and expanded the `output.md` reference to its full `doc/designs/output.md` path. The Reuse-section `Tempo` entry now spells out the inline `f64 × 10⁶ → u32` boundary conversion that mirrors `channel_trace`.

<!-- gh-id: 3140448426 -->
#### ↳ cmk ([2026-04-24 21:42 UTC](https://github.com/cmk/agogo/pull/14#discussion_r3140448426))

Fixed — changed the error message from `(0, {}] BPM` to `(0, {}) BPM` to match the exclusive upper bound of the `0.0..u32::MAX` range check. `channel_trace` has the same pre-existing inconsistency (same pattern, same line shape) but that's out of scope for this PR.
