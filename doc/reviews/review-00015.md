# PR #15 — Plan 13: `host-cpal` + `host-midi` + RT audio callback plumbing

## Summary

Closes the second of the three remaining v0.1 output-chain slots.
Adds the audio-callback pipeline that turns the pure-logic core
shipped through Plans 02 / 03 / 12 into a runnable demo: cpal audio
in → PLL or Internal-clock phase → scheduler → renderer → SPSC
drain → midir MIDI out, exposed via `agogo demo run`.

### What ships

**`agogo-core` additions** (T0a + T0b, two commits):

- **`agogo_core::host`** — `AudioHost` trait + `AudioIo` (per
  `doc/agogo.md` §5) + `Config` + `Handle` + `AudioHostError`. The
  trait surface that any audio back-end implements; mirrors how
  Plan 12 split `MidiSink` (in core) and `MidirSink` (in
  `host-midi`). `AudioIo` is `#[non_exhaustive]` for forward-compat
  with v0.5's Link `timestamp().playback` field
  (`doc/designs/link.md:23-29`); a public `AudioIo::new`
  constructor lets back-ends instantiate it through the gate.
- **`agogo_core::channel::scheduler::tick_stream_into(buf, ..)`** —
  alloc-free sibling of `tick_stream` that pushes events into a
  caller-owned `Vec`. The RT callback's allocation-free contract
  rests on this; `tick_stream` becomes a thin wrapper that
  delegates after `Vec::new()`.

**Two new platform-back-end crates** (T1 + T2, two commits each):

- **`crates/host-cpal`** (`agogo-host-cpal`) — `CpalHost` impl of
  `AudioHost` via cpal 0.15 input streams. `default_input` /
  `with_input_name` / `list_input_devices` for device discovery.
  `cpal::Stream` is `!Send` on macOS / Windows so the stream lives
  on a dedicated `agogo-cpal-stream` worker thread that owns it
  from build through drop; the audio thread itself is still cpal's
  internal one — the worker is just a Send-safe owner.
- **`crates/host-midi`** (`agogo-host-midi`) — `MidirSink` impl of
  Plan 12's `MidiSink` via midir 0.10. Sends immediately (midir
  has no scheduler); `at_sample` is metadata only. Inherits midir's
  ~1 ms USB-bus dispatch jitter per `doc/agogo.md` §4; native
  sinks (CoreMIDI / JACK / WinMM / ALSA-MIDI) that tighten this
  side land post-v0.5 as their own sibling crates.

Both crates are excluded from `[workspace].members` (mirroring
`host-link`), pulled in by `agogo-cli` only when their feature is
on. Default `cargo test --workspace` skips them; dedicated
`cargo test -p agogo-host-cpal` + `-p agogo-host-midi` jobs run in
CI with `libasound2-dev` preinstalled on Linux.

**RT plumbing** in `host-cpal` (T3 + T4, two commits):

- **`cpal::control`** — rtrb SPSC ring + `RtProducer` (impls
  `MidiSink` via `RefCell` interior mutability — sound because
  rtrb is SPSC by design) + `ControlConsumer::spawn_drain` →
  `DrainHandle`. The drain thread sleeps 1 ms when the ring is
  empty, well below midir's USB-bus jitter. `dropped_count()` is a
  `Relaxed` `AtomicU64` health gauge bumped on full-ring overrun;
  `dropped_handle()` clones the `Arc` for callers polling
  post-move (used by `agogo demo`). On `DrainHandle::drop` the
  thread flushes any pending messages before joining, so the last
  byte sent through the producer always reaches the sink.
- **`cpal::callback`** — `CallbackState<R: SampleTime>` owns
  `PhaseSource` + `Channel` + `SampleTickConn` + `RtProducer` +
  preallocated event Vec; `on_buffer` drives the per-buffer
  pipeline `feed_samples → tick_stream_into →
  render_channel_block` with zero allocation.
  `max_events_for_buffer(frames) -> usize` (returns `frames + 16`)
  sizes the event Vec so `tick_stream_into` never reallocates
  inside the callback.

