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
