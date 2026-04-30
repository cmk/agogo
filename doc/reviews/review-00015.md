# PR #15 — Plan 13: `host-cpal` + `host-midi` + RT audio callback plumbing

## Summary

Closes the second of the three remaining v0.1 output-chain slots.
Adds the audio-callback pipeline that turns the pure-logic core
shipped through Plans 02 / 03 / 12 into a runnable demo: cpal audio
in → PLL or Internal-clock phase → scheduler → renderer → SPSC
drain → midir MIDI out, exposed via `agogo demo run`.

### What ships

**`agogo-core` additions** (T0a + T0b, two commits):

- **`agogo_core::sink::audio`** — `AudioHost` trait + `AudioIo` (per
  `doc/agogo.md` §5) + `Config` + `Handle` + `AudioHostError`. The
  trait surface that any audio back-end implements; mirrors how
  Plan 12 split `MidiSink` (in core) and `MidirSink` (in
  `host-midi`). `AudioIo` is `#[non_exhaustive]` for forward-compat
  with v0.5's Link `timestamp().playback` field
  (`doc/designs/link.md:23-29`); a public `AudioIo::new`
  constructor lets back-ends instantiate it through the gate.
- **`agogo_core::control::event::tick_stream_into(buf, ..)`** —
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
  `crates/core/src/sink/audio.rs`, `crates/host-cpal/src/cpal.rs`, and
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
- The `Handle` payload in `agogo_core::sink::audio` uses `Box<dyn Any +
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

   **Round 2 correction** (Copilot gh-id 3141035633 / 3141035679):
   `-p <crate>` does not resolve crates that are excluded from
   `[workspace].members`. The round-1 CI jobs above would have
   errored with "package ID specification ... did not match any
   packages". The round-2 fix commit replaces `-p` with
   `defaults.run.working-directory: crates/host-<name>` so cargo
   runs from the crate's own directory.

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

<!-- gh-id: 3140987404 -->
### Copilot on [`crates/cli/src/main.rs:63`](https://github.com/cmk/agogo/pull/15#discussion_r3140987404) (2026-04-25 00:55 UTC)

The CLI help text says `agogo demo run` runs for `--duration-ms` “(or until Ctrl-C)”, but the current implementation always sleeps for the fixed duration and explicitly defers Ctrl-C handling to Plan 14. Please update the docstring so `--help` output matches actual behavior.


<!-- gh-id: 3140987422 -->
### Copilot on [`doc/reviews/review-00015.md:244`](https://github.com/cmk/agogo/pull/15#discussion_r3140987422) (2026-04-25 00:55 UTC)

This review text references adding CI jobs that run `cargo test -p <crate>` / `cargo clippy -p <crate>`, but `host-cpal`/`host-midi` are not workspace members in this PR, so `-p` from the repo root won’t work. The CI jobs (and this doc) should use `--manifest-path crates/host-*/Cargo.toml` (or run cargo from the crate directories) to align with the “detached crate” setup.


<!-- gh-id: 3140987431 -->
### Copilot on [`doc/plans/plan-2026-04-24-02.md:66`](https://github.com/cmk/agogo/pull/15#discussion_r3140987431) (2026-04-25 00:55 UTC)

This plan’s workspace/CI wiring details don’t match the actual “detached crate” setup in this PR:
- It says CI runs `cargo test -p agogo-host-cpal` / `-p agogo-host-midi` (line 65), but these crates are not `[workspace].members`, so `-p` from the repo root won’t resolve them; CI should use `--manifest-path crates/host-*/Cargo.toml` (or run cargo from those directories).
- The `crates/cli/Cargo.toml` feature snippet uses `midir`/`demo = ["cpal", "midir"]` (lines ~127-131), but the implemented feature name is `midi`.
- The dependency snippets for detached crates use `{ workspace = true }` (e.g. lines ~113-115), which won’t work when building a detached crate standalone.

Please update the plan so readers can reproduce the build/test steps and feature flags as implemented.


<!-- gh-id: 3140987439 -->
### Copilot on [`crates/host-cpal/src/cpal/control.rs:56`](https://github.com/cmk/agogo/pull/15#discussion_r3140987439) (2026-04-25 00:55 UTC)

`MidiMessage::from_slice` silently truncates messages longer than 3 bytes in release builds, which can turn a programmer error into corrupted MIDI output. Additionally, `MidiMessage` has public fields and `as_slice()` will panic if `bytes_len > 3`. To make the API robust, consider enforcing the invariant (e.g., make fields private and validate in constructors, or clamp/return an empty slice + bump `dropped` on invalid length).

<!-- gh-id: 3140987448 -->
### Copilot on [`.github/workflows/ci.yml:81`](https://github.com/cmk/agogo/pull/15#discussion_r3140987448) (2026-04-25 00:55 UTC)

Same issue as `host-cpal`: `cargo test -p agogo-host-midi` / `cargo clippy -p ...` won’t resolve because `agogo-host-midi` is not a workspace member. Use `--manifest-path crates/host-midi/Cargo.toml` (or set the job’s working directory) so CI actually runs the detached crate’s tests.


<!-- gh-id: 3140987456 -->
### Copilot on [`crates/host-cpal/README.md:20`](https://github.com/cmk/agogo/pull/15#discussion_r3140987456) (2026-04-25 00:55 UTC)

This README’s suggested commands (`cargo build/test -p agogo-host-cpal`) won’t work from the repo root because `agogo-host-cpal` is intentionally not a workspace member. Use `--manifest-path crates/host-cpal/Cargo.toml` (or run cargo from `crates/host-cpal/`). Also, the README references a `cpal_default_input_smoke` test/fixture that doesn’t appear to exist in this crate right now, so the instructions are currently not actionable.

<!-- gh-id: 3140987466 -->
### Copilot on [`crates/host-midi/README.md:22`](https://github.com/cmk/agogo/pull/15#discussion_r3140987466) (2026-04-25 00:55 UTC)

Same as `host-cpal`: `cargo build/test -p agogo-host-midi` won’t work from the repo root because this crate is not a workspace member. Use `--manifest-path crates/host-midi/Cargo.toml` (or run cargo from `crates/host-midi/`). Additionally, the README describes a `midir_loopback_roundtrip` hardware test, but there’s no such test in the crate yet (Plan 13 T6 is noted as deferred), so this section should be updated.

<!-- gh-id: 3140987484 -->
### Copilot on [`crates/core/src/sink/audio.rs:57`](https://github.com/cmk/agogo/pull/15#discussion_r3140987484) (2026-04-25 00:55 UTC)

`AudioIo` exposes `frames` (“samples per channel”) but does not expose `input_channels` / `output_channels` (or otherwise define whether `input`/`output` are mono vs interleaved). Since `Config` *does* carry channel counts (and `host-cpal` can deliver interleaved buffers), downstream callbacks can’t reliably interpret `input` without inferring layout from `len/frames`, and multi-channel support is effectively underspecified. Consider either (a) adding explicit channel-count fields (and documenting interleaving), or (b) documenting/enforcing that Plan 13’s `AudioIo` is mono-only (`*_channels == 1`) until v0.4/v0.5 expands it.

<!-- gh-id: 3140987493 -->
### Copilot on [`.github/workflows/ci.yml:68`](https://github.com/cmk/agogo/pull/15#discussion_r3140987493) (2026-04-25 00:55 UTC)

`cargo test -p agogo-host-cpal` / `cargo clippy -p ...` will fail because `agogo-host-cpal` is not a `[workspace].members` package (it’s intentionally detached). In CI you’ll need to run via `--manifest-path crates/host-cpal/Cargo.toml` (or `working-directory: crates/host-cpal`) instead of `-p` so the job can actually exercise the crate’s tests/clippy.


<!-- gh-id: 3140987498 -->
### Copilot on [`crates/core/src/control/event.rs:47`](https://github.com/cmk/agogo/pull/15#discussion_r3140987498) (2026-04-25 00:55 UTC)

Doc comment references `max_events_for_buffer(channel, frames)`, but no such function exists in this module (and the caller-side helper lives in `host-cpal::cpal::callback`). Please adjust the docs to avoid pointing at a nonexistent API and clarify what capacity bound callers should actually use to guarantee `tick_stream_into` stays allocation-free.


<!-- gh-id: 3140987506 -->
### Copilot on [`crates/core/src/control/event.rs:102`](https://github.com/cmk/agogo/pull/15#discussion_r3140987506) (2026-04-25 00:55 UTC)

This comment still references a `tick_stream_into_matches_tick_stream` proptest, but the test in this file is now named `tick_stream_into_matches_transform_filtered`. Updating the comment would keep the “change either path’s arithmetic…” guidance accurate for future maintainers.


<!-- gh-id: 4174304385 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-04-25 00:55 UTC](https://github.com/cmk/agogo/pull/15#pullrequestreview-4174304385))

## Pull request overview

This PR implements Plan 13’s first runnable end-to-end demo pipeline by introducing an `AudioHost` trait in `agogo-core`, adding detached platform backends for cpal audio input and midir MIDI output, and wiring them into `agogo demo` behind feature flags. It also updates CI/docs and repository conventions to account for the new detached crates.

**Changes:**
- Added `agogo_core::sink::audio` (`AudioHost`, `AudioIo`, `Config`, `Handle`, `AudioHostError`) and an allocation-free scheduler API (`tick_stream_into`).
- Introduced detached backend crates `agogo-host-cpal` (cpal stream + RT callback + rtrb SPSC/drain thread) and `agogo-host-midi` (midir `MidiSink`).
- Integrated a feature-gated `agogo demo` CLI and updated float allowlisting/docs/CI metadata accordingly.

### Reviewed changes

Copilot reviewed 21 out of 23 changed files in this pull request and generated 14 comments.

<details>
<summary>Show a summary per file</summary>

| File | Description |
| ---- | ----------- |
| scripts/check-floats.sh | Extends the float allowlist to new PCM-ABI files. |
| doc/reviews/review-00015.md | Adds Plan 13 review record and verification narrative. |
| doc/plans/plan-2026-04-24-02.md | Adds Plan 13 plan doc describing architecture, tasks, and verification. |
| crates/host-midi/src/midir.rs | Implements `MidirSink` using midir, with basic tests. |
| crates/host-midi/src/lib.rs | Host-midi crate root + re-exports. |
| crates/host-midi/README.md | Dev workflow and (currently inaccurate) test instructions. |
| crates/host-midi/Cargo.toml | New detached crate manifest for `agogo-host-midi`. |
| crates/host-cpal/src/lib.rs | Host-cpal crate root + docs and re-export. |
| crates/host-cpal/src/cpal/control.rs | rtrb SPSC producer/consumer + drain thread + tests. |
| crates/host-cpal/src/cpal/callback.rs | `CallbackState` hot loop + event preallocation helper + tests. |
| crates/host-cpal/src/cpal.rs | `CpalHost` implementation of `AudioHost` with dedicated owner thread. |
| crates/host-cpal/README.md | Dev workflow and (currently inaccurate) smoke test instructions. |
| crates/host-cpal/Cargo.toml | New detached crate manifest for `agogo-host-cpal`. |
| crates/core/src/lib.rs | Exposes new `host` module. |
| crates/core/src/sink/audio.rs | Defines `AudioHost` API and `AudioIo` callback payload. |
| crates/core/src/control/event.rs | Adds `tick_stream_into` and refactors `tick_stream` to delegate. |
| crates/cli/src/main.rs | Adds `agogo demo` subcommand and end-to-end wiring module. |
| crates/cli/Cargo.toml | Adds optional path deps + features (`cpal`, `midi`, `demo`). |
| Cargo.toml | Excludes new detached host crates from the workspace. |
| CLAUDE.md | Updates float-exception module count/notes. |
| .gitignore | Ignores detached crates’ per-crate `Cargo.lock` files. |
| .github/workflows/ci.yml | Adds CI jobs intended to test/clippy detached host crates. |
</details>






<!-- gh-id: 3140987513 -->
### Copilot on [`crates/host-cpal/src/cpal.rs:82`](https://github.com/cmk/agogo/pull/15#discussion_r3140987513) (2026-04-25 00:55 UTC)

The `supported_input_configs()` check uses `c.channels() >= cfg.input_channels`, but `SupportedStreamConfigRange::channels()` is a *fixed* channel count in cpal; requesting fewer channels than the supported config usually isn’t allowed. As written, this can report “supported” and then still fail `build_input_stream`, and the comment about slicing leading channels isn’t implemented. Either require `c.channels() == cfg.input_channels`, or actually build the stream with the device’s channel count and down-mix/slice in the callback.


<!-- gh-id: 3140987516 -->
### Copilot on [`crates/host-cpal/src/cpal.rs:128`](https://github.com/cmk/agogo/pull/15#discussion_r3140987516) (2026-04-25 00:55 UTC)

The callback passes the raw interleaved `samples` slice straight through `AudioIo::input`. `PhaseSource::feed_samples`/`PeakDetector::process` treats this as mono PCM, so any `input_channels > 1` stream will produce incorrect detection/PLL behavior. Consider enforcing `cfg.input_channels == 1` for now, or explicitly deinterleave/down-mix (and document the channel layout in `AudioIo`). Also, `channels.max(1)` hides an invalid `input_channels = 0` config instead of rejecting it early.

<!-- gh-id: 3140987521 -->
### Copilot on [`crates/host-cpal/src/cpal/control.rs:178`](https://github.com/cmk/agogo/pull/15#discussion_r3140987521) (2026-04-25 00:55 UTC)

`spawn_drain`’s shutdown semantics depend on drop order: if a caller drops `DrainHandle` while the audio stream is still producing, messages pushed after the stop signal will be silently lost. Since this is an easy footgun for downstream callers, please document the required ordering (drop/stop the stream/producer before dropping `DrainHandle`), or refactor the API to make the lifetime relationship explicit.

<!-- gh-id: 3141025364 -->
#### ↳ cmk ([2026-04-25 01:17 UTC](https://github.com/cmk/agogo/pull/15#discussion_r3141025364))

Fixed — `--duration-ms` docstring no longer claims "(or until Ctrl-C)"; explicit note that Plan 13 has no signal handler and Plan 14's `agogo run` adds it.

<!-- gh-id: 3141025427 -->
#### ↳ cmk ([2026-04-25 01:17 UTC](https://github.com/cmk/agogo/pull/15#discussion_r3141025427))

Fixed in this round's plan doc + CI workflow updates: CI now uses `working-directory: crates/host-<name>` instead of `-p`. The local-review section's original text stays as a historical record of round 1's intent (which itself was wrong, as you flagged).

<!-- gh-id: 3141025470 -->
#### ↳ cmk ([2026-04-25 01:17 UTC](https://github.com/cmk/agogo/pull/15#discussion_r3141025470))

All three fixed in `doc/plans/plan-2026-04-24-02.md`: CI snippet now uses `working-directory: crates/host-<name>`; feature name `midir` → `midi` to match the implementation; detached-crate Cargo.toml snippets now use literal versions (`thiserror = "2"` etc.) since detached crates can't inherit `{ workspace = true }`.

<!-- gh-id: 3141025525 -->
#### ↳ cmk ([2026-04-25 01:17 UTC](https://github.com/cmk/agogo/pull/15#discussion_r3141025525))

Partial fix — `as_slice` now clamps `bytes_len` with `min(3)` so an out-of-range value returns the full 3-byte view rather than panicking on out-of-bounds indexing. Keeping fields public for now: routing every `arb_msg` proptest call site through a constructor adds API surface for a Plan 12-bounded `≤ 3 bytes per send_at` invariant that the existing `from_slice` `debug_assert!` already enforces. Will revisit if Plan 14's MidiCc rendering surfaces a real >3-byte caller.

<!-- gh-id: 3141025605 -->
#### ↳ cmk ([2026-04-25 01:17 UTC](https://github.com/cmk/agogo/pull/15#discussion_r3141025605))

Fixed — `host-midi` job switches to `defaults.run.working-directory: crates/host-midi` so `cargo test` / `cargo clippy` resolve the detached crate. Verified locally that `cargo test -p agogo-host-midi` from the workspace root errors with "package ID specification ... did not match any packages".

<!-- gh-id: 3141025650 -->
#### ↳ cmk ([2026-04-25 01:17 UTC](https://github.com/cmk/agogo/pull/15#discussion_r3141025650))

Fixed — README now uses `cargo build --manifest-path crates/host-cpal/Cargo.toml` (with a `cd` alternative). The stale `cpal_default_input_smoke` reference is replaced with a pointer to Plan 14, which lands the hardware fixtures alongside `agogo run`'s acceptance scenario per Plan 13's Review §T6 deferral.

<!-- gh-id: 3141025712 -->
#### ↳ cmk ([2026-04-25 01:17 UTC](https://github.com/cmk/agogo/pull/15#discussion_r3141025712))

Fixed — same as `host-cpal` README: `--manifest-path crates/host-midi/Cargo.toml` + `cd` alternative; stale `midir_loopback_roundtrip` reference replaced with a pointer to Plan 14 (T6 deferred per the plan's Review section). Local-setup notes (IAC bus / virmidi / loopMIDI) stay since they'll apply unchanged when Plan 14 lands the test.

<!-- gh-id: 3141025822 -->
#### ↳ cmk ([2026-04-25 01:17 UTC](https://github.com/cmk/agogo/pull/15#discussion_r3141025822))

Picked option (b) — `AudioIo` doc now pins Plan 13 to mono explicitly, and `CpalHost::run` enforces `cfg.input_channels == 1` at entry (errors with a clear message otherwise). v0.4's `out/audio` work is the right place to add explicit `input_channels` / `output_channels` fields and switch the buffers to interleaved frames; Plan 13's mono-only path is sound until then.

<!-- gh-id: 3141025889 -->
#### ↳ cmk ([2026-04-25 01:17 UTC](https://github.com/cmk/agogo/pull/15#discussion_r3141025889))

Fixed in the same change as the `host-midi` job — `host-cpal` job now uses `defaults.run.working-directory: crates/host-cpal`. Verified locally that `cargo test -p agogo-host-cpal` errors with "package ID specification ... did not match any packages" from the workspace root.

<!-- gh-id: 3141025968 -->
#### ↳ cmk ([2026-04-25 01:17 UTC](https://github.com/cmk/agogo/pull/15#discussion_r3141025968))

Fixed — `tick_stream_into`'s doc comment no longer references the nonexistent `max_events_for_buffer(channel, frames)` signature; it now describes the no-realloc contract qualitatively and points at the actual helper at `agogo_host_cpal::cpal::callback::max_events_for_buffer(frames)` (which returns `frames + 16`).

<!-- gh-id: 3141026027 -->
#### ↳ cmk ([2026-04-25 01:17 UTC](https://github.com/cmk/agogo/pull/15#discussion_r3141026027))

Fixed — the inlined-pipeline comment now references `tick_stream_into_matches_transform_filtered` (the proptest's current name post-PR-15-round-1 rename); `scheduler_block_equivalence` reference is unchanged.

<!-- gh-id: 3141026089 -->
#### ↳ cmk ([2026-04-25 01:17 UTC](https://github.com/cmk/agogo/pull/15#discussion_r3141026089))

Fixed — comparison is now `c.channels() == cfg.input_channels`, paired with mono enforcement at `CpalHost::run` entry (`cfg.input_channels != 1` errors out). Removed the misleading "slicing leading channels" comment too — there's no slicing in Plan 13. Multi-channel input + explicit deinterleave/down-mix lands in v0.4 alongside the CV output side.

<!-- gh-id: 3141026158 -->
#### ↳ cmk ([2026-04-25 01:17 UTC](https://github.com/cmk/agogo/pull/15#discussion_r3141026158))

Fixed — `CpalHost::run` rejects `cfg.input_channels != 1` at entry, so the audio thread only ever sees mono PCM (matching what `PhaseSource::feed_samples` expects). The `samples.len() / channels.max(1)` runtime guard is dropped — `len == frames` post-mono-check, no zero-divisor path. `AudioIo`'s doc note now pins this for Plan 13.

<!-- gh-id: 3141026215 -->
#### ↳ cmk ([2026-04-25 01:17 UTC](https://github.com/cmk/agogo/pull/15#discussion_r3141026215))

Fixed — `spawn_drain`'s doc comment now spells out the drop ordering explicitly: drop the audio stream's `Handle` before the `DrainHandle`. The reverse order causes final-buffer messages to be counted as overruns instead of reaching the sink. `agogo demo run` already does this correctly; the doc just makes the requirement explicit for future API callers.

<!-- gh-id: 3141035612 -->
### Copilot on [`crates/host-cpal/src/cpal.rs:222`](https://github.com/cmk/agogo/pull/15#discussion_r3141035612) (2026-04-25 01:22 UTC)

This test module doc comment refers to “hardware smoke tests fixture-gate on `cpal_default_input`”, but there’s no such fixture-gated hardware test in this crate (no `cpal_default_input_smoke` or similar). This is misleading for maintainers/debugging; either add the referenced fixture test or update the comment to match what actually exists today.

<!-- gh-id: 3141035625 -->
### Copilot on [`doc/plans/plan-2026-04-24-02.md:68`](https://github.com/cmk/agogo/pull/15#discussion_r3141035625) (2026-04-25 01:22 UTC)

This plan text says CI/build gates run `cargo test -p agogo-host-cpal` / `-p agogo-host-midi` (and similarly `cargo build -p ...` later), but both crates are explicitly excluded from `[workspace].members` so `-p` from the repo root won’t resolve them. Please update the plan to use `--manifest-path crates/host-*/Cargo.toml` (or `working-directory: crates/host-*`) consistently wherever it documents build/test commands.


<!-- gh-id: 3141035633 -->
### Copilot on [`.github/workflows/ci.yml:81`](https://github.com/cmk/agogo/pull/15#discussion_r3141035633) (2026-04-25 01:22 UTC)

Same issue as `host-cpal`: this job uses `cargo test -p agogo-host-midi` / `cargo clippy -p ...`, but `agogo-host-midi` is excluded from the workspace, so `-p` won’t work from the repo root. Use `--manifest-path crates/host-midi/Cargo.toml` or set `working-directory: crates/host-midi` for these steps.

<!-- gh-id: 3141035639 -->
### Copilot on [`crates/host-cpal/README.md:20`](https://github.com/cmk/agogo/pull/15#discussion_r3141035639) (2026-04-25 01:22 UTC)

This README’s suggested commands (`cargo build/test -p agogo-host-cpal`) won’t work from the repo root because `agogo-host-cpal` is not a workspace member. Use `--manifest-path crates/host-cpal/Cargo.toml` (or `cd crates/host-cpal && cargo …`). Also, it references a `cpal_default_input_smoke` hardware test/fixture that doesn’t exist in this crate (no matches under `crates/host-cpal/**/*.rs`), so the instructions aren’t currently actionable.

<!-- gh-id: 3141035647 -->
### Copilot on [`crates/host-cpal/src/cpal.rs:83`](https://github.com/cmk/agogo/pull/15#discussion_r3141035647) (2026-04-25 01:22 UTC)

The input config validation is inconsistent with how cpal handles channel counts: `SupportedStreamConfigRange::channels()` is a fixed channel count, but the check here uses `c.channels() >= cfg.input_channels` and the comment claims “we slice the leading channels in the callback” (no such slicing exists). This can report “supported” and then still fail `build_input_stream`, and if `cfg.input_channels > 1` the callback will pass interleaved PCM to `AudioIo`/`PhaseSource::feed_samples` (which expects mono). Either enforce `cfg.input_channels == 1` (and require `c.channels() == 1`), or build the stream at the device’s channel count and explicitly down-mix/deinterleave before calling the callback.

<!-- gh-id: 3141035654 -->
### Copilot on [`crates/host-cpal/src/cpal/control.rs:56`](https://github.com/cmk/agogo/pull/15#discussion_r3141035654) (2026-04-25 01:22 UTC)

`MidiMessage::as_slice` can panic if `bytes_len > 3` (because `bytes_len` is public and is used directly as a slice bound). Even if current constructors/tests keep it in-range, this is a public type and it only takes one accidental `MidiMessage { bytes_len: 4, .. }` to crash the drain thread. Clamp `bytes_len` to 3 when slicing (or make the fields private and enforce the invariant in constructors).

<!-- gh-id: 3141035659 -->
### Copilot on [`crates/host-cpal/src/cpal/control.rs:170`](https://github.com/cmk/agogo/pull/15#discussion_r3141035659) (2026-04-25 01:22 UTC)

`spawn_drain`’s shutdown behavior depends on drop order (dropping the drain while the audio callback is still producing can lose final-buffer messages). The doc comment currently doesn’t state the required ordering; please document that callers should stop/drop the audio stream (producer) before dropping `DrainHandle` if they need flush-at-shutdown semantics.

<!-- gh-id: 3141035662 -->
### Copilot on [`crates/host-midi/src/midir.rs:117`](https://github.com/cmk/agogo/pull/15#discussion_r3141035662) (2026-04-25 01:22 UTC)

This comment says hardware loopback tests are fixture-gated on `midir_loopback`, but there’s no such hardware/fixture test in this crate right now (only the unit tests in this module). Please update the comment to avoid implying coverage that doesn’t exist yet, or add the corresponding fixture-gated test.


<!-- gh-id: 3141035670 -->
### Copilot on [`doc/reviews/review-00015.md:243`](https://github.com/cmk/agogo/pull/15#discussion_r3141035670) (2026-04-25 01:22 UTC)

This review record states that CI was fixed by adding jobs that run `cargo test -p agogo-host-cpal` / `-p agogo-host-midi`, but these crates are excluded from the workspace, so `-p` from the repo root does not work. If the intended fix is “dedicated jobs”, the commands should be recorded as `--manifest-path crates/host-*/Cargo.toml` or “run with working directory set to the crate”.


<!-- gh-id: 3141035679 -->
### Copilot on [`.github/workflows/ci.yml:68`](https://github.com/cmk/agogo/pull/15#discussion_r3141035679) (2026-04-25 01:22 UTC)

The `host-cpal` CI job runs `cargo test -p agogo-host-cpal` / `cargo clippy -p ...`, but `agogo-host-cpal` is excluded from `[workspace].members` (see root `Cargo.toml`), so `-p` from the repo root won’t resolve the package and this job will fail. Run cargo with `--manifest-path crates/host-cpal/Cargo.toml` or set the job’s working directory to `crates/host-cpal` before invoking `cargo test`/`clippy`.

<!-- gh-id: 4174378757 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-04-25 01:22 UTC](https://github.com/cmk/agogo/pull/15#pullrequestreview-4174378757))

## Pull request overview

Adds Plan 13’s cross-platform “host” layer and demo wiring: cpal audio input feeds the core scheduler/renderer, MIDI is dispatched via an RT-safe SPSC + drain thread into a midir-backed sink, exposed through `agogo demo`.

**Changes:**
- Introduces `agogo_core::sink::audio` (`AudioHost`, `AudioIo`, `Handle`, errors) and an allocation-free `tick_stream_into` scheduler path.
- Adds two detached backend crates: `agogo-host-cpal` (cpal input + RT callback/control plane) and `agogo-host-midi` (midir `MidiSink`).
- Integrates a feature-gated `agogo demo` CLI plus CI/docs/allowlists for the new detached crates.

### Reviewed changes

Copilot reviewed 21 out of 23 changed files in this pull request and generated 13 comments.

<details>
<summary>Show a summary per file</summary>

| File | Description |
| ---- | ----------- |
| scripts/check-floats.sh | Extends float allowlist for new PCM-ABI files. |
| doc/reviews/review-00015.md | Adds Plan 13 review record and verification notes. |
| doc/plans/plan-2026-04-24-02.md | Adds Plan 13 design/verification plan and wiring details. |
| crates/host-midi/src/midir.rs | Implements `MidirSink` over midir with port enumeration + unit tests. |
| crates/host-midi/src/lib.rs | Declares/exports the host-midi crate API. |
| crates/host-midi/README.md | Documents detached-crate workflow and intended loopback testing. |
| crates/host-midi/Cargo.toml | Adds detached crate manifest + deps (midir/thiserror/tracing). |
| crates/host-cpal/src/lib.rs | Declares/exports the host-cpal crate API. |
| crates/host-cpal/src/cpal/control.rs | Implements RT SPSC ring + drain thread for MIDI dispatch. |
| crates/host-cpal/src/cpal/callback.rs | Implements `CallbackState` hot loop + capacity sizing helper + tests. |
| crates/host-cpal/src/cpal.rs | Implements `CpalHost` as `AudioHost` with worker-thread stream ownership. |
| crates/host-cpal/README.md | Documents detached-crate workflow and intended hardware smoke testing. |
| crates/host-cpal/Cargo.toml | Adds detached crate manifest + deps (cpal/rtrb/thiserror/tracing). |
| crates/core/src/lib.rs | Exposes the new `host` module. |
| crates/core/src/sink/audio.rs | Defines `AudioHost` trait surface and callback payload types. |
| crates/core/src/control/event.rs | Adds `tick_stream_into` and refactors `tick_stream` to delegate. |
| crates/cli/src/main.rs | Adds feature-gated `agogo demo` subcommands and demo pipeline module. |
| crates/cli/Cargo.toml | Adds optional path deps + features (`cpal`, `midi`, `demo`). |
| Cargo.toml | Excludes new host crates from workspace members. |
| Cargo.lock | Locks new dependencies (cpal, midir, rtrb, etc.). |
| CLAUDE.md | Updates float-exception count and referenced exception files. |
| .gitignore | Ignores detached-crate `Cargo.lock` files for new host crates. |
| .github/workflows/ci.yml | Adds dedicated CI jobs intended to test/clippy the detached host crates. |
</details>






<!-- gh-id: 3141035684 -->
### Copilot on [`crates/host-midi/README.md:35`](https://github.com/cmk/agogo/pull/15#discussion_r3141035684) (2026-04-25 01:22 UTC)

This README’s build/test commands use `-p agogo-host-midi`, but `agogo-host-midi` is not a workspace member, so `-p` from the repo root won’t resolve. Use `--manifest-path crates/host-midi/Cargo.toml` (or run cargo from `crates/host-midi/`). Also, the `midir_loopback_roundtrip` hardware test mentioned here doesn’t exist yet (no matches under `crates/host-midi/**/*.rs`), so readers will not be able to run it as written.

<!-- gh-id: 3141035693 -->
### Copilot on [`crates/core/src/sink/audio.rs:58`](https://github.com/cmk/agogo/pull/15#discussion_r3141035693) (2026-04-25 01:22 UTC)

`AudioIo` exposes `frames` (“samples per channel”) but does not specify the channel layout or expose `input_channels`/`output_channels`. Since `Config` does carry channel counts (and cpal delivers interleaved buffers), this makes it ambiguous how callbacks should interpret `input`/`output`. Either document/enforce that Plan 13 is mono-only (e.g., `input_channels == 1`) or add explicit channel-count/layout fields to `AudioIo` so downstream code can safely handle multi-channel streams.

<!-- gh-id: 3141035697 -->
### Copilot on [`crates/cli/src/main.rs:65`](https://github.com/cmk/agogo/pull/15#discussion_r3141035697) (2026-04-25 01:22 UTC)

The `--help` text for `agogo demo run` says it runs “for `--duration-ms` ms (or until Ctrl-C)”, but the implementation just sleeps for the fixed duration and explicitly defers Ctrl-C handling. Please update this docstring so the CLI help matches actual behavior.


<!-- gh-id: 3141063403 -->
### Copilot on [`crates/host-cpal/Cargo.toml:16`](https://github.com/cmk/agogo/pull/15#discussion_r3141063403) (2026-04-25 01:38 UTC)

The comment says CI runs a dedicated `cargo test -p agogo-host-cpal` job, but this crate is detached (not a workspace member) and the workflow in this PR runs cargo from `crates/host-cpal` instead. Please update this comment to match the actual CI invocation (e.g., working-directory / `--manifest-path`).


<!-- gh-id: 3141063411 -->
### Copilot on [`crates/host-midi/README.md:13`](https://github.com/cmk/agogo/pull/15#discussion_r3141063411) (2026-04-25 01:38 UTC)

README says CI runs `cargo test -p agogo-host-midi`, but the CI workflow in this PR uses `working-directory: crates/host-midi` (since `-p` won’t resolve detached crates from the workspace root). Please update this section so the README matches the real CI job behavior.


<!-- gh-id: 3141063420 -->
### Copilot on [`crates/host-cpal/src/cpal/control.rs:185`](https://github.com/cmk/agogo/pull/15#discussion_r3141063420) (2026-04-25 01:38 UTC)

The drop-ordering doc claims messages produced after the drain thread exits “get counted as overruns by `dropped_count`”. In reality, messages can still be successfully enqueued (and then silently lost) until the ring fills; only pushes that hit a full ring increment `dropped_count`. Please adjust the wording to reflect that the reverse drop order can lose messages (not necessarily count them) unless/until the buffer overruns.


<!-- gh-id: 3141063427 -->
### Copilot on [`crates/host-cpal/src/cpal.rs:101`](https://github.com/cmk/agogo/pull/15#discussion_r3141063427) (2026-04-25 01:38 UTC)

The support-scan predicate checks sample format (F32) and channels in addition to sample rate, but the error returned on failure is `UnsupportedSampleRate(cfg.sample_rate)`. This can produce a misleading error when the rate is supported but only a different format/channel count is available. Consider either (a) narrowing the predicate to rate-only, or (b) adding an `UnsupportedConfig`/`UnsupportedFormat`/`UnsupportedChannels` error (or including details in the error message) so callers can distinguish the real cause.


<!-- gh-id: 3141063430 -->
### Copilot on [`doc/plans/plan-2026-04-24-02.md:583`](https://github.com/cmk/agogo/pull/15#discussion_r3141063430) (2026-04-25 01:38 UTC)

This plan doc’s Build gates section still uses `cargo build -p agogo-host-cpal` / `-p agogo-host-midi` (and later `cargo clippy -p ...`) even though the crates are intentionally detached and earlier in the plan it correctly notes that `-p <crate>` from the workspace root won’t resolve them. Please update these commands to use `--manifest-path crates/host-*/Cargo.toml` or `cd crates/host-*` so the instructions are reproducible.

<!-- gh-id: 4174435551 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-04-25 01:38 UTC](https://github.com/cmk/agogo/pull/15#pullrequestreview-4174435551))

## Pull request overview

Implements Plan 13’s first runnable end-to-end demo pipeline by adding an `AudioHost` trait to `agogo-core`, introducing detached platform backends for cpal audio input and midir MIDI output, and wiring them into `agogo demo` behind feature flags (plus CI/docs/conventions updates to support the detached crates).

**Changes:**
- Added `agogo_core::sink::audio` (`AudioHost`, `AudioIo`, `Config`, `Handle`, `AudioHostError`) and an allocation-free scheduler API (`tick_stream_into`).
- Introduced detached backend crates `agogo-host-cpal` (cpal stream + RT callback + rtrb SPSC/drain thread) and `agogo-host-midi` (midir `MidiSink`).
- Integrated feature-gated `agogo demo` CLI and updated CI + float allowlisting/docs to account for the new crates.

### Reviewed changes

Copilot reviewed 21 out of 23 changed files in this pull request and generated 5 comments.

<details>
<summary>Show a summary per file</summary>

| File | Description |
| ---- | ----------- |
| scripts/check-floats.sh | Extends float allowlist to new PCM-ABI sites. |
| doc/reviews/review-00015.md | Adds a Plan 13 review record (verification narrative, follow-ups). |
| doc/plans/plan-2026-04-24-02.md | Adds the Plan 13 design/plan document and verification checklist. |
| crates/host-midi/src/midir.rs | Implements `MidirSink` over midir with basic tests and port listing. |
| crates/host-midi/src/lib.rs | host-midi crate root + docs + exports. |
| crates/host-midi/README.md | Documents detached-crate workflow and local setup notes. |
| crates/host-midi/Cargo.toml | Adds detached `agogo-host-midi` crate manifest and deps. |
| crates/host-cpal/src/lib.rs | host-cpal crate root + docs + exports. |
| crates/host-cpal/src/cpal/control.rs | Adds rtrb SPSC + RT producer + drain thread + tests. |
| crates/host-cpal/src/cpal/callback.rs | Adds `CallbackState` hot loop + preallocation helper + tests. |
| crates/host-cpal/src/cpal.rs | Implements `CpalHost: AudioHost` with stream-owner thread. |
| crates/host-cpal/README.md | Documents detached-crate workflow and development notes. |
| crates/host-cpal/Cargo.toml | Adds detached `agogo-host-cpal` crate manifest and deps. |
| crates/core/src/lib.rs | Exposes new `host` module. |
| crates/core/src/sink/audio.rs | Defines `AudioHost` API and RT callback payload shape (`AudioIo`). |
| crates/core/src/control/event.rs | Adds `tick_stream_into` and refactors `tick_stream` to delegate. |
| crates/cli/src/main.rs | Adds `agogo demo` commands and end-to-end wiring module. |
| crates/cli/Cargo.toml | Adds optional path deps + features (`cpal`, `midi`, `demo`). |
| Cargo.toml | Excludes new detached host crates from the workspace. |
| Cargo.lock | Updates lockfile for new dependencies pulled by `agogo-cli`. |
| CLAUDE.md | Updates “no stored floats” exception-module count/notes. |
| .gitignore | Ignores detached crates’ per-crate `Cargo.lock` files. |
| .github/workflows/ci.yml | Adds CI jobs to test/clippy detached host crates from their dirs. |
</details>






<!-- gh-id: 3141092170 -->
#### ↳ cmk ([2026-04-25 01:53 UTC](https://github.com/cmk/agogo/pull/15#discussion_r3141092170))

Fixed in the round-3 fix commit — test-module doc comment no longer references the nonexistent `cpal_default_input` fixture; it now points at Plan 14's `agogo run` acceptance path per Plan 13's §T6 deferral.

<!-- gh-id: 3141092247 -->
#### ↳ cmk ([2026-04-25 01:53 UTC](https://github.com/cmk/agogo/pull/15#discussion_r3141092247))

Fixed — the plan doc's workspace/CI section was updated in round 1 to use `working-directory: crates/host-<name>`. The Build gates section at lines 581–594 still had stale `-p` commands though, which round 3 (`a2196fa`) cleans up (`--manifest-path crates/host-*/Cargo.toml` for the detached crates; `-p agogo-cli` stays since `agogo-cli` IS a workspace member).

<!-- gh-id: 3141092353 -->
#### ↳ cmk ([2026-04-25 01:53 UTC](https://github.com/cmk/agogo/pull/15#discussion_r3141092353))

Stale — fixed in the round-1 fix commit (`ca3b622`). `ci.yml` at line 81 now contains `working-directory: crates/host-midi`, not `cargo test -p`. Copilot appears to be reviewing the pre-fix commit's diff rather than the current HEAD.

<!-- gh-id: 3141092444 -->
#### ↳ cmk ([2026-04-25 01:53 UTC](https://github.com/cmk/agogo/pull/15#discussion_r3141092444))

Stale on the `-p` commands — fixed in round 1 (`ca3b622`): the Dev workflow section uses `--manifest-path crates/host-cpal/Cargo.toml` (with a `cd` alternative). Stale on `cpal_default_input_smoke` too — the README now explicitly points at Plan 14 per Plan 13's T6 deferral.

<!-- gh-id: 3141092533 -->
#### ↳ cmk ([2026-04-25 01:53 UTC](https://github.com/cmk/agogo/pull/15#discussion_r3141092533))

Stale — fixed in round 1 (`ca3b622`): comparison is now `c.channels() == cfg.input_channels`, paired with `cfg.input_channels != 1` rejection at `CpalHost::run` entry. Round 3 (`a2196fa`) additionally splits the error into `UnsupportedSampleRate` vs `UnsupportedConfig(String)` so a format/channel mismatch no longer surfaces as a rate error (your gh-id 3141063427).

<!-- gh-id: 3141092635 -->
#### ↳ cmk ([2026-04-25 01:53 UTC](https://github.com/cmk/agogo/pull/15#discussion_r3141092635))

Stale — fixed in round 1 (`ca3b622`): `as_slice` now clamps `bytes_len` with `min(3)`, so a `MidiMessage { bytes_len: 4, .. }` returns the full 3-byte view rather than panicking. Keeping fields public with the `debug_assert!` in `from_slice` as the primary invariant guard — see the round-1 reply (gh-id 3141025525) for rationale.

<!-- gh-id: 3141092708 -->
#### ↳ cmk ([2026-04-25 01:53 UTC](https://github.com/cmk/agogo/pull/15#discussion_r3141092708))

Stale — drop-ordering doc comment was added in round 1 (`ca3b622`). Round 3 (`a2196fa`) additionally refines the wording per your own gh-id 3141063420 catch: messages land in the ring successfully (no overrun), they're just silently lost when the drain thread exits. `dropped_count` only bumps on full-ring pushes.

<!-- gh-id: 3141092888 -->
#### ↳ cmk ([2026-04-25 01:53 UTC](https://github.com/cmk/agogo/pull/15#discussion_r3141092888))

Fixed in round 3 (`a2196fa`) — same shape as the `cpal_default_input` fix for cpal.rs's test module: the midir.rs test-module doc comment now points at Plan 14's acceptance path rather than the `midir_loopback` fixture (deferred per Plan 13 T6).

<!-- gh-id: 3141093182 -->
#### ↳ cmk ([2026-04-25 01:53 UTC](https://github.com/cmk/agogo/pull/15#discussion_r3141093182))

Fixed in round 3 (`a2196fa`) — the local-review section now carries an inline "Round 2 correction" note pointing at Copilot gh-id 3141035633 / 3141035679 and the actual `working-directory` fix. Keeping the original text as a historical record of round 1's (broken) intent; the correction note makes it clear that approach was superseded.

<!-- gh-id: 3141093339 -->
#### ↳ cmk ([2026-04-25 01:53 UTC](https://github.com/cmk/agogo/pull/15#discussion_r3141093339))

Stale — fixed in round 1 (`ca3b622`). Same as gh-id 3141035633 for the host-midi job: line 68 of the current ci.yml is `crates/host-cpal` as a `working-directory`, not `-p agogo-host-cpal`. Copilot is re-flagging the pre-fix content.

<!-- gh-id: 3141093469 -->
#### ↳ cmk ([2026-04-25 01:53 UTC](https://github.com/cmk/agogo/pull/15#discussion_r3141093469))

Stale on the `-p` commands in the Dev workflow section — fixed in round 1 (`ca3b622`) with `--manifest-path`. Round 3 (`a2196fa`) additionally fixes the top-of-file intro paragraph that still had a stray `-p agogo-host-midi` reference (your gh-id 3141063411). The `midir_loopback_roundtrip` reference now points at Plan 14's acceptance path per Plan 13's T6 deferral.

<!-- gh-id: 3141093532 -->
#### ↳ cmk ([2026-04-25 01:53 UTC](https://github.com/cmk/agogo/pull/15#discussion_r3141093532))

Stale — fixed in round 1 (`ca3b622`): `AudioIo` doc now pins Plan 13 to mono explicitly, and `CpalHost::run` enforces `cfg.input_channels == 1` at entry. See round-1 reply gh-id 3141025822 for the full picked-option-(b) rationale.

<!-- gh-id: 3141093612 -->
#### ↳ cmk ([2026-04-25 01:53 UTC](https://github.com/cmk/agogo/pull/15#discussion_r3141093612))

Stale — fixed in round 1 (`ca3b622`). The docstring no longer claims "(or until Ctrl-C)"; explicit note that Plan 13 has no signal handler and Plan 14's `agogo run` adds it. See round-1 reply gh-id 3141025364.

<!-- gh-id: 3141093712 -->
#### ↳ cmk ([2026-04-25 01:53 UTC](https://github.com/cmk/agogo/pull/15#discussion_r3141093712))

Fixed in round 3 (`a2196fa`) — the `host-cpal/Cargo.toml` preamble comment now describes the actual CI pattern (`working-directory: crates/host-cpal` in `ci.yml`) rather than the stale `cargo test -p agogo-host-cpal`.

<!-- gh-id: 3141093799 -->
#### ↳ cmk ([2026-04-25 01:53 UTC](https://github.com/cmk/agogo/pull/15#discussion_r3141093799))

Fixed in round 3 (`a2196fa`) — the host-midi README intro paragraph (line 12) now matches CI: `CI runs \`cargo test\` from this crate's directory in a dedicated \`host-midi\` job`. The Dev workflow section's build commands were already `--manifest-path` after round 1; this was the stray intro I missed.

