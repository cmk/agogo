# agogo v0.1

## Goal

The smallest end-to-end demo of the design from `doc/agogo.md`:
**plug an audio-sync click into the laptop's audio input and watch a
downstream MIDI synth lock to it.** First runnable `agogo` binary.

The §4 "precision crown jewel" CV-out path is **not** in v0.1 — it
needs a DC-coupled audio interface to demo and adds a second host
backend to write before any output is visible. v0.1 ships the
cross-platform MIDI baseline only; CV moves to v0.4 once the pipeline
is proven.

## Status as of 2026-04-24

Three foundation sprints shipped as originally scoped, then work
pivoted into a four-plan detour (CLI infra, fxp refactor, Link
read-only slice, post-fxp numerical-cast hardening). Two worktrees
are now wrapping up the tail of the detour. Output chain (MIDI out →
hosts → binary) still to go.

### Merged on main

| Slug | PR | Scope |
|------|----|-------|
| `plan-2026-04-22-01` | #2 | `time/` — Cirklon grid algebra: `Tick`, `Time`, `TBase` lattice, `SwingConfig`, envelopes, Galois connections via `connections::Conn`. |
| `plan-2026-04-22-02` | #1 | `sync/` — peak detector with parabolic sub-sample interp, Type-II PI PLL, `PhaseSource` enum (Internal vs External). |
| `plan-2026-04-23-01` | #4 | `channel/` + `SampleTickConn` — pure-logic per-channel scheduler (divider/shuffle/shift/offset) plus the §7 Tick↔Sample bridge. |
| `plan-2026-04-23-02` | #3 | CLI parser migration: clap → bpaf. Pure infra. |
| `plan-2026-04-23-03` | #5 | Fixed-point refactor across `sync` + envelope + CLI. Introduces `Tempo`, `Phase`, `Q15` fxp types backed by the sibling `connections` crate. Foundational for every subsequent plan. |
| `plan-2026-04-23-04` | #6 | Link read-only structural slice — `agogo-host-link` crate scaffold, `PhaseSourceImpl` trait, `PhaseSource::Custom(...)` variant. |
| *(hotfix)* | #7 | `fix/host-link-phase-type` — align `phase_at_sample` return type with fxp `Phase`. Pre-rule standalone fix. |
| `plan-2026-04-23-05` | #8 | Link phase bridge — `LinkClock::phase_at_sample` over a static `HostTimeAnchor`; `agogo link probe` CLI. |
| `plan-2026-04-23-07` | #9 | Post-fxp enforcement MR #1: scaffolding + PLL migration. `PicoSampleConn`, `tempo_to_hz` / `bits_q48_16_to_seconds` helpers, PLL consumes the new helpers. |
| `plan-2026-04-23-08` | #10 | Rev bump to `connections` ccc4d85 + rename migration (sprint opener for the plan below). |

### In flight

| Worktree / branch | Scope |
|-------------------|-------|
| `plan/2026-04-23-06` (Plan 09) | Link bidirectional foundation: tempo push, `{Stopped, Playing}` transport FSM via `rust-fsm 0.7`, per-channel `snap_to_quantum: Option<Quantum>`. Closes the Link detour for v0.1; PID sync + forerun transport are v0.5 territory. |
| `plan/2026-04-23-08` (Plan 11 MR #2) | Post-fxp boundary sweep: Channel state → `Micro`, CLI argv f64 via Conns, `ProbeRow` drops stored floats, `LinkClock` surface exposes `Tempo`, `scripts/check-floats.sh` grep gate + CI wire, CLAUDE.md + review-calibration rules. Closes the numerical-cast / Conn-usage hardening. |

### Still to ship for v0.1

| Slug | Scope |
|------|-------|
| TBD | `out/midi` — MIDI clock byte emission (0xF8 ticks at PPQN/24, 0xFA/0xFC start/stop), `MidiSink` trait, sample-indexed timestamping. Pure trait + a synthetic test sink; no real backend yet. |
| TBD | `host/cpal` + `host/midir` + `rt/` — cross-platform audio input via cpal, MIDI output via midir. The audio callback hot loop and `rtrb`-based control plane. |
| TBD | `machine.rs` + `bin/agogo` — N-channel `Machine`, top-level binary, end-to-end CLI run. |

## Detour context

v0.1's original plan had three output-chain sprints (`out/midi`,
`host/cpal`+`midir`+`rt`, `machine`+binary) directly after the
`channel/` foundation. Four items landed between slot 03 and the
current state that weren't in the original plan:

