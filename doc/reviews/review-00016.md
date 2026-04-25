# PR #16 — Plan 14: Machine + agogo run + bin/agogo

## Summary

Final v0.1 sprint. Generalises Plan 13's single-channel `agogo demo
run` into the user-facing `agogo run` runner. After this PR, the v0.1
acceptance scenario in `doc/versions/version-0.1.md` is reachable
end-to-end.

### What changed

- **`agogo_core::machine`** (new module): N-channel `Machine<R>`
  orchestrator. Owns `Vec<Channel>`, shared `PhaseSource<R>` /
  `SampleTickConn`, transport state, and a reused per-channel
  scratch buffer. `on_buffer` is the single buffer-driven entry the
  audio callback calls — feeds PCM to the phase source, computes one
  per-buffer transport byte, then iterates channels through Plan 12's
  `render_channel_block`.
- **`TransportPolicy`** enum (`Internal | LinkDriven | Scripted`) +
  `MachineStopHandle` (cross-thread `AtomicBool` for Ctrl-C
  signalling). Internal emits `Start` at first buffer / `Stop` after
  `request_stop`. LinkDriven reads `LinkSession::is_playing()`
  through a closure and emits on transitions. Scripted is a test
  fixture.
- **`agogo_core::machine::spec`**: parser for the docker-style
  `--ch key=val,...` mini-language. Required keys `div` / `dev`;
  optional `id` / `out` / `swing` / `swing-mult` / `shift-ms` /
  `offset-ms` / `snap-quantum-us`. Quoted values support embedded
  spaces and commas. `dev=audio` is a hard error (reserved for v0.4
  per `version-0.1.md:99`).
- **`crates/host-link/src/source.rs`** (new): `LinkPhaseSource`
  adapter that wraps `LinkSession` in `Arc<Mutex<_>>` and impls
  `PhaseSourceImpl`. The audio thread reads phase via the lock; the
  control thread holds a `LinkSessionHandle` clone for
  `is_playing` / `poll_transport` / `set_tempo` / `user_stop`.
  Lock contention is bounded — sub-µs on the audio thread, human
  pace on the control thread. v0.5 Sprint 02 swaps to seqlock when
  precision matters.
- **`crates/host-cpal/src/cpal/callback.rs`**: `CallbackState<R>`
  shrinks to a 2-field wrapper around `Machine<R>` + `RtProducer`.
  `on_buffer` is one delegating call. The transport
  `Option<MidiRtByte>` parameter goes away — Machine drives that
  internally.
- **`crates/cli/src/run.rs`** (new): `agogo run` handler. Six-rate
  static dispatch (`S44 | S48 | S88 | S96 | S176 | S192`). Three
  sources (internal / external / link). Ctrl-C handler via the
  `ctrlc` crate; tear-down order is `request_stop → 50 ms grace
  → drop stream → drop drain`. Hidden `--max-duration-ms` for
  deterministic test runs.
- **`bin/agogo` binary entry**: explicit `[[bin]] name = "agogo"`
  in `crates/cli/Cargo.toml`. The default `agogo-cli` binary stays
  available for one release; v0.2 removes it.
- **`run = ["demo", "link", "dep:ctrlc"]`** feature on
  `agogo-cli`. Adds `ctrlc = "3"` as the only new dep
  (small, MIT, cross-platform).
- **`max_events_for_buffer`** moved from `host-cpal` to
  `agogo_core::channel::scheduler` so `Machine` can size its pool
  without depending on `host-cpal`. host-cpal re-exports for
  back-compat.
- **`scripts/check-floats.sh` + `CLAUDE.md`** allowlist gains four
  files (machine.rs, machine/spec.rs, host-link/source.rs,
  cli/run.rs); count goes from ten → fourteen exception modules,
  documented inline.
- **`doc/versions/version-0.1.md`** updated: status table reflects
  Plans 09/12/13 as merged, Plan 14 as in flight; acceptance
  scenario respelled to the docker-style `--ch` syntax.

### Tests

- 8 spec parser unit tests + `spec_round_trip` proptest.
- 4 Machine tests + 2 Machine proptests (`multi_channel_independent
  _dispatch`, `transport_link_driven_emits_on_transitions`).
- 1 host-link `LinkPhaseSource` round-trip + 1 no-deadlock
  contention proptest.
- 4 CLI `run::tests` smoke tests covering the no-device error paths
  (empty `--ch`, `dev=audio`, unsupported rate, malformed key).
- Existing host-cpal `callback_emits_expected_clock_schedule`
  re-pinned against the Machine-backed callback for byte-equivalence
  with Plan 13's original.

234 → 255 workspace tests; clippy clean across all features;
`scripts/check-floats.sh` clean; gitleaks clean.

### Verification end-to-end

```
cargo run --bin agogo --features run -- run \
    --bpm 120 --sr 48000 --source external \
    --audio-in default --ch dev=midi,div=t32t,out=<midi-port>
```

Emits steady MIDI clock that follows an audio click within Plan
02's PLL spec (±0.05 BPM steady-state at ≤ 200 µs input jitter).
Ctrl-C tears down cleanly: `0xFC` Stop byte at the next buffer
boundary, cpal stream paused, drain thread joined, exit 0 within
~150 ms.

### MR split

One PR. Plan 14's task graph is tightly coupled — Machine needs the
spec parser to be useful, the CLI needs Machine, the binary entry
needs the CLI shape. Splitting fragments review.

### What's deferred

- Multi-port MIDI dispatch (`MidiSinkRouter`) → v0.2.
- `dev=audio` (CV pulse + analog LFO) → v0.4.
- Live BPM knob / atomic param bridge → v0.3.
- PID-smoothed Link follower + forerun transport → v0.5.
- Preset I/O (`--config presets/foo.toml`) → post-v0.5.
- `agogo-cli` binary alias removal → v0.2 cleanup.
- `machine_alloc_free_per_buffer` (alloc_tracker fixture) → v0.5
  if RT-allocation regressions surface; existing
  `callback_does_not_realloc_events` covers the contract for v0.1.
- Hardware-fixture loopback tests → v0.5 acceptance suite.
