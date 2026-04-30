# agogo v0.4

## Goal

**De-risk the agogo↔stdio-core TUI-update seam, and ship CV out.**
agogo publishes a decimated `AgogoSnapshot` (BPM, transport state,
PLL lock, per-channel state) through stdio-core's
`ObservationDispatcher`, and the long-deferred v0.1 "§4 precision
crown jewel" — CV pulse output via a cpal output host — ships
alongside.

Two seams get proven here: observation (the second half of the
stdio-core integration after v0.3's dispatch) and audio output
(the other half of the cpal host story that v0.1 only did on the
input side). Bundling them is deliberate — both are "new output
surface" work and share the same cpal-output scaffolding.

## Upstream dependency

stdio-core 0.2's `ObservationDispatcher`
(stdio-core/src/observation/dispatcher.rs:36–124), wire format
(stdio-core/src/observation/wire.rs:20–104), and Plan 11 agogo
snapshot contract must be in place. The contract fixes
`FormType::Other("agogo-state")`, `stream_id = "agogo.main"`,
`form_id = "agogo.main"`, monotonic `seq`, and full-snapshot `Patch`
payloads for v1. The stdio-core-side TUI renderer consumes agogo's
snapshot stream.

## Sprint slots

| # | Slug | Status | Scope |
|---|------|--------|-------|
| 01 | `plan-2026-04-30-02` | in progress | `AgogoSnapshot` type, JSON schema, fixed-capacity RT snapshot slot, and stdio-shaped publisher: serde shape covering BPM, transport state, PLL lock indicator, audio load, and per-channel phase/active state. Decimation cadence targets ~30 Hz. Stable observation identifiers are `Other("agogo-state")`, `agogo.main`, and full-snapshot `Patch` v1. Schema doc lands in `doc/designs/snapshot.md`. |
| 02 | `plan-2026-04-2N-02` (TBD) | next-next | Direct stdio-core `ObservationDispatcher` integration once the sibling dependency can be consumed without moving agogo's MSRV. Monotonic `seq` lets consumers detect gaps; stdio-core newest-drop policy is acceptable for telemetry only. |
| 03 | `plan-2026-04-2N-03` (TBD) | last | CV pulse output: `out/audio` module + cpal output host; single-sample impulse per tick; 4-channel interleaving; bipolar ±1.0 option to avoid DC offset on AC-coupled interfaces. Covers the v0.1 deferred "§4 precision crown jewel." |

## Properties (must pass)

| Property | Module | Invariant |
|----------|--------|-----------|
| `snapshot_schema_round_trips` | `agogo_host::snapshot` | `AgogoSnapshot` serde → `serde_json::Value` → `AgogoSnapshot` is identity for arbitrary generated snapshots. |
| `seq_monotonic_under_decimation` | `agogo_host::snapshot::push` | The `seq` field on published `ObservationParams` is strictly monotonic per-`stream_id`, even when the RT writer and the decimating reader run at different rates. |
| `rt_push_has_no_alloc` | `agogo_host::snapshot::push` | Writing a snapshot from the audio thread does not allocate. The serialization and `dispatch()` call happen on the decimating task, not in the RT callback. |
| `agogo_snapshot_drop_is_detectable` | `agogo_host::snapshot::push` | Forced observation backpressure produces a detectable `seq` gap, not corrupted or unparsable snapshot state. |
| `cv_impulse_is_sample_accurate` | `out::audio::cv` | For any `(sr, ppqn, bpm)` with an integer samples-per-tick (the v0.2 sweet-spot band), the emitted CV impulse lands on the exact tick-boundary sample index with zero offset. |
| `cv_impulse_energy_is_one_sample` | `out::audio::cv` | Each emitted CV pulse is exactly one non-zero sample (`±1.0`) followed by zero; no multi-sample ringing or DC creep. |

## v0.4 acceptance

- `cargo test --workspace` green, including integration test that
  spawns stdio-core `Dispatcher` + mock observation subscriber and
  verifies snapshot cadence plus BPM mutations from v0.3 tools
  round-trip into the observation stream.
- `cargo run -p agogo-cli -- agogo run --audio-in <dev> --cv-out
  <dev> --bpm 120` emits sample-accurate impulses on tick boundaries
  with a DC-coupled audio interface, measurable on an oscilloscope.
- Snapshot schema doc lives in `doc/designs/snapshot.md` and is
  linked from the stdio-core-side renderer's form-type registry.
- Every property in the table above passes without `#[ignore]`.

## Deferred to v0.5 (and beyond)

- Transport FSM, Ableton Link follower, heterogeneous per-channel
  output dispatch, MTC quarter-frame generator — v0.5.
- JSON-patch diffs for incremental snapshot updates: upstream
  stdio-core ships full-snapshot `Patch` ops in 0.2
  (stdio-core/src/observation/wire.rs:72–74). agogo waits on the
  upstream diff format before optimizing wire size.
- All v0.1–v0.3 deferred items that carry forward unchanged.

## Reference

- `../stdio-core/src/observation/wire.rs:20–104` — `ObservationParams`
  shape, `FormType` enum.
- `../stdio-core/src/observation/dispatcher.rs:36–124` — push
  protocol; 128-slot channel; drop-newest-on-full policy.
- `../stdio-core/doc/designs/agogo.md` — `agogo-state` form contract
  and audio-rate event boundary.
- `../stdio-core/doc/plans/plan-2026-04-30-02.md` — stdio-core Plan
  11 agogo snapshot observation contract.
- `doc/designs/snapshot.md` — agogo-owned v1 schema and RT boundary.
- `doc/agogo.md` §4 — precision budget; CV out is the "crown jewel"
  path whose acceptance is sample-accurate impulse alignment.
- `doc/versions/version-0.1.md` — CV pulse output entry in the
  deferred list (first-class v0.4 target).
- `doc/notes/note-2026-04-23-03.md` lines 448–520 — single-sample
  impulse design.