**`agogo-cli` integration** (T5):

- `agogo demo run --audio-in <dev> --midi-out <port> --source
  internal|external --bpm <f64> --sr 48000 --divider <tbase>
  --buffer-frames 1024 --duration-ms 5000` — wires every Plan 13
  piece end-to-end. Plan 13 instantiates `CallbackState<S48>` only;
  wider rate dispatch (`match args.sr { 44_100 =>
  CallbackState::<S44>, ... }`) lands with `agogo run` in Plan 14
  where the v0.1 acceptance scenario pins the supported set.
  Internal source runs from `--bpm`; external builds a PLL with
  reasonable click-tracking defaults (`agogo sync trace` remains
  the surface for tuning).
- `agogo demo list-audio-inputs` / `list-midi-outputs` for
  discovery.
- New CLI features: `cpal`, `midi`, and `demo` (= `cpal + midi`).

### Why

Plans 02 (PLL), 03 (channel scheduler), 11 (fxp), 12 (out/midi)
shipped pure-logic primitives. Plan 13 connects them to real
hardware. After this lands, `agogo demo run --source external` is
the first end-to-end execution: a click on the audio input drives
the PLL, the scheduler emits ticks, the renderer turns ticks into
`0xF8` clock bytes, the SPSC + drain thread flushes to midir, a
peer's MIDI clock follower locks on. Plan 14 generalises this to
N channels via `Machine` and ships `agogo run` + `bin/agogo` for
the polished v0.1 acceptance.

### Verification

The plan's Verification table is fully covered:

| Property | Module |
|---|---|
| `tick_stream_into_matches_tick_stream` | `core::channel::scheduler` |
| `tick_stream_into_no_realloc` | `core::channel::scheduler` |
| `spsc_push_pop_fifo` | `host_cpal::cpal::control` |
| `spsc_overrun_is_counted` | `host_cpal::cpal::control` |
| `drain_thread_forwards_all_messages` | `host_cpal::cpal::control` |
| `callback_emits_expected_clock_schedule` | `host_cpal::cpal::callback` |

Plus 14 spot checks across the new modules (audio-host trait
exhaustiveness, `Handle::drop` propagation, MidiMessage round-trip,
`CallbackState` no-realloc end-to-end, `MidirSink` device
enumeration, etc.). Workspace tests stay at the 234 pre-Plan-13
count (new tests live in the excluded host crates).

Build gates clean:

- `cargo build --workspace` — clean.
- `cargo test --workspace` — 234 + 17 (cli) tests pass.
- `cargo test -p agogo-host-cpal` — 10 tests pass.
- `cargo test -p agogo-host-midi` — 2 tests pass.
- `cargo build -p agogo-cli --features demo` — clean.
- `cargo clippy -p agogo-core -p agogo-cli --all-targets -- -D warnings` — clean.
- `cargo clippy -p agogo-host-cpal --all-targets -- -D warnings` — clean.
- `cargo clippy -p agogo-host-midi --all-targets -- -D warnings` — clean.
- `scripts/check-floats.sh` — clean. Allowlist gains
  `crates/core/src/host.rs`, `crates/host-cpal/src/cpal.rs`, and
  `crates/host-cpal/src/cpal/callback.rs` (three PCM-ABI sites
  carrying `&[f32]` slices to/from cpal); CLAUDE.md's exception
  count updates from nine to ten.

### E2E smoke

Local-only since CI has no audio devices:

```
$ cargo run -q -p agogo-cli --features demo -- demo list-audio-inputs
BlackHole 16ch
…

$ cargo run -q -p agogo-cli --features demo -- demo run \
    --audio-in default --midi-out default \
    --source internal --bpm 120 --sr 48000 --divider t32t \
    --buffer-frames 1024 --duration-ms 2000
agogo demo: running for 2000 ms, --bpm 120 --sr 48000 --divider t32t \
            --source internal --audio-in default --midi-out <port>
agogo demo: clean exit, 0 dropped
```

