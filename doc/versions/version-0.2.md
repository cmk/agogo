# agogo v0.2 - Hard-Time Steel Thread

## Thesis

v0.2 pulls the hardest timing and integration risks forward. It must prove that
agogo can accept soft agent commands, apply them through a bounded hard-time
bridge, emit timestamped or explicitly best-effort output, and publish
human-rate state snapshots without letting stdio-core or stdio enter the audio
callback path.

This version is the agogo side of the three-repo steel thread:

```
stdio server/client
  -> stdio-core dispatcher + policy + event log
  -> agogo host adapter
  -> agogo RT bridge
  -> Playhead::on_buffer
  -> timestamped sink or declared best-effort sink
  -> agogo-state snapshot back to stdio-core
```

## Pull-Forward Decisions

- Move the stdio-core control bridge from the old v0.3 plan into v0.2.
- Move the snapshot seqlock and `agogo-state` observation contract from the old
  v0.4 plan into v0.2.
- Start the native timestamped MIDI sink work now, even if the first backend is
  a narrow macOS CoreMIDI or JACK spike.
- Add command admission metadata now: command id, source id, time domain,
  deadline, coalesce key, and rejection reason.
- Keep 960 PPQN math in scope only where it is needed for steel-thread
  correctness. Full grid expansion can follow after bridge/output risk retires.

## Sprints

### S1 - Callback Contract And Allocation Gates

- Add a test/bench harness that asserts no allocation in `Playhead::on_buffer`
  after construction.
- Add worst-case channel/event count timing tests for the configured buffer
  sizes.
- Keep all logging, serialization, JSON, and stdio-core interaction outside the
  callback.

Verifies: callback path remains hard-time safe before new integration code is
allowed to land.

### S2 - Typed RT Command Bridge

- Extend the current bridge from tempo plus ordered commands to a typed command
  envelope.
- Keep last-value controls as atomics and ordered controls in a bounded SPSC
  ring.
- Return visible errors on queue full, late command, invalid time domain, or
  unsupported command class.
- Add a soft-side adapter that can be driven by stdio-core without linking
  stdio-core into the RT path.

Verifies: every accepted soft command is either applied by a declared deadline
or rejected before admission.

### S3 - Snapshot Slot And Observation Publisher

- Publish `AgogoSnapshot` through a fixed-capacity seqlock slot.
- Keep `SNAPSHOT_SCHEMA = "agogo.snapshot.v1"`,
  `AGOGO_STATE_FORM_TYPE = "agogo-state"`, and
  `AGOGO_MAIN_ID = "agogo.main"`.
- Serialize only from the decimating soft task.
- Include monotonic `seq`, sync source, lock/error state, audio load, and
  per-channel status.

Verifies: observation backpressure produces detectable sequence gaps, never
callback stalls or corrupted snapshots.

### S4 - Timestamped Output Backend Spike

- Introduce sink classes:
  - `BestEffortMidiSink` for current midir behavior.
  - `TimestampedMidiSink` for a first native scheduled backend.
  - `DiagnosticSink` for intended-vs-sent timing measurement.
- Preserve `at_sample` through the drain path and prove whether the backend can
  honor it.
- Add a per-backend timing capability report.

Verifies: agogo can distinguish "internally sample accurate" from "externally
timestamped" from "best effort".

### S5 - Three-Repo Steel Thread Demo

- Provide the agogo adapter API consumed by stdio-core.
- Demo path: stdio dispatches `agogo.tempo.set` and `agogo.start`, stdio-core
  logs and observes the command, agogo applies it by the next buffer, and
  `agogo-state` reports the new state.
- The demo must run with no observation subscriber and still apply control.

Verifies: the hard-time path survives the full product stack without timing
control passing through a lossy telemetry channel.

## Properties

| Property | Invariant |
| --- | --- |
| `rt_callback_no_alloc` | `Playhead::on_buffer` performs no allocation after construction. |
| `command_admission_is_total` | Every command returns accepted, rejected, or late; no silent loss. |
| `accepted_command_applies_by_deadline` | Accepted commands apply by their declared domain/deadline or report a missed-deadline fault. |
| `control_independent_of_observation` | Control works with zero observation subscribers. |
| `snapshot_write_no_alloc` | RT snapshot writes do not allocate, lock, serialize, or await. |
| `snapshot_gap_detectable` | Dropped telemetry frames are visible as seq gaps. |
| `timestamp_capability_truthful` | Backend capability reports match measured behavior under diagnostic sink tests. |

## Acceptance

- `cargo test --workspace` green.
- Steel-thread demo runs against stdio-core and stdio path dependencies.
- The demo records: command requested, command accepted, command applied,
  snapshot observed, and no RT drop/overrun counters.
- Documentation marks current midir output as best effort unless a native
  timestamped backend is selected.

## Deferred To v0.3

- Full MIDI clock follower.
- Full Link follower/source PID behavior.
- Complete 960 PPQN grid expansion if it was not required by steel-thread
  tests.
- Multi-device latency calibration UI.
