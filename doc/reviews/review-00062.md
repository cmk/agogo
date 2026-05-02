# PR #62 - Audio metronome test feature

## Summary

Adds a minimal generated-tone audio metronome path for user testing:
`dev=audio,mode=click` now lowers to an audio channel and writes fixed
PCM clicks into the cpal output buffer. The CLI intentionally does not
surface sound-shaping controls; tone shape, duration, amplitude, and
accent behavior are fixed in core.

The old MIDI click path remains unchanged. MIDI ports are now opened
only when a MIDI channel exists, so an audio-only metronome spec does
not require MIDI hardware. `agogo run` uses output-only cpal for audio
click channels so `--source internal` can produce audible clicks without
an audio input device.

Notable constraints:

- `dev=audio` supports only `mode=click` in this test-feature slice.
- MIDI click keys (`note`, `vel`, `mch`, `accent-*`) are rejected on
  audio specs.
- `--source external` plus audio output is deferred until duplex
  input+output callback wiring exists.

Verification:

- `cargo fmt --all -- --check`
- `cargo fmt --manifest-path crates/host-cpal/Cargo.toml -- --check`
- `scripts/check-pii.sh`
- `scripts/check-floats.sh`
- `scripts/check-layers.sh`
- `cargo test --workspace`
- `cargo test -p agogo-cli --features run`
- `cargo test --manifest-path crates/host-cpal/Cargo.toml`
- `cargo clippy --all-targets -- -D warnings`
- `cargo clippy -p agogo-cli --features run --all-targets -- -D warnings`
- `cargo clippy --manifest-path crates/host-cpal/Cargo.toml --all-targets -- -D warnings`

## Local review (2026-05-02)

**Branch:** plan-2026-05-02-06
**Commits:** 3 (origin/main..plan-2026-05-02-06 before this review note)
**Reviewer:** Codex in-session local-only review

---

The canonical `scripts/local_review.sh` transition could not run in
this environment: it invokes `codex review --base origin/main`, and the
sandbox rejected that external review call because it would disclose
private repository context to the Codex review service. I did not try to
bypass that restriction.

Local-only review covered the target-selection path in `agogo run`, the
cpal output-only callback path, parser cross-key validation, and the
stateless audio renderer. No must-fix issues were found.

Residual risk: the generated click renderer is intentionally stateless,
so a click that starts near the end of one callback buffer is truncated
instead of continuing into the next buffer. This is documented in the
plan review section and is acceptable for the current test feature; WAV
playback or productized sound shaping should revisit it.
