# PR #21 — feat: mode=click metronome + bars multiplier

## Summary

Adds a per-channel "metronome" output mode and a divider-agnostic
period multiplier — two small features that compose to express
arbitrary click patterns without any new audio-output plumbing.

**`mode=click`** routes a per-tick MIDI Note On (followed by a
same-sample Note Off) through the existing `MidirSink`. Per-channel
configuration:

```
--ch dev=midi,mode=click,div=t4,note=37,vel=70,mch=10,\
     accent-every=4,accent-note=38,accent-vel=120,out="IAC Bus 1"
```

The accent counter substitutes a different note/vel every Nth
emitted click (counted per-channel from transport start; reset on
stop). With no audio-output plumbing pulled forward from v0.4, the
user's hardware/soft synth turns the Note On into sound.

**`bars=N`** is a divider-agnostic multiplier on the channel's
scheduled tick stream — emit every N-th `tick_stream_into` event.
Idiomatic case `div=t1,bars=N` fires every N bars (in 4/4); the
mechanism also works on smaller dividers, so `div=t8,bars=3`
expresses a dotted-quarter cadence not in `Grid::ALL`.

### Why these shapes

- **`mode=` is a new spec key**, not an overload of `dev=`. `dev=` is
  the routing target axis (MIDI vs. future audio); `mode=` is the
  role axis (clock, click, future cc/lfo). Both axes will eventually
  combine — `dev=audio,mode=click` is the v0.4 audio metronome.
- **`ChannelMode::Click(ClickConfig)` nests output specifics.** The
  agnostic role variant (`Click(_)`) carries no MIDI fields; the
  inner `ClickConfig::Midi(MidiClickConfig{note, vel, ch, accent})`
  is where MIDI lives. Adding `ClickConfig::Audio(...)` later won't
  touch the role layer.
- **`Channel.bar_multiplier: Option<NonZeroU16>`.** Sized to the
  addressable horizon: `Tick(u32) / Grid::T1 (3840) ≈ 1.1M` bars,
  so `u16::MAX (65,535)` caps at ~5.8% of that with comfortable
  headroom (~36 hours at 120 BPM in 4/4). `bars × 3840` fits in u32
  with no overflow-check arithmetic.
- **Per-channel accent and bar counters live on `Machine`,** not on
  `Channel`. `Channel` stays `Copy`. Both reset to 0 in the existing
  transport-stop arm.

### Test coverage

- `agogo-core` lib: 320 passing (up from 302). New properties:
  `click_every_event_produces_two_records`,
  `click_records_are_paired_on_off`,
  `click_status_byte_carries_channel`,
  `accent_lands_every_n_emitted_clicks_from_zero`,
  `click_counter_persists_across_calls`. Plus 4 Machine-level
  integration tests for counter reset on stop and bars+accent
  composition.
- `agogo-cli`: 25 passing. New: `run_accepts_mode_click_spec`,
  `run_accepts_bars_on_non_t1_div`.
- `spec_round_trip` proptest extended to generate both
  `mode=clock` and `mode=click` variants with arbitrary
  note/vel/mch/accent/bars; pins parser ↔ Display symmetry.
- `clippy --all-targets -- -D warnings` clean.
- `scripts/check-floats.sh` exit 0 (no new floats introduced).

### Out of scope

- Audio-output click (`dev=audio,mode=click`) — defers to v0.4 with
  the rest of the cpal-output plumbing; the nested `ClickConfig`
  shape leaves room for an `Audio(_)` variant.
- Time-signature / global bar awareness — per-channel accent is
  deliberately tick-counted, independent of the transport-FSM
  time-sig work scoped to v0.5.
- `mode=cc` — could fold in if it stays small, but not in this ask.
- Multi-MIDI-port routing — v0.2 deferral.

### Demo

```
cargo run -p agogo-cli --features run -- run --bpm 120 --sr 48000 \
  --audio-in default \
  --ch dev=midi,mode=click,div=t4,note=37,vel=70,mch=10,\
       accent-every=4,accent-note=38,accent-vel=120,out="IAC Bus 1" \
  --ch dev=midi,mode=click,div=t8,note=42,vel=50,mch=10,out="IAC Bus 1" \
  --ch dev=midi,mode=click,div=t1,bars=4,note=39,vel=110,mch=10,out="IAC Bus 1"
```

