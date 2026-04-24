# agogo v0.5

## Goal

**Close the open-question list from `doc/agogo.md` §10.** Transport
FSM with NEG/POS one-bar forerun semantics, `PhaseSource::Link` PID
sync (building on the Plan 09 write-path foundation), heterogeneous
per-channel output format (MIDI / OSC / CV / MTC), and MTC
quarter-frame generation.

This is feature expansion on top of a stable stdio-core integration:
v0.3 proved the dispatch seam, v0.4 proved the telemetry seam, so
each new feature here slots into existing tool schemas and
observation payloads rather than forcing redesigns.

Plan 09 (pre-v0.5) lands the Link write-path foundation: tempo push,
a minimal `{Stopped, Playing}` `rust-fsm 0.7` declaration with
Link-subscription seam + one-shot publish, and per-channel
`snap_to_quantum: Option<Quantum>`. Sprints 01 and 02 build on that
foundation — Sprint 01 extends the FSM declaration with forerun
states; Sprint 02 restructures `LinkClock` from direct
`PhaseSourceImpl` to PID-smoothed reference per
`doc/designs/link.md`.

## Sprint slots

| # | Slug | Status | Scope |
|---|------|--------|-------|
| 01 | `plan-2026-04-2N-01` (TBD) | next | Transport FSM forerun semantics: **extend** Plan 09's `rust-fsm 0.7` declaration in `crates/host-link/src/transport.rs` (do not rewrite) with `PreRoll` + forerun-aware stop states, `Locate` command, NEG/POS one-bar forerun per `doc/agogo.md` §10, and bar-boundary alignment. Interaction with `PhaseSource` variants. Proptests for forerun correctness across arbitrary time-signature and tempo-change sequences. |
| 02 | `plan-2026-04-2N-02` (TBD) | next-next | `PhaseSource::Link` PID sync: **demote** `LinkClock` from direct `PhaseSourceImpl` to PID-smoothed reference per `doc/designs/link.md` Adopt §§1–6. Introduces: per-buffer atomic-seqlock `HostTimeAnchor`, per-buffer `capture_app_session_state()`, `cpal_info.timestamp().playback` as query instant, wrapped-error PID over Link timeline, `Stopped → Starting` reset-phase-and-zero-integrator. Depends on Plan 05 (audio callback). The rusty_link binding + lifecycle surface already landed in PRs #6 / #8; tempo push + FSM seam + quantum snap land in Plan 09. |
| 03 | `plan-2026-04-2N-03` (TBD) | then | Heterogeneous output dispatch: per-channel `OutputFormat` enum (MIDI / OSC / CV / MTC) routed through a single dispatch layer; per-format latency compensation table so sample-accuracy survives format plurality. |
| 04 | `plan-2026-04-2N-04` (TBD) | last | MTC quarter-frame generator: SMPTE 24/25/29.97/30 fps selection; alignment with Link-driven BPM changes; quarter-frame transmission over MIDI. Resolves note lines 2199–2248. |

## Properties (must pass)

Plan 09 already owns `transport_fsm_deterministic`,
`transport_fsm_no_spurious_publishes`, `tempo_push_monotone`,
`quantum_snap_idempotent`, and `quantum_snap_nonneg`. The v0.5
Sprint 01/02 properties below are additive.

| Property | Module | Invariant |
|----------|--------|-----------|
| `transport_forerun_lands_on_bar` | `host-link::transport` | For any `(time_sig, bpm, pre_roll_bars)` combination, the computed NEG-forerun start offset places the first emitted tick exactly on a bar boundary. |
| `transport_fsm_never_deadlocks` | `host-link::transport` | Arbitrary `Play` / `Stop` / `Locate` command sequences reach a terminal transport state within a bounded number of steps (no infinite loops in the forerun-extended FSM). |
| `host_time_anchor_torn_read_safe` | `host-link::anchor` | 1000-iteration racing writer/reader never yields a torn `(host_origin_micros, sample_rate)` pair; seqlock generation-counter retry catches all tears. |
| `wrapped_error_within_halfquantum` | `host-link::pid` | Wrapped-error computation against a Link target phase never returns an error magnitude exceeding half the quantum; tested across arbitrary quantum settings and phase pairs. |
| `link_follower_converges` | `host-link::pid` | Given a Link peer injecting arbitrary BPM steps within the PID's settling spec, agogo's tick stream converges to the Link timeline within the PID's documented settling time. |
| `hetero_dispatch_preserves_tick_order` | `out::dispatch` | For any multi-channel `Machine` with mixed `OutputFormat`s, emitted events across channels preserve the tick-order of the master stream (no format reorders a tick). |
| `mtc_quarter_frame_round_trips` | `out::mtc` | A generated MTC quarter-frame stream, fed back through a reference MTC reader, recovers the original SMPTE timecode exactly. |
| `per_format_latency_compensation_is_sample_accurate` | `out::dispatch` | With the documented latency compensation applied, a tick timestamped at master sample `s` lands on the wire at exactly `s` sample-aligned, regardless of which output format it targets. |

## v0.5 acceptance

- `cargo test --workspace` green.
- Demo: Ableton Live (or any Link peer) injects a tempo change;
  agogo's TUI telemetry (via v0.4 observation) reflects the new BPM
  within the PID's settling spec; per-channel output (channel 0 →
  MIDI clock, channel 1 → OSC `/tick`, channel 2 → CV pulse) all
  remain sample-aligned through the change.
- Transport FSM proptests green for 10⁵+ generated sequences.
- MTC quarter-frame output measured against a reference SMPTE
  reader shows zero frame drift over a 10-minute run.
- Every property in the table above passes without `#[ignore]`.

## Deferred to post-0.5

Kept here so v0.5 reviewers can flag scope creep:

- **Preset I/O** — `serde` + `ciborium` round-trip of `Machine` state;
  the sample-rate-agnosticism proptest invariant flagged in
  `doc/agogo.md` §10.
- **Platform-native MIDI sinks** — `host/coremidi` (sub-µs via
  `MIDITimeStamp`), `host/jack`, `host/winmm`, `host/alsa_midi`.
  v0.5 keeps using midir; native sinks are the precision-tier
  upgrade.
- **Network MIDI / rtpMIDI** backend.
- **Remote control** via pitch-bend / CC parameter mapping.
- **LFO render** — `channel/lfo` (sample-rate render of
  `time::envelope`).

Permanently out of scope per `doc/agogo.md` §11: VST/AU plugins,
hardware firmware, encoder UI.

## Reference

- `doc/agogo.md` §10 — the open-question list this version closes.
- `doc/designs/link.md` — Adopt/Defer/Reject triage of the Gemini
  chat's Link design; drives Sprint 02's PID-sync restructuring.
- `doc/plans/plan-2026-04-23-06.md` — Plan 09, the Link write-path
  foundation that Sprints 01 and 02 build on.
- `doc/notes/note-2026-04-23-03.md` lines 1028–1631 (Link + PID
  prototype), lines 2073–2198 (heterogeneous output design), lines
  2199–2248 (MTC).
- `doc/versions/version-0.1.md`, `version-0.2.md`, `version-0.3.md`,
  `version-0.4.md` — deferred lists carried forward.