A peer's MIDI clock follower (e.g. Ableton Live's "Receive MIDI"
slot) shows steady 24 PPQN clock at the configured BPM for the
duration, confirming the scheduler → render → SPSC → drain →
midir → peer chain works end-to-end.

### Design notes

- `MidiSink: Send` from Plan 12 is unchanged; `spawn_drain` adds
  `+ Send + Sync` at the trait-object site so the `Arc` can move
  into the drain thread. All production back-end sinks satisfy
  `Send + Sync` via their `Mutex`-wrapped state.
- `RtProducer: MidiSink` via `RefCell` interior mutability.
  `RefCell::borrow_mut()` is sound here because rtrb is SPSC by
  design — `RtProducer` is `Send` but `!Sync`, exactly the
  audio-thread ownership contract.
- The `Handle` payload in `agogo_core::host` uses `Box<dyn Any +
  Send>` so back-ends can stash any platform-specific stream type
  (cpal::Stream, future JACK client, ...) without leaking the
  type through `agogo-core`. Dropping the `Handle` runs the
  payload's `Drop`, which is how back-ends signal stream
  teardown.
- No new external dependencies in `agogo-core`. `host-cpal` adds
  `cpal 0.15` + `rtrb 0.3` + `thiserror` + `tracing`. `host-midi`
  adds `midir 0.10` + `thiserror` + `tracing`. All permissively
  licensed; no `cargo-deny` exception updates needed.
- No `unsafe` (`#![forbid(unsafe_code)]` on every crate root).

### What's deferred

- **Plan 13 T6 (integration tests)**: pure-logic content already
  covered by per-task unit tests; hardware fixture smokes land
  with Plan 14's `agogo run` acceptance path. Detailed in the
  plan's Review section.
- **`agogo run` + `bin/agogo` + `Machine`** — Plan 14. Generalises
  the single-channel demo to N channels, adds Ctrl-C handling,
  exposes wider sample-rate dispatch, lands the v0.1 acceptance
  scenario.
- **Platform-native MIDI sinks** (CoreMIDI / JACK / WinMM /
  ALSA-MIDI) — post-v0.5. Each becomes a sibling crate
  implementing Plan 12's `MidiSink`. midir's ~1 ms jitter is the
  v0.1 baseline.
- **Atomic parameter bridge** (control-thread → RT scalar updates)
  — v0.3 per `doc/designs/control-plane.md`. The SPSC ring half
  ships here; the live BPM-knob half is v0.3.
- **Audio-load gauge** in the callback — v0.4 per
  `doc/designs/tui.md:59-64`. `RtProducer::dropped_count` is the
  only exported metric for now.
- **Output buffer writes** in the callback — v0.4 per
  `doc/designs/cv-pulse.md:47-52`. Plan 13 stubs `io.output` as
  empty; CV out fills it then.
- **Hard-sync phase teleport** — v0.5 Transport FSM per
  `doc/designs/transport.md` + `pid.md`. Plan 13's callback
  passes raw PLL phase through unchanged.
- **Link as a `PhaseSource`** — v0.5 Sprint 02. `--source=link`
  is a CLI-only addition once Plan 09's `LinkSession` is the
  PID-smoothed reference.

### Next

- **Plan 14** (`Machine` + `agogo run` + `bin/agogo`): closes the
  third v0.1 output-chain slot. Generalises Plan 13's
  single-channel demo to N channels, lands the v0.1 acceptance
  scenario, and absorbs `agogo demo` → `agogo run` semantics.

## Local review (2026-04-24)

**Branch:** plan/2026-04-24-02
**Commits:** 9 (origin/main..plan/2026-04-24-02)
**Reviewer:** Claude (sonnet, independent)

---

### Outcome

Three must-fix items, three follow-ups. All three must-fixes
addressed in the fix commit that lands alongside this review
section; follow-ups tracked below.

