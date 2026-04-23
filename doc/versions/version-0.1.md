# agogo v0.1

## Goal

The smallest end-to-end demo of the design from `doc/agogo.md`:
**plug an audio-sync click into the laptop's audio input and watch a
downstream MIDI synth lock to it.** First runnable `agogo`
binary; six sprints total (two already merged).

The §4 "precision crown jewel" CV-out path is **not** in v0.1 — it
needs a DC-coupled audio interface to demo and adds a second host
backend to write before any output is visible. v0.1 ships the
cross-platform MIDI baseline only; CV moves to v0.2 once the pipeline
is proven.

## Sprint slots

The two foundation sprints are landed; four more deliver v0.1.

| # | Slug | Status | Scope |
|---|------|--------|-------|
| 01 | `plan-2026-04-22-01` | PR #2 (in flight) | `time/` — Cirklon grid algebra: `Tick`, `Time`, `TBase` lattice, `SwingConfig`, envelopes, five Galois connections via `connections::Conn`. |
| 02 | `plan-2026-04-22-02` | merged (PR #1) | `sync/` — peak detector with parabolic sub-sample interp, Type-II PI PLL, `PhaseSource` enum (Internal vs External). |
| 03 | `plan-2026-04-23-01` | this branch | `channel/` + `SampleTickConn` shim. Pure-logic per-channel scheduler: divider/shuffle/shift/offset transforms over Tick streams; the §7 closure-capturing bridge from Tick to Sample. |
| 04 | `plan-2026-04-2N-01` (TBD) | next | `out/midi` — MIDI clock byte emission (0xF8 ticks at PPQN/24, 0xFA/0xFC start/stop), `MidiSink` trait, sample-indexed timestamping. Pure trait + a synthetic test sink; no real backend yet. |
| 05 | `plan-2026-04-2N-01` (TBD) | next-next | `host/cpal` + `host/midir` + `rt/` — cross-platform audio input via cpal, MIDI output via midir. The audio callback hot loop and `rtrb`-based control plane. |
| 06 | `plan-2026-04-2N-01` (TBD) | last | `machine.rs` + `bin/agogo` — N-channel `Machine`, top-level binary, end-to-end CLI run. |

## v0.1 acceptance

- `cargo run -p agogo-cli -- agogo run --audio-in <device>
  --midi-out <port> --bpm 120` emits a steady MIDI clock that
  follows the input click within the PLL's jitter spec (Plan 02
  baseline: ±0.05 BPM steady-state at ≤ 200 µs input jitter).
- All `cargo test --workspace` properties green; `cargo clippy
  --all-targets -- -D warnings` clean; gitleaks job green.
- macOS-first but the test suite runs on Linux CI as well (cpal +
  midir are both cross-platform).

## Deferred to v0.2 (and beyond)

Tracked here so v0.1 reviewers can flag scope creep:

- **CV pulse output** — `out/audio` + cpal output side. The §4
  precision crown-jewel; needs DC-coupled output for E2E demo.
- **LFO render** — `channel/lfo` (sample-rate render of
  `time::envelope`).
- **Platform-native MIDI sinks** — `host/coremidi` (sub-µs via
  `MIDITimeStamp`), `host/jack` (frame-indexed), `host/winmm`,
  `host/alsa_midi`. midir gives ms-level for v0.1; native sinks are
  the precision-tier upgrade.
- **Preset I/O** — `serde` + `ciborium` round-trip of `Machine`
  state. Per agogo.md §10, one open question is whether presets are
  sample-rate-agnostic by virtue of the Tick-master design — worth
  testing as a proptest invariant when this lands.
- **Ableton Link** — wrap the C++ library as a `PhaseSource`
  variant. Open question per §10.
- **Network MIDI / rtp-MIDI backend.**
- **Transport FSM** — NEG/POS one-bar forerun semantics. Per §10
  needs a concrete FSM spec before implementation.
- **Remote control** via pitch-bend / CC parameter mapping.

## Reference

- `doc/agogo.md` — design brief, §3 module layout, §4 precision
  budget, §5 platform abstraction, §6 Cirklon mapping, §7 the
  `SampleTickConn` workaround, §8 dependencies, §11 out-of-scope.
- `doc/plans/plan-2026-04-22-01.md` — Plan 01 sprint spec.
- `doc/plans/plan-2026-04-22-02.md` — Plan 02 sprint spec.
- `doc/plans/plan-2026-04-23-01.md` — Plan 03 sprint spec (this branch).
