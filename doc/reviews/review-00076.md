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

## Local review (2026-05-04)

**Branch:** plan/2026-05-04-04
**Commits:** 4 (origin/main..plan/2026-05-04-04)
**Reviewer:** Codex (`codex review --base origin/main`)

---

The new CV pulse renderer can drop pulses for accepted schedules where events are adjacent because the bipolar reset cancels the next event's impulse. This is a functional correctness issue in the newly added rendering path.

Review comment:

- [P2] Preserve pulse when reset overlaps next event — crates/chan/src/sink/audio.rs:291-295
  For CV channels with adjacent scheduled events (possible at very fine grids/high but validator-accepted tempos, e.g. one event per sample), the bipolar reset for event N is mixed into the same sample as the positive impulse for event N+1, so `-1.0 + 1.0` clamps to `0.0` and the second pulse disappears. This violates the advertised “every scheduled pulse writes the positive impulse at the intended sample” behavior; handle overlaps explicitly or reject configurations where bipolar pulses cannot fit.

Resolution: fixed by skipping a bipolar reset when it would land on another
scheduled pulse sample, preserving the positive impulse. Added a spot test and
extended `cv_impulse_sample_exact` to cover adjacent events.

## Local review (2026-05-04)

**Branch:** plan/2026-05-04-04
**Commits:** 5 (origin/main..plan/2026-05-04-04)
**Reviewer:** Codex (`codex review --base origin/main`)

---

The new CV pulse renderer handles adjacent events within one channel, but still allows cross-channel bipolar resets to cancel another channel's scheduled pulse on the shared mono output.

Review comment:

- [P2] Preserve pulses across CV channels — crates/chan/src/sink/audio.rs:297-301
  When two accepted CV pulse channels share the current mono output and one channel has an event one sample before another, this reset check only sees events from the current channel, so the first channel writes `-1.0` into the second channel's positive pulse sample and the final mix clamps to `0.0`. That makes a scheduled pulse disappear for multi-channel CV specs using the same output; the overlap handling needs to account for all CV pulse events in the mixed buffer or otherwise reject these schedules.

Resolution: fixed by making CV positive pulses full-scale writes and making
CV negative resets skip samples that already contain a full-scale CV pulse.
Added cross-channel spot tests for both render orders.

## Local review (2026-05-04)

**Branch:** plan/2026-05-04-04
**Commits:** 6 (origin/main..plan/2026-05-04-04)
**Reviewer:** Codex (`codex review --base origin/main`)

---

I did not find any discrete correctness issues in the diff. The added CV pulse parsing, rendering, runtime sink selection, and offline diagnostics are covered by tests and the workspace test suite passes.


## Local review (2026-05-04)

**Branch:** plan/2026-05-04-04
**Commits:** 6 (origin/main..plan/2026-05-04-04)
**Reviewer:** Codex (`codex review --base origin/main`)

---

I did not identify any actionable correctness issues in the changes relative to the base branch. The parser, runtime routing, CV pulse rendering, and offline diagnostics appear consistent with the intended MVP behavior.