A 4/4 metronome on a GM drum kit: side-stick quarters with a
hand-clap accent on the 1, hi-hat eighths underneath, and a single
phrase-marker click every 4 bars.

Plan: [`doc/plans/plan-2026-04-25-03.md`](../plans/plan-2026-04-25-03.md).

## Local review (2026-04-25)

**Branch:** plan/2026-04-25-03
**Commits:** 4 (origin/main..plan/2026-04-25-03)
**Reviewer:** Claude (sonnet, independent)

---

### Commit Hygiene

Four commits:
1. `plan: mode=click metronome + bar multiplier, sprint goals` — correct prefix, opens the branch.
2. `feat(core): T1–T4 ClickConfig types, rendering, Machine counters + bar filter` — bundles four tasks but they form a coherent unit (types + rendering + machine wiring). Acceptable.
3. `feat(core,cli): T5 spec parser + T6 CLI smoke test` — both tasks are parser/CLI tier; fine together.
4. `doc: Finalize plan 03 and PR description` — correct.

All subjects are under 72 characters. No merge commits visible.

### Code Quality

**Conventions.** `thiserror` in the lib crate, no `unsafe`, `F64F06`/`Conn` for ms→µs conversions, modern module layout (no `mod.rs`). `micro_from_ms` wraps `F64F06.ceil` correctly — not open-coded arithmetic.

**`render_channel_block` always passes `Some` counter to every channel.** `Machine::on_buffer:344` passes `Some(&mut self.click_counters[idx])` regardless of whether the channel is `Click` or `MidiClock`. The `click_counter` parameter is unused in the `MidiClock` / stub arms, so this is safe. It does mean the parallel `click_counters` vec is advanced-by-never for non-Click channels — the doc comment on the struct field correctly says "Slot is meaningful only for `ChannelMode::Click(_)` channels."

**`accent_every = 0` double-defense.** The `accent-every` parser rejects 0 at line 211–215 and then uses `NonZeroU32::new(e).expect("accent-every > 0")` at line 343. The `expect` is unreachable code if the parser runs first; `// SAFETY:` would be a clearer comment than `// SAFETY: e > 0 enforced at parse time`, but this is cosmetic.

**`arb_spec` generates `shift_ms` only in `0..=300`.** Negative shift_ms values are valid `ChannelSpec` state (clamped to zero by `into_channel`, not by `parse`). The spec round-trip proptest never exercises a negative `shift_ms`. Since the Display path emits it and parse accepts it, this is a gap but one with low real-world impact (negative shift is a no-op via clamp; any value round-trips). No comment explains the bound.

**Error message specificity.** `ChannelSpecError::BadValue("note", "only valid with mode=click")` — sufficient to diagnose from a log line.

**No dead code, no unexpected cross-crate dependencies.**

### Test Coverage

**Verification table audit:**

| Plan property | Status |
|---|---|
| `click_every_event_produces_two_records` | proptest in `out::midi` ✓ |
| `click_records_are_paired_on_off` | proptest in `out::midi` ✓ |
| `click_status_byte_carries_channel` | proptest in `out::midi` ✓ |
| `accent_lands_every_n_emitted_clicks_from_zero` | proptest in `out::midi` ✓ |
| `click_counter_advances_across_buffer_boundaries` | **spot check only** — see below |
| `click_counter_resets_on_transport_stop` | **spot check only** — see below |
| `bars_filter_emits_every_nth_grid_event` | **spot check only** — see below |
| `bars_counter_resets_on_transport_stop` | **spot check only** — see below |
| `spec_round_trip` (extended) | proptest in `machine::spec` ✓ |

The plan lists the bottom four rows under "Properties (must pass)" — meaning they must be `proptest!` blocks per CLAUDE.md convention. Three issues follow from this:

**Issue 1 — `click_counter_advances_across_buffer_boundaries` is a spot check, not a proptest.**

`crates/core/src/out/midi.rs:591`

`click_counter_persists_across_calls` hardcodes `accent.every = 3`, split at index 3, total 7 events. The plan requires this to be a property test: arbitrary split point `m`, arbitrary suffix count `n`, arbitrary accent period. With fixed values it doesn't catch regressions where off-by-one in counter advancement only manifests at specific (m+n, n) combinations.

