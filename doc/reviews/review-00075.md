## Summary

Implements Plan 2026-05-04-03.

First performs the mechanical public API cleanup requested before the render
work: adds the `agogo::chan` facade and moves downstream pure
channel/time/control/sink/conn imports to it; narrows `agogo::core` to runtime
orchestration exports; removes the public `conn::boundary` module by exposing
its helpers through `conn::float`; renames `conn::sample` to `conn::rate`; and
renames the sample-rate marker/connection family from `SXYZ` to `RXYZ`.

Then adds a deterministic hardware-free render path. `agogo-core` now exposes
`render_offline`, which drives the same `Playhead::on_buffer` path used by host
callbacks with silent input, scratch audio output, and an in-memory MIDI sink.
`agogo render` parses the same channel specs as `agogo run`, currently accepts
only `--source internal`, bounds duration by bars, and prints deterministic JSON
with MIDI records, offline timing metadata, dropped count, and audio-output
summary fields. The README now uses this as the primary hardware-free runtime
demo.

Verification run locally:

- `scripts/check-pii.sh`
- `scripts/check-floats.sh`
- `scripts/check-layers.sh`
- `scripts/check-connections.sh`
- `scripts/check-boundary-panics.sh`
- `cargo test -p agogo-core render --quiet`
- `cargo test -p agogo-cli --test render --quiet`
- `cargo run -p agogo-cli --bin agogo -- render --source internal --bpm 120 --sr 48000 --duration-bars 1 --buffer-frames 4096 --ch id=three,dev=midi,mode=clock,grid=t2t,out=diag --ch id=two,dev=midi,mode=clock,grid=t2,out=diag`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test --workspace`
