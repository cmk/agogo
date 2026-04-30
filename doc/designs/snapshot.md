# AgogoSnapshot v1

`AgogoSnapshot` is the human-rate observation payload shared by the
standalone TUI, the stdio adapter, and the future stdio-core
`agogo-state` renderer. It is telemetry only: control still flows
through agogo's RT-safe control bridge, never through observation.

## Wire Contract

Snapshots publish through stdio-core observation notifications with:

- `form_type = Other("agogo-state")`
- `stream_id = "agogo.main"`
- `form_id = "agogo.main"`
- first notification as `Create`
- later notifications as full-snapshot `Patch`
- optional `Destroy` during graceful unmount

`seq` is monotonic per stream. stdio-core may drop newest observation
frames under pressure; consumers detect missing telemetry by looking
for `seq` gaps and keep parsing later snapshots.

## Schema

```json
{
  "schema": "agogo.snapshot.v1",
  "seq": 42,
  "bpm": 120,
  "transport": {
    "state": "running",
    "bar": 12,
    "beat": 3,
    "tick": 480
  },
  "sync": {
    "source": "internal",
    "pll_locked": true,
    "error_ticks": 0.12
  },
  "audio": {
    "sample_rate": 48000,
    "buffer_size": 128,
    "load": 0.31
  },
  "channels": [
    {
      "index": 0,
      "enabled": true,
      "grid": "T4",
      "phase": 0.5,
      "output": "midi"
    }
  ]
}
```

Numeric fields that need fractional presentation are serialized as JSON
numbers from fixed-point storage:

| Field | Stored Unit | Meaning |
|-------|-------------|---------|
| `bpm` | micro-BPM | Tempo, matching `agogo_core::conn::tempo::Tempo`. |
| `sync.error_ticks` | micro-ticks | Signed PLL error in ticks. |
| `audio.load` | parts per million | Fraction of buffer budget used. |
| `channels[*].phase` | parts per million | Channel phase in `[0, 1]`. |

The adapter stores no floating-point state for snapshots. Decimal JSON
numbers are produced off the RT thread from fixed-point integers.

## RT Boundary

The audio callback writes `RtSnapshotFrame` into a fixed-capacity
`SnapshotSlot`. The write path uses atomics and fixed channel storage:
no JSON serialization, no stdio-core calls, no locks, and no heap
allocation. The async side reads a typed `AgogoSnapshot`, serializes it,
and publishes through the observation sink at the UI cadence.

The v1 channel storage limit is 16 channels. If a callback provides
more channels, the snapshot writer publishes the first 16 in order.
Within generated and normal adapter snapshots, channel indices must be
unique within a snapshot.

## Cadence

The target publish cadence is about 30 Hz. This is a UI visibility
contract, not an audio timing contract. Continuous MIDI clock ticks, CV
impulses, and MTC quarter frames stay out of the stdio event bus except
as decimated summary state.

## Compatibility

`Patch` currently carries a complete `AgogoSnapshot` v1 payload. JSON
Patch diffs are deliberately deferred until stdio-core owns a shared
patch convention for all form types.
