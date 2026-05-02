# agogo v0.1 - Baseline Timing Kernel

## Roadmap Role

v0.1 remains the historical baseline: prove that agogo can run an audio-driven
clocking kernel, render MIDI clock/transport bytes, and exercise the host stack
from a CLI. The adapted roadmap changes the interpretation of v0.1:

- It is a hard-time kernel proof, not a product timing claim.
- `midir` output is best-effort external MIDI, not sample-accurate scheduling.
- Any downstream roadmap must preserve the "no locks, no allocation, no async"
  rule inside the audio callback.

## In Scope

- Keep the existing time, channel, sync, sink, host-cpal, host-midi, host-link,
  and CLI baseline intact.
- Document the callback contract in one place:
  - `Playhead::on_buffer` is the only hard-time entry point.
  - Control threads communicate through atomics or bounded SPSC queues.
  - MIDI drain threads may block, but the audio callback may not.
- Keep the current fixed-point/Conn discipline as a permanent invariant.
- Add a compatibility note to user-facing demos: "sample-accurate internally;
  external MIDI timing depends on backend and hardware."

## Acceptance

- Existing v0.1 tests remain green.
- CLI demo still emits stable MIDI clock through the current best-effort
  backend.
- Documentation states that v0.2 owns native timestamped output and the
  three-repo steel thread.

## Deferred Explicitly

- Native timestamped MIDI sinks.
- Per-buffer Link host-time anchoring.
- stdio-core lifecycle/dispatch/observation integration.
- Durable timing diagnostics.
- Heterogeneous outputs and latency compensation.

## Why This Shape

This avoids the main pitfall identified in the recommendations: treating the
first visible MIDI output path as evidence that the full studio system is
sample-accurate. v0.1 is valuable because the callback architecture exists; the
next version must retire the hard external-output and cross-repo integration
risk immediately.