<!-- gh-id: 3141093892 -->
#### ↳ cmk ([2026-04-25 01:53 UTC](https://github.com/cmk/agogo/pull/15#discussion_r3141093892))

Fixed in round 3 (`a2196fa`) — `spawn_drain`'s doc comment now says the reverse-drop-order case lands messages in the ring successfully (no overrun) and then silently loses them when the drain thread exits; `dropped_count` only increments on full-ring pushes. Accurate description replaces round 1's wording that implied they'd all be counted.

<!-- gh-id: 3141094057 -->
#### ↳ cmk ([2026-04-25 01:53 UTC](https://github.com/cmk/agogo/pull/15#discussion_r3141094057))

Fixed in round 3 (`a2196fa`) — `AudioHostError` gains an `UnsupportedConfig(String)` variant (and is marked `#[non_exhaustive]` so future variants can land cleanly). `CpalHost::run` now distinguishes: if no config covers the requested rate, `UnsupportedSampleRate(sample_rate)`; if the rate IS supported but no f32 mono config exists, `UnsupportedConfig` with details of what's available vs. requested. Preserves the existing variant's intent while giving callers a clearer signal when the failure is format/channel, not rate.

<!-- gh-id: 3141094277 -->
#### ↳ cmk ([2026-04-25 01:53 UTC](https://github.com/cmk/agogo/pull/15#discussion_r3141094277))

