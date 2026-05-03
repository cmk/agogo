# PR #69 - Declare MIDI timing capabilities

## Summary

Adds explicit MIDI output timing capability reports so current output paths
state whether `at_sample` is honored or metadata-only. The core MIDI sink
module now exposes `MidiTimingCapability`, scheduling/latency/`at_sample`
classes, and a `MidiTimingCapabilities` trait with no blanket default.

Current sinks report immediate best-effort timing: `TestSink`,
`DiagnosticSink`, `RtProducer`, and `MidirSink` preserve intended sample
indices as metadata but do not claim native timestamped scheduling. The new
`DiagnosticSink` records intended samples, payload bytes, drain order, optional
observed sample positions, and integer delay summaries outside the realtime
callback path.

The SPSC and midir tests pin the truthful capability reports, `RtProducer`
FIFO preservation of `at_sample`, and diagnostic delay summaries. v0.2/v0.3
docs now mark this as the first timestamped-output step: truthfulness and
diagnostics now, native CoreMIDI/JACK-style scheduling later.

Validation:

- `cargo fmt --all -- --check`
- `scripts/check-pii.sh`
- `scripts/check-floats.sh`
- `scripts/check-layers.sh`
- `scripts/check-connections.sh`
- `scripts/check-boundary-panics.sh`
- `cargo test --workspace --quiet`
- `cargo clippy --all-targets --quiet -- -D warnings`
- `cargo test --quiet` in `crates/host-cpal`
- `cargo test --quiet` in `crates/host-midi`

## Local review (2026-05-03)

**Branch:** plan/2026-05-03-04
**Commits:** 3 (origin/main..plan/2026-05-03-04)
**Reviewer:** Codex (`codex review --base origin/main`)

---

The capability API does not enforce or preserve timing reports through the main sink abstraction, and the host-midi verification does not actually exercise the backend implementation it claims to verify. These issues undermine the central behavior added by the patch.

Full review comments:

- [P2] Require timing reports on every MidiSink — crates/core/src/sink/midi.rs:114-115
  Because `MidiTimingCapabilities` is independent of `MidiSink`, the main drain API can still accept `Arc<dyn MidiSink + Send + Sync>` and the CLI can coerce `MidirSink` into that type, at which point the selected output sink no longer exposes `timing_capability()`. In that scenario a new backend can implement only `MidiSink` and still enter production output without any explicit timing claim, which defeats this PR's purpose of distinguishing metadata-only versus timestamped output; consider making `MidiSink` require the timing-capability trait or carrying a combined trait object through the drain path.

- [P2] Exercise MidirSink capability reporting in the test — crates/host-midi/src/midir.rs:136-136
  This test constructs the expected best-effort value directly instead of querying `MidirSink`'s implementation, so changing `MidirSink::timing_capability()` to return the wrong backend name or claim timestamped support would still pass. Since the verification table claims this pins `MidirSink`'s report, route the assertion through the backend's advertised capability, such as an associated capability used by the impl, rather than the generic constructor.

## Local review resolution (2026-05-03)

- Fixed: `MidiSink` now inherits `MidiTimingCapabilities`, so any production
  sink that enters the drain path must make an explicit timing claim.
- Fixed: `MidirSink` exposes `TIMING_CAPABILITY`, the trait impl returns that
  value, and the test asserts the backend-advertised capability rather than a
  separately constructed expected value.
