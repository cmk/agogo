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
- `cargo check -p agogo-cli --features cpal --quiet`
- `cargo check --manifest-path crates/host-midi/Cargo.toml --quiet`
- `cargo check --manifest-path crates/host-link/Cargo.toml --quiet`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test --workspace`

## Local review (2026-05-03)

**Branch:** plan/2026-05-04-03
**Commits:** 4 (origin/main..plan/2026-05-04-03)
**Reviewer:** Codex (`codex review --base origin/main`)

---

The default workspace tests pass, but optional host backend builds are broken by unresolved `agogo::chan` paths in crates that do not depend on the facade module exposing that path. This makes feature builds such as `agogo-cli --features cpal` fail to compile.

Review comment:

- [P1] Keep detached host crates on a resolvable API path — crates/host-cpal/src/cpal/callback.rs:14-15
  When building any optional host backend, these `agogo::chan` imports do not resolve because the detached host crates still depend only on `agogo-core` and define a local `agogo::core` shim, not the facade crate's new `agogo::chan` module. For example, `cargo check -p agogo-cli --features cpal` fails here, and the same pattern also breaks `host-midi` and `host-link`; either add/update the local shim/dependencies or import through the existing `core_impl`/`agogo::core` surface.

Resolution:

- Added a local public `agogo::chan` shim to each detached host crate
  (`host-cpal`, `host-midi`, `host-link`) that re-exports the pure
  channel/conn/control/sink/test/time surface from `agogo-core`.
- Verified:
  - `cargo check -p agogo-cli --features cpal --quiet`
  - `cargo check --manifest-path crates/host-midi/Cargo.toml --quiet`
  - `cargo check --manifest-path crates/host-link/Cargo.toml --quiet`