1. **CLI parser migration** (`plan-2026-04-23-02`). Pure infra, no
   scope impact.
2. **Fixed-point refactor** (`plan-2026-04-23-03`). Flipped `f32`/`f64`
   math in `sync` + envelope + CLI to fixed-point via the sibling
   `connections` crate's Galois-connection API. Foundational — every
   subsequent plan is downstream of this.
3. **Link read-only slice** (`plan-2026-04-23-04`, `plan-2026-04-23-05`,
   `plan/2026-04-23-06` in flight). Structural skeleton for Ableton
   Link as a `PhaseSource`. The in-flight Plan 09 is the last Link
   work v0.1 carries; full Link (PID sync, per-buffer anchor, forerun
   transport) is scoped to v0.5.
4. **Post-fxp enforcement** (`plan-2026-04-23-07`, `plan-2026-04-23-08`
   landed; `plan/2026-04-23-08` MR #2 in flight). No stored `f32`/`f64`
   outside five documented exceptions; every numerical conversion
   routes through a named `Conn`; CI grep gate + CLAUDE.md rules lock
   it in.

Once the two in-flight branches merge, the detour is done and work
returns to the output-chain sprints listed above.

## v0.1 acceptance

- `cargo run -p agogo-cli -- agogo run --audio-in <device>
  --midi-out <port> --bpm 120` emits a steady MIDI clock that
  follows the input click within the PLL's jitter spec
  (`plan-2026-04-22-02` baseline: ±0.05 BPM steady-state at ≤ 200 µs
  input jitter).
- All `cargo test --workspace` properties green; `cargo clippy
  --all-targets -- -D warnings` clean; gitleaks job green.
- `scripts/check-floats.sh` exits 0 (post-hardening).
- macOS-first but the test suite runs on Linux CI as well (cpal +
  midir are both cross-platform).

## Deferred to v0.2+ (tracked so reviewers can flag scope creep)

- **192 → 960 PPQN retrofit** — v0.2. Pentuplet subdivisions and
  integer-exact samples-per-tick at 48 k / 96 k.
- **CV pulse output** — `out/audio` + cpal output side. v0.4.
- **LFO render** — `channel/lfo` (sample-rate render of
  `time::envelope`). post-v0.5.
- **Platform-native MIDI sinks** — `host/coremidi` (sub-µs via
  `MIDITimeStamp`), `host/jack` (frame-indexed), `host/winmm`,
  `host/alsa_midi`. midir gives ms-level for v0.1; native sinks are
  the precision-tier upgrade. post-v0.5.
- **Preset I/O** — `serde` + `ciborium` round-trip of `Machine`
  state. post-v0.5. Per `doc/agogo.md` §10, one open question is
  whether presets are sample-rate-agnostic by virtue of the
  Tick-master design.
- **Ableton Link PID sync + forerun transport** — v0.5. The
  read-only slice + write-path foundation (tempo push, FSM seam,
  quantum snap) land in v0.1 via the detour; PID-smoothed reference
  and forerun states are v0.5 Sprint 01 / 02.
- **Network MIDI / rtp-MIDI backend** — post-v0.5.
- **Remote control** via pitch-bend / CC parameter mapping —
  post-v0.5.

## Reference

- `doc/agogo.md` — design brief, §3 module layout, §4 precision
  budget, §5 platform abstraction, §6 Cirklon mapping, §7 the
  `SampleTickConn` workaround, §8 dependencies, §10 open questions,
  §11 out-of-scope.
- `doc/plans/plan-2026-04-22-01.md` — `time/` sprint.
- `doc/plans/plan-2026-04-22-02.md` — `sync/` sprint.
- `doc/plans/plan-2026-04-23-01.md` — `channel/` + `SampleTickConn`.
- `doc/plans/plan-2026-04-23-02.md` — CLI parser migration.
- `doc/plans/plan-2026-04-23-03.md` — fxp refactor.
- `doc/plans/plan-2026-04-23-04.md` — Link read-only.
- `doc/plans/plan-2026-04-23-05.md` — Link phase bridge.
- `doc/plans/plan-2026-04-23-06.md` — Link write-path (Plan 09, in flight).
- `doc/plans/plan-2026-04-23-07.md` — Post-fxp enforcement (MR #1 scope landed).
- `doc/plans/plan-2026-04-23-08.md` — Post-fxp boundary sweep (Plan 11, MR #2 in flight).
- `doc/versions/version-0.2.md` — PPQN retrofit; deferred list carried forward.
- `doc/versions/version-0.5.md` — Link PID sync + forerun transport continuation.
