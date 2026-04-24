# PR #14 — Plan 12: `out/midi` — MidiSink trait + MIDI clock byte emission

## Summary

First of the three remaining v0.1 output-chain sprints. Adds
pure-logic MIDI clock rendering: the `MidiSink` trait contract (per
`doc/agogo.md` §5), a synthetic `TestSink` for unit/proptest use,
and a byte-emission pipeline that maps per-channel `ScheduledEvent`
slices to `MidiSink::send_at(&[0xF8], at_sample)` calls.

### What ships

- **`crates/core/src/out/midi.rs`** — the new module:
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
PhaseSourceStop }` — whose transitions the Machine maps down to
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
  to `t32t` for the same reason. `output.md:37-41` is the design
  source.
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