**Issue 2 — `bars_filter_emits_every_nth_grid_event` is a spot check, not a proptest.**

`crates/core/src/machine.rs:771`

`bars_filter_keeps_every_nth_event` tests only `Grid::T1`, `bars=3`, 9 buffers of 96,000 frames. The plan's Verification table says: "Strategy varies divider across `Grid::ALL` and mode across clock/click." Only one divider and one mode are tested. The filter mechanism is divider-agnostic so the same filter bug could manifest on a `Grid::T8Q` (192-tick period) and this test would miss it.

**Issue 3 — `bars_counter_resets_on_transport_stop` and `click_counter_resets_on_transport_stop` are both spot checks, not proptests.**

`crates/core/src/machine.rs:855`

`counters_reset_on_transport_stop` covers both with fixed parameters (4 buffers of 96,000 frames, `Grid::T1`, `bars=2`, `accent-every=4`). A proptest varying these parameters would be more robust.

**Issue 4 — Generator domain for `accent_every` in `arb_mode` is bounded to `1..=64`.**

`crates/core/src/machine/spec.rs:749`

CLAUDE.md: "Bounding the generator to keep intermediate arithmetic 'safe'... is an anti-pattern — it fakes coverage by hiding the exact region where wrap / saturation bugs live. If you genuinely must bound the domain, document *why* immediately above the strategy." There is no comment explaining why `accent_every` is bounded at 64 rather than `u32::MAX`. The code (`counter % a.every.get()`) is well-defined for any `NonZeroU32`, so there is no arithmetic reason to restrict the domain. Same applies to `arb_bars` using `1..=1000` rather than `1..=u16::MAX`.

**Issue 5 — `arb_spec` generator bounds `shift_ms` to `0..=300` without comment.**

`crates/core/src/machine/spec.rs:781`

The `shift_ms` field of `ChannelSpec` can be any `f64` (including negative), but the strategy generates only `0..=300`. A negative `shift_ms` is a valid spec that round-trips through `Display`/`parse` cleanly — `into_channel` clamps it, but the round-trip test doesn't call `into_channel`. CLAUDE.md requires a comment documenting why the domain is bounded if it must be.

### Plan Conformance

T1 (`ClickConfig` + `ChannelMode::Click`) — implemented, `all_variants_constructible` updated.
T2 (`Channel.bar_multiplier`) — field added, all fixture builders updated.
T3 (Machine counters + bar filter) — `bar_counters` and `click_counters` added, filter applied, both reset on stop.
T4 (`render_midi_click_block` + dispatch) — implemented including the `expect` panic contract.
T5 (spec parser) — all keys handled, cross-key validation, Display round-trip.
T6 (CLI smoke test) — `run_accepts_mode_click_spec` and `run_accepts_bars_on_non_t1_div` added.

The T3 counter reset fires in the `!self.transport.running` branch. Per the plan, this covers "every transport-Stop path including Ctrl-C teardown" — that arm is hit whenever `running` has been latched off, regardless of what buffer brought it there.

Spot checks from the Verification table: all 15 listed spot checks are present and named correctly.

### Risks

**`render_channel_block` `expect` panic for missing counter.** The contract "Click channel must have `Some` counter" is enforced at runtime, not structurally. `Machine::on_buffer` always passes `Some(...)`, so the panic path is only reachable from test code or direct callers that forget the contract. The plan's Review section acknowledges this and defers a typed fix. Given the scope this is acceptable — but callers outside `Machine` (e.g., future integration tests constructing Click channels directly) risk the panic silently. A doc comment on `render_channel_block` already explains the precondition.

**No TODOs or stubs in new code.** Stub variants (`Din`, `AnalogPulse`, etc.) are pre-existing; no new ones added.

**Existing `MidiClock` rendering path.** The `render_channel_block` signature gained an `Option<&mut u32>` parameter. All pre-existing call sites now pass `None` — confirmed in test helpers (`zero_channel`). No behavior change for `MidiClock`.

---

### Recommendations

**Must fix before push:**

