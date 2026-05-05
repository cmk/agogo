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


<!-- gh-id: 3186464012 -->
### Copilot on [`crates/host-cpal/src/cpal.rs:260`](https://github.com/cmk/agogo/pull/80#discussion_r3186464012) (2026-05-05 06:36 UTC)

`render_interleaved_output_chunks` zeroes the output buffer twice per callback (`interleaved.fill(...)` and then `output.fill(...)` for every chunk). Since the chunk loop covers the full buffer, the first fill is redundant and adds extra work on the realtime thread. Consider removing the outer fill (or the per-chunk fill) and only zeroing once, keeping the early-return path zeroing if needed.

<!-- gh-id: 3186464092 -->
### Copilot on [`crates/chan/src/sink/audio.rs:201`](https://github.com/cmk/agogo/pull/80#discussion_r3186464092) (2026-05-05 06:36 UTC)

`AudioClickState::reset` resets `click_counter` and `filter_q15` but leaves `rng_state` untouched. If the transport stops and later restarts in the same process, the click accent pattern will restart from 0 while the noise sequence continues mid-stream, which undermines the stated goal of deterministic click rendering across start/stop cycles. Consider storing the initial seed in `AudioClickState` (or re-deriving it) and resetting `rng_state` in `reset` as well.

<!-- gh-id: 4225775095 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-05-05 06:36 UTC](https://github.com/cmk/agogo/pull/80#pullrequestreview-4225775095))

## Pull request overview

This PR updates the generated (metronome/CV) audio-output path to target standard interleaved stereo (ch1-2) and tightens cpal stream selection to require an exact stereo f32 configuration. It also revises the audio click renderer to a deterministic filtered-noise timbre with per-channel state, and avoids allocating the MIDI SPSC ring for audio-only runs.

**Changes:**
- Switch cpal output-only streams from mono+fanout to strict interleaved stereo (2ch f32), rejecting mono-only and multichannel-only device configs.
- Replace square-wave clicks with deterministic low-pass filtered noise and introduce per-audio-channel click state + lane (L/R) routing.
- Avoid allocating the MIDI SPSC ring for audio-only runs by introducing a no-op MIDI sink, while preserving the existing ring path for MIDI/mixed runs.

### Reviewed changes

Copilot reviewed 10 out of 10 changed files in this pull request and generated 3 comments.

<details>
<summary>Show a summary per file</summary>

| File | Description |
| ---- | ----------- |
| README.md | Updates project status/docs to reflect stereo generated output and L/R click channel behavior. |
| doc/reviews/review-00080.md | Adds local review record for this PR’s changes. |
| doc/plans/plan-2026-05-05-01.md | Adds the implementation plan + verification checklist for the stereo metronome cleanup. |
| crates/host-cpal/src/cpal/callback.rs | Replaces the fixed SPSC producer field with a boxed `MidiSink` and adds `NoopMidiSink`. |
| crates/host-cpal/src/cpal.rs | Enforces stereo output config and renders directly into interleaved output (removes mono fanout). |
| crates/host-cpal/README.md | Updates backend documentation for stereo-only generated output behavior. |
| crates/core/src/transport.rs | Adds per-channel audio click state + stereo lane routing and updates tests for stereo behavior. |
| crates/cli/src/command/run.rs | Enforces max 2 audio click channels; avoids SPSC allocation when MIDI output is absent; sets stereo output config. |
| crates/cli/src/command/demo.rs | Updates `CallbackState` construction to use the new `midi_sink` field. |
| crates/chan/src/sink/audio.rs | Adds `AudioIo.output_channels`, introduces `AudioClickState`, and updates click/CV rendering for interleaved output. |
</details>






<!-- gh-id: 3186464108 -->
### Copilot on [`crates/host-cpal/README.md:7`](https://github.com/cmk/agogo/pull/80#discussion_r3186464108) (2026-05-05 06:36 UTC)

The plan reference here appears inconsistent/ambiguous: this PR’s plan is `doc/plans/plan-2026-05-05-01.md`, but the README says “Plan 2026-05-04 metronome cleanup”. Please update the referenced plan identifier/date so readers can find the correct plan document.


<!-- gh-id: 3186477204 -->
#### ↳ cmk ([2026-05-05 06:39 UTC](https://github.com/cmk/agogo/pull/80#discussion_r3186477204))

Fixed by removing the outer buffer fill while keeping the early-return zeroing path. The per-chunk fill remains the single normal-path clear.

<!-- gh-id: 3186477932 -->
#### ↳ cmk ([2026-05-05 06:40 UTC](https://github.com/cmk/agogo/pull/80#discussion_r3186477932))

Fixed the reference to the committed plan identifier: Plan 2026-05-05-01.

<!-- gh-id: 3186478654 -->
#### ↳ cmk ([2026-05-05 06:40 UTC](https://github.com/cmk/agogo/pull/80#discussion_r3186478654))

Leaving this as-is for this round per author direction. The important behavior here is stable channel identity and cutoff; restart-identical noise sequence is not required for the metronome cleanup.

<!-- gh-id: 3186524668 -->
### Copilot on [`crates/chan/src/sink/audio.rs:283`](https://github.com/cmk/agogo/pull/80#discussion_r3186524668) (2026-05-05 06:49 UTC)

The doc comment immediately above `render_audio_click_block` still says the output buffer is mono, but this function now renders into interleaved output via `io.output_channels` and the `output_channel` lane parameter. Please update the comment to reflect the new interleaved/stereo contract to avoid misleading API readers.

<!-- gh-id: 4225847329 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-05-05 06:49 UTC](https://github.com/cmk/agogo/pull/80#pullrequestreview-4225847329))

## Pull request overview

Copilot reviewed 10 out of 10 changed files in this pull request and generated 2 comments.





<!-- gh-id: 3186524699 -->
### Copilot on [`crates/host-cpal/src/cpal.rs:260`](https://github.com/cmk/agogo/pull/80#discussion_r3186524699) (2026-05-05 06:49 UTC)

`render_interleaved_output_chunks` clears each chunk with `output.fill(0.0)` before invoking the callback, but the AudioIo contract says the callback must fully write the output buffer anyway (and `Playhead::on_buffer` also zeroes `io.output` up-front). This extra fill adds avoidable work on the realtime thread; consider removing the per-chunk clear and relying on the callback/Playhead to produce silence.

<!-- gh-id: 3186541315 -->
#### ↳ cmk ([2026-05-05 06:52 UTC](https://github.com/cmk/agogo/pull/80#discussion_r3186541315))

Fixed: the doc comment now describes interleaved output and the selected output lane instead of the old mono feature-slice contract.

<!-- gh-id: 3186542887 -->
#### ↳ cmk ([2026-05-05 06:53 UTC](https://github.com/cmk/agogo/pull/80#discussion_r3186542887))

Fixed: removed the normal-path per-chunk clear. The helper still clears the buffer before returning on invalid zero-channel or zero-scratch configurations.