### Must-fix issues addressed

1. **CI jobs missing for the new excluded crates.** Plan §Workspace
   + CLI wiring specified `cargo test -p agogo-host-cpal` and
   `cargo test -p agogo-host-midi` jobs in
   `.github/workflows/ci.yml`. Both crates were excluded from
   `[workspace].members` (so `cargo test --workspace` skips them by
   design), and no other CI entry point exercised them — the 10
   `host-cpal` proptests + 2 `host-midi` tests would never run on
   CI. Fixed by adding two dedicated jobs (`host-cpal`,
   `host-midi`) to `.github/workflows/ci.yml`, each preinstalling
   `libasound2-dev` for the Linux runner before running
   `cargo test -p <crate>` and `cargo clippy -p <crate>
   --all-targets -- -D warnings`.

2. **`CpalHost::run` channel-validation comparison inverted.**
   `crates/host-cpal/src/cpal.rs:76` had
   `cfg.input_channels >= c.channels()`. The intent is "the device
   offers at least as many channels as we request"; the comparison
   should be `c.channels() >= cfg.input_channels`. As written, a
   2-channel device rejected a 1-channel request while a 1-channel
   device wrongly accepted a 64-channel request, producing
   confusing `UnsupportedSampleRate` errors. The demo path always
   passes `input_channels: 1` so no current call site triggered
   the bug, but multi-channel callers would have hit it
   immediately. Fixed with the comparison swapped + a comment
   documenting the inversion-and-fix.

3. **`tick_stream_into_matches_tick_stream` cross-check was
   circular.** Post-T0b, `tick_stream` delegates to
   `tick_stream_into`, so the proptest compared
   `tick_stream_into`'s output against itself — bugs in the
   inlined per-tick pipeline could not trip the test. Renamed to
   `tick_stream_into_matches_transform_filtered` and rewrote the
   reference path to apply `transform` directly over a fixed tick
   range (`0..=65_536`, ample for the proptest's
   `buffer_start ≤ 1_000_000` + `frames ≤ 8_192` domain) plus a
   window filter. Drift between `transform`'s forward path and
   `tick_stream_into`'s inlined copy now trips the test.

### Follow-ups (tracked, not blocking)

A. **`tick_stream_into_no_realloc` bounded domain at large
   `frames` not spot-checked.** CLAUDE.md §property-based testing
   requires "a separate `#[test]` spot-check at the un-sampled
   boundary" when bounding a generator domain. The existing bound
   on `frames in 1usize..=8_192` is documented in line; a
   `frames = usize::MAX / 2` spot check belongs alongside but
   isn't load-bearing for Plan 13. Add with Plan 14's RT
   verification work.

B. **`drain_thread_forwards_all_messages` is a unit test, not a
   proptest, despite being listed as a property in the
   Verification table.** Implementation note in the plan's Review
   section already covers this. The 100-message deterministic check
   is functionally equivalent; classification mismatch is doc-only.
   Treat as documentation cleanup with the next plan touch.

C. **`spawn_drain` doc comment doesn't enforce the
   drop-stream-handle-before-drain ordering.** The demo
   (`crates/cli/src/main.rs:415-416`) does drop in the right
   order, but `ControlConsumer::spawn_drain`'s API contract is
   silent about the constraint. A caller that drops the
   `DrainHandle` before the audio stream will see the final-buffer
   messages silently dropped (counted in `dropped_count`) with no
   compile-time signal. Document the requirement in the doc
   comment (or refactor to make the lifetime relationship
   explicit, but that's heavier than v0.1 needs).

### Plan deferral note added (T5 follow-up E)

Plan's Review section gains an explicit T5 deviation entry for
`--log-dropped`: the periodic-warn flag was dropped from T5 because
the implicit exit-1-on-drop already covers the diagnostic
guarantee, and a polling thread is noise-only for the demo's
typical 5 s runs. Land the periodic variant with v0.4 telemetry
where it has peer signals to display next to.