1. **`click_counter_advances_across_buffer_boundaries` must be a proptest.** (`out/midi.rs`) Convert `click_counter_persists_across_calls` to a `proptest!` with arbitrary split point `m in 0usize..=32`, total count `n in 0usize..=32`, and arbitrary accent period `any::<u32>().prop_filter(..., |&n| n > 0)`. This is the "buffer boundaries" invariant the plan commits to shipping.

2. **`bars_filter_emits_every_nth_grid_event` must be a proptest varying divider and mode.** (`machine.rs`) Convert `bars_filter_keeps_every_nth_event` to a `proptest!` sampling `Grid::ALL` for the divider and generating both `MidiClock` and `Click` mode channels, with arbitrary `bars=N` value in `1..=16`. The plan's Verification table description explicitly requires "Strategy varies divider across `Grid::ALL` and mode across clock/click."

3. **Generator domain bounds in `arb_mode` and `arb_bars` need justification comments or expansion.** (`spec.rs:749`, `spec.rs:768`) Per CLAUDE.md: either expand `1u32..=64` → `any::<u32>().prop_filter(...)` / `1u32..=u32::MAX` and `1u16..=1000` → `1u16..=u16::MAX`, or add a comment directly above each strategy function explaining why the narrowing is safe and add a spot check at the excluded boundary.

**Follow-up (future work):**

4. `arb_spec` bounds `shift_ms` to `0..=300` with no comment. Since `shift_ms` can be negative in `ChannelSpec` and still round-trip cleanly, the `spec_round_trip` proptest never exercises negative `shift_ms`. Worth widening the generator to `(-300i32..=300).prop_map(|n| n as f64)` and adding a comment noting that `into_channel` clamps to zero — this is a low-priority gap but one the CLAUDE.md domain rule applies to.

5. The `render_channel_block` `Option<&mut u32>` API makes the Click/non-Click invariant a runtime panic rather than a compile-time guarantee. The plan's Review section acknowledges this and defers a typed solution; tracking it as a v0.2 debt item would keep the deferral visible.

<!-- gh-id: 3143031925 -->
### Copilot on [`crates/core/src/machine/spec.rs:10`](https://github.com/cmk/agogo/pull/21#discussion_r3143031925) (2026-04-26 05:29 UTC)

Module docs describe `bars` as a “T1-multiplier on the divider”, but the implemented semantics (and the plan) are divider-agnostic “emit every Nth scheduled tick_stream event”. This wording is contradictory and can mislead readers into thinking `bars` is tied to `div=t1`.

Consider rephrasing to something like “divider-agnostic period multiplier (keep every Nth scheduled event)” and optionally call out that `div=t1` is the idiomatic ‘bars’ case.
```suggestion
//! `offset-ms`, `snap-quantum-us`, `bars` (divider-agnostic period
//! multiplier: keep every Nth scheduled event; `div=t1` is the
//! idiomatic "bars" case — see Plan 2026-04-25-03).
```

