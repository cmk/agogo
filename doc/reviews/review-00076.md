# Review 00076

## Summary

### What Changed

- Added the plan for a public-demo CV pulse MVP.
- Taught channel specs to parse and lower `dev=cv,mode=pulse` into
  `Channel::Cv`.
- Added fixed-shape CV pulse rendering through the existing mono audio output
  path, including bipolar reset state across buffer boundaries.
- Extended offline render JSON with positive/negative audio peak diagnostics
  and added a hardware-free CV pulse render test.
- Updated README and roadmap docs to distinguish the CV pulse MVP from the
  later heterogeneous output/calibration work.

### Verification

- `cargo test -p agogo-chan cv --quiet`
- `cargo test -p agogo-core transport --quiet`
- `cargo test -p agogo-cli --test render --quiet`
- `cargo run -p agogo-cli --bin agogo -- render --source internal --bpm 120 --sr 48000 --duration-bars 1 --ch 'id=cv,dev=cv,mode=pulse,grid=t4,out=diag'`
- `scripts/check-pii.sh`
- `cargo test --workspace --quiet`
- `cargo clippy --all-targets --quiet -- -D warnings`

## Local review (2026-05-04)

**Branch:** plan/2026-05-04-04
**Commits:** 3 (origin/main..plan/2026-05-04-04)
**Reviewer:** Codex (`codex review --base origin/main`)

---

The offline renderer works, but the patch exposes and documents `dev=cv,mode=pulse` for `agogo run` without updating runtime sink selection, so the CV-only run path silently opens no output buffer and produces no signal.

Review comment:

- [P2] Route CV pulse channels to the audio output host — crates/chan/src/channel/spec/parser.rs:398-398
  When a user runs the newly accepted/documented CV-only spec (`dev=cv,mode=pulse,out=default`), `agogo run` still only treats `ChannelSpecRole::Audio` / `Channel::Audio` as requiring an audio output device. As a result `mix.has_audio` is false, cpal opens the input-only config with an empty `AudioIo::output`, and `render_cv_pulse_block` returns without writing any pulse. Include CV pulse channels in the output-device selection and channel mix so the advertised hardware-backed CV path actually opens an output stream.

Resolution: fixed by routing `ChannelSpecRole::Cv` and `Channel::Cv` through
the existing audio-output sink selection in `agogo run`, plus a feature-gated
CLI regression test for `dev=cv,mode=pulse`.
