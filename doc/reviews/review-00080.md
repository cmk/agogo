# Review 00080 - Stereo metronome cleanup

## Summary

Generated audio metronome output now targets normal stereo ch1-2 instead of
mono fanout or wide multichannel device streams. cpal output-only streams require
an exact stereo f32 config; mono-only and multichannel-only configs fail with
`UnsupportedConfig`.

The audio click renderer now uses deterministic low-pass filtered white noise
with lower gain and per-audio-channel render state. `agogo run` accepts one or
two `dev=audio,mode=click` channels: the first writes left only, the second
writes right only, and a third errors before device open. CV pulse output is
preserved as dual-mono stereo.

Audio-only generated-output runs no longer allocate the MIDI SPSC ring. MIDI and
mixed MIDI+audio runs still allocate one SPSC producer/drain path.

Verification:

- `cargo test --workspace`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --offline --manifest-path crates/host-cpal/Cargo.toml cpal -- --nocapture`
- `cargo clippy --offline --manifest-path crates/host-cpal/Cargo.toml --all-targets -- -D warnings`
- `cargo fmt --all -- --check`
- `scripts/check-floats.sh`
- `scripts/check-layers.sh`

## Local review (2026-05-04)

**Branch:** plan/2026-05-05-01
**Commits:** 3 (origin/main..plan/2026-05-05-01)
**Reviewer:** Codex (`codex review --base origin/main`)

---

The changes compile and the workspace tests pass. I did not identify any actionable correctness issues introduced by this diff.