<!-- gh-id: 4176565740 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-04-26 05:29 UTC](https://github.com/cmk/agogo/pull/21#pullrequestreview-4176565740))

## Pull request overview

Adds two composable scheduling/output features to the core + CLI: a per-channel `mode=click` metronome (MIDI Note On + same-sample Note Off per tick) and a divider-agnostic `bars=N` multiplier (emit every Nth scheduled tick-stream event).

**Changes:**
- Add `ChannelMode::Click(ClickConfig::Midi(MidiClickConfig{...}))` and MIDI click rendering in `out/midi.rs`.
- Add `Channel.bar_multiplier` plus `Machine`-owned `bar_counters`/`click_counters`, applying the bars filter pre-render and resetting counters on transport stop.
- Extend `ChannelSpec` parsing/Display (`mode=click`, `note`/`vel`/`mch`, `accent-*`, `bars`) and add CLI smoke tests + plan/review docs.

### Reviewed changes

Copilot reviewed 12 out of 13 changed files in this pull request and generated 3 comments.

<details>
<summary>Show a summary per file</summary>

| File | Description |
| ---- | ----------- |
| doc/reviews/review-00021.md | Adds local review record for PR #21. |
| doc/plans/plan-2026-04-25-03.md | Adds implementation plan + verification matrix for click + bars. |
| crates/host-link/tests/bidirectional.rs | Updates channel fixtures to include `bar_multiplier: None`. |
| crates/host-link/src/session.rs | Updates test helper channel construction for new field. |
| crates/host-cpal/src/cpal/callback.rs | Updates test fixtures to include `bar_multiplier: None`. |
| crates/core/src/channel/mode.rs | Introduces `ClickConfig` / `MidiClickConfig` / `MidiClickAccent` and `ChannelMode::Click`. |
| crates/core/src/channel/transform.rs | Adds `Channel.bar_multiplier: Option<NonZeroU16>` to runtime `Channel`. |
| crates/core/src/channel/scheduler.rs | Updates scheduler test fixtures for `bar_multiplier`. |
| crates/core/src/out/midi.rs | Adds MIDI Note On/Off constants, click rendering, and dispatch via `render_channel_block`. |
| crates/core/src/machine/spec.rs | Extends spec grammar/parser/Display + proptests and spot checks for `mode=click` and `bars`. |
| crates/core/src/machine.rs | Adds per-channel counters, bars filtering, and click-counter threading/reset logic + tests. |
| crates/cli/src/run.rs | Adds CLI smoke tests to accept `mode=click` and `bars` on non-T1 dividers. |
| crates/cli/src/main.rs | Updates midi_trace call site for new `render_channel_block` signature; updates fixtures for `bar_multiplier`. |
</details>






<!-- gh-id: 3143031929 -->
### Copilot on [`crates/core/src/machine.rs:1005`](https://github.com/cmk/agogo/pull/21#discussion_r3143031929) (2026-04-26 05:29 UTC)

The `const _: u8 = MIDI_NOTE_OFF;` line looks like it’s only here to silence an unused import. This makes the test module noisier than necessary.

Prefer removing the unused `MIDI_NOTE_OFF` import (and this dummy const), or use the constant in an assertion so it’s referenced for a meaningful reason.
```suggestion
        let records = sink.records();
        let notes: Vec<u8> = records
            .iter()
            .filter(|r| r.bytes[0] == (MIDI_NOTE_ON | 9))
            .map(|r| r.bytes[1])
            .collect();
        let note_offs: Vec<u8> = records
            .iter()
            .filter(|r| r.bytes[0] == (MIDI_NOTE_OFF | 9))
            .map(|r| r.bytes[1])
            .collect();
        assert_eq!(notes, vec![38, 37, 38, 37]);
        assert_eq!(note_offs, vec![38, 37, 38, 37]);
    }
```

<!-- gh-id: 3143031933 -->
### Copilot on [`crates/core/src/machine.rs:70`](https://github.com/cmk/agogo/pull/21#discussion_r3143031933) (2026-04-26 05:29 UTC)

`bar_counters` / `click_counters` are indexed in lock-step with `self.channels`, but `Machine.channels` is a public `Vec`. If any caller mutates `machine.channels` after construction (push/remove/reorder), `on_buffer` can panic due to out-of-bounds indexing or silently associate counters with the wrong channel.

Consider making `channels` private (or at least `pub(crate)`), or adding a guard that enforces/repairs the invariant (e.g., a debug assertion that lengths match, or a non-allocating early error) so misuse fails fast with a clearer message.

<!-- gh-id: 3143039914 -->
#### ↳ cmk ([2026-04-26 05:38 UTC](https://github.com/cmk/agogo/pull/21#discussion_r3143039914))

Fixed — module docstring now leads with the divider-agnostic semantics ("keep every Nth scheduled event") and demotes `div=t1` to the "idiomatic case." Adopted your suggested phrasing nearly verbatim.

<!-- gh-id: 3143039963 -->
#### ↳ cmk ([2026-04-26 05:38 UTC](https://github.com/cmk/agogo/pull/21#discussion_r3143039963))

Fixed — replaced the dummy const with a real Note Off assertion in `bars_and_accent_compose_correctly`, mirroring the accent pattern through both the Note On and Note Off streams.

<!-- gh-id: 3143040004 -->
#### ↳ cmk ([2026-04-26 05:38 UTC](https://github.com/cmk/agogo/pull/21#discussion_r3143040004))

Fixed — `channels` is now `pub(crate)` so external mutation can't desync the parallel `bar_counters`/`click_counters` vecs. Confirmed no external `machine.channels` access exists today; the field's doc comment now spells out the lockstep invariant.
