# agogo v0.3

## Goal

**De-risk the agogo↔stdio-core async-dispatch seam.** A new
`crates/host/` workspace member implements stdio-core 0.2's
`StudioMcpServer` trait so a stdio-core `Dispatcher` can drive
agogo's real-time callback from async tool calls, across a
lock-free control-plane bridge that never drops ticks and never
allocates in the audio thread.

This is the highest-risk item in the v0.2–v0.5 roadmap: stdio-core's
only existing RT↔async bridge (`clock/bridge.rs`) uses `try_send` +
drop-newest, which is fine for an observation stream but unsuitable
for agogo's sample-accurate tick domain. v0.3 builds the design
stdio-core's upstream left for drivers to solve.

## Upstream dependency

This sprint depends on **stdio-core 0.2** being on crates.io (or a
pinned path dep) with the Plan 10 lifecycle surface in place:
`StudioMcpServer::on_mount(bus)`, `StudioMcpServer::on_unmount()`,
dispatcher mount failure reporting, and a real-time-style fixture that
proves control dispatch does not depend on observation subscribers. If
that stdio-core surface slips, v0.3 slips with it.

## Sprint slots

| # | Slug | Status | Scope |
|---|------|--------|-------|
| 01 | `plan-2026-04-30-01` | next | `crates/host/` workspace member plus RT-safe bridge: implement `StudioMcpServer` lifecycle, expose `agogo.tempo.set`, `agogo.channel.configure`, `agogo.start`, `agogo.stop`, and `agogo.locate`, route ordered commands through `rtrb`, route last-value controls through atomics, and return tool errors when the agogo command queue is full. |
| 02 | `plan-2026-04-2N-02` (TBD) | next-next | Tool handlers end-to-end: each tool returns accepted/applies-by output after the bridge accepts the write, emits semantic stdio-core events on the async side only, and mutates a mock or real `Machine` through the same control-plane path. Unit tests spawn a stdio-core `Dispatcher`, issue tool calls, and assert tick-stream integrity under load. |
| 03 | `plan-2026-04-2N-03` (TBD) | last (optional) | Backpressure and lifecycle hardening: document shutdown ordering, mount/unmount idempotence, queue-full error surfaces, and any missing stdio-core primitive found during adapter work. File upstream stdio-core issues rather than routing timing-critical control through dropping channels. |

## Properties (must pass)

| Property | Module | Invariant |
|----------|--------|-----------|
| `tempo_set_has_no_rt_alloc` | `agogo_host::rt_bridge` | `agogo.tempo.set { bpm }` from an async tool call results in zero allocations on the audio thread (verified via `#[no_alloc]` guard or `DeallocTest` harness). |
| `tempo_set_applies_within_one_buffer` | `agogo_host::rt_bridge` | After an async `agogo.tempo.set`, the observed BPM in the audio callback changes by the start of the next audio buffer, never later. |
| `tick_stream_never_drops_under_command_load` | `agogo_host::rt_bridge` | At 1 kHz sustained tool-call rate against a running `Machine`, no ticks are lost from the emitted MIDI clock stream (proptest generates arbitrary command sequences). |
| `agogo_control_never_routes_through_observation` | `agogo_host::tools` | Tool dispatch succeeds with no `ObservationDispatcher` subscriber or observation stream; control reaches the RT bridge directly. |
| `agogo_mount_unmount_idempotent` | `agogo_host::driver` | Repeated mount/unmount releases background handles once and leaves the driver offline without leaking a running task. |
| `inverse_op_is_round_trip` | `agogo_host::tools` | For any tool call `(name, args)` that has an `inverse_op`, applying the inverse returns the `Machine` to its prior state. (Even though stdio-core's full inverse plumbing is 0.3 upstream, our trait impl declares correct inverses now.) |

## v0.3 acceptance

- `cargo test -p agogo-host` green.
- Demo: stdio-core dispatcher issues `agogo.tempo.set { bpm: 140 }`
  while the audio thread is running; observed BPM change within one
  audio buffer, zero dropped ticks, zero allocations on the RT
  thread.
- Every property in the table above passes without `#[ignore]`.
- `crates/host/` publishes a clean `StudioMcpServer` impl that
  works both in-process (bundled) and standalone (wrapped with
  `serve_mcp_stdio`).

## Deferred to v0.4 (and beyond)

- Observation/telemetry push + CV pulse output — v0.4.
- Transport FSM, Ableton Link, heterogeneous outputs, MTC — v0.5.
- All v0.1 and v0.2 deferred items that carry forward unchanged.

## Reference

- `../stdio-core/doc/versions/version-0.2.md` lines 46–54 —
  `StudioMcpServer` description and v0.2 commitment.
- `../stdio-core/doc/designs/agogo.md` — agogo driver integration
  contract: lifecycle, control, observation, backpressure, and
  audio-rate event boundaries.
- `../stdio-core/doc/plans/plan-2026-04-30-01.md` — stdio-core Plan
  10 lifecycle + real-time-style driver fixture.
- `../stdio-core/doc/designs/drivers.md` — driver authoring guide.
- `../stdio-core/src/driver.rs:38–71` — trait surface.
- `../stdio-core/src/dispatcher.rs:35–68` — dispatcher actor.
- `../stdio-core/src/clock/bridge.rs:36–42` — the lossy-bridge
  anti-pattern agogo's control plane must not repeat.
- `doc/agogo.md` §5 (platform abstraction; `AudioHost`, `MidiSink`).
- `doc/notes/note-2026-04-23-03.md` lines 895–928 — `SharedParams`
  sketch and lock-free parameter-update rationale.