Fixed in round 3 (`a2196fa`) — plan doc Build gates section (lines 581–594) now uses `--manifest-path crates/host-*/Cargo.toml` for the detached crates; `cargo build -p agogo-cli --features demo` stays with `-p` since `agogo-cli` IS a workspace member.

<!-- gh-id: 3141114797 -->
### Copilot on [`crates/cli/src/main.rs:1317`](https://github.com/cmk/agogo/pull/15#discussion_r3141114797) (2026-04-25 01:59 UTC)

`SampleRate` is imported in the `demo` module but never used. When building/clippying with `--features demo`, this will trigger an `unused_imports` warning (and can fail `-D warnings`). Remove the import or use it.


<!-- gh-id: 3141114806 -->
### Copilot on [`crates/host-cpal/src/cpal.rs:77`](https://github.com/cmk/agogo/pull/15#discussion_r3141114806) (2026-04-25 01:59 UTC)

Invalid user configuration (`input_channels != 1`) is currently returned as `AudioHostError::Backend(...)`. Since `AudioHostError` has `UnsupportedConfig`, this should be reported as `UnsupportedConfig` (or a dedicated variant) so callers can distinguish user config errors from true back-end failures.


<!-- gh-id: 4174506140 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-04-25 01:59 UTC](https://github.com/cmk/agogo/pull/15#pullrequestreview-4174506140))

## Pull request overview

Implements Plan 13’s first runnable end-to-end demo pipeline by adding a core audio-host trait (`AudioHost`/`AudioIo`), introducing detached platform backends for cpal audio input and midir MIDI output, and wiring the full RT callback → scheduler → renderer → SPSC drain → MIDI-out chain behind `agogo demo`.

**Changes:**
- Adds `agogo_core::sink::audio` and extends the scheduler with allocation-free `tick_stream_into`.
- Introduces detached crates `agogo-host-cpal` (cpal stream + RT callback + rtrb SPSC/drain) and `agogo-host-midi` (midir-backed `MidiSink`).
- Updates CLI (`agogo demo`), CI, and repo conventions (float allowlist, detached-crate CI jobs, gitignore).

### Reviewed changes

Copilot reviewed 21 out of 23 changed files in this pull request and generated 3 comments.

<details>
<summary>Show a summary per file</summary>

| File | Description |
| ---- | ----------- |
| scripts/check-floats.sh | Extends float allowlist for new PCM-ABI sites. |
| doc/reviews/review-00015.md | Adds review record/verification narrative for Plan 13. |
| doc/plans/plan-2026-04-24-02.md | Adds Plan 13 plan doc (design + tasks + verification + review notes). |
| crates/host-midi/src/midir.rs | Implements `MidirSink` over midir + basic tests. |
| crates/host-midi/src/lib.rs | host-midi crate root and exports. |
| crates/host-midi/README.md | Documents detached-crate workflow and setup notes. |
| crates/host-midi/Cargo.toml | Adds detached `agogo-host-midi` manifest and deps. |
| crates/host-cpal/src/lib.rs | host-cpal crate root and exports. |
| crates/host-cpal/src/cpal/control.rs | Adds rtrb SPSC producer/consumer + drain thread + tests. |
| crates/host-cpal/src/cpal/callback.rs | Adds RT `CallbackState` hot loop + sizing helper + tests. |
| crates/host-cpal/src/cpal.rs | Implements `CpalHost: AudioHost` with worker-thread stream ownership. |
| crates/host-cpal/README.md | Documents detached-crate workflow and development notes. |
| crates/host-cpal/Cargo.toml | Adds detached `agogo-host-cpal` manifest and deps. |
| crates/core/src/lib.rs | Exposes new `host` module. |
| crates/core/src/sink/audio.rs | Defines `AudioHost` API, `AudioIo`, `Handle`, and `AudioHostError`. |
| crates/core/src/control/event.rs | Adds `tick_stream_into` and refactors `tick_stream` to delegate. |
| crates/cli/src/main.rs | Adds feature-gated `agogo demo` subcommands and wiring. |
| crates/cli/Cargo.toml | Adds optional path deps + features (`cpal`, `midi`, `demo`). |
| Cargo.toml | Excludes new host crates from workspace members. |
| Cargo.lock | Locks new dependencies pulled in by CLI feature(s). |
| CLAUDE.md | Updates float-exception module count/notes for Plan 13. |
| .gitignore | Ignores detached-crate `Cargo.lock` files for host crates. |
| .github/workflows/ci.yml | Adds dedicated CI jobs for detached host crates. |
</details>






<!-- gh-id: 3141114811 -->
### Copilot on [`doc/plans/plan-2026-04-24-02.md:486`](https://github.com/cmk/agogo/pull/15#discussion_r3141114811) (2026-04-25 01:59 UTC)

This section still states that `--duration-ms` defaults to “run until Ctrl-C”, but the implemented `agogo demo run` path is fixed-duration and explicitly defers Ctrl-C handling to Plan 14. Please update this CLI synopsis (and the related handler-flow / acceptance steps that mention Ctrl-C) so the plan doc matches what actually ships in this PR.

<!-- gh-id: 3141135168 -->
#### ↳ cmk ([2026-04-25 02:09 UTC](https://github.com/cmk/agogo/pull/15#discussion_r3141135168))

Push-back — `SampleRate` IS used. `S48::HZ` at line 1382 (`if args.sr != S48::HZ`) accesses a trait-associated constant, so the `SampleRate` trait must be in scope at the call site. Verified locally: `cargo clippy -p agogo-cli --features demo --all-targets -- -D warnings` is clean.

<!-- gh-id: 3141135276 -->
#### ↳ cmk ([2026-04-25 02:09 UTC](https://github.com/cmk/agogo/pull/15#discussion_r3141135276))

Fixed in round 4 (`ccd767c`) — `CpalHost::run`'s mono rejection now returns `AudioHostError::UnsupportedConfig(...)` instead of `Backend(...)`. Round 3 added the `UnsupportedConfig` variant for format/channel mismatches but I missed the mono-rejection site; caught here, fixed here.

<!-- gh-id: 3141135399 -->
#### ↳ cmk ([2026-04-25 02:09 UTC](https://github.com/cmk/agogo/pull/15#discussion_r3141135399))

Fixed in round 4 (`ccd767c`) — plan doc lines 485 (CLI synopsis), 505 (handler flow), and 615 (E2E acceptance) no longer claim `--duration-ms` runs "until Ctrl-C"; each now explicitly notes Plan 13 has no signal handler and SIGINT hard-kills before teardown. Plan 14's `agogo run` adds clean interrupt.
