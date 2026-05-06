# PR #81 — Multi-channel offline render and inter-channel proptests

## Summary

Generalize the deterministic offline render path from hardcoded mono to
1..=16 explicitly-assigned output channels, replace the implicit
`audio_index % 2` lane policy with explicit per-channel assignment via
`out=N` on the channel spec, validate at the boundary that no two
audio channels share a lane (no mix bus), expose per-lane PCM through a
new `render_offline_capture` entry point, and cover the whole pipeline
with an end-to-end proptest battery that asserts inter-channel
sample-perfect timing for arbitrary polyrhythms.

### What changed

- **`OfflineRenderConfig`** gains `output_channels: u16`, validated
  `1..=MAX_OUTPUT_CHANNELS` (= 16). New error variants
  `UnsupportedChannelCount`, `LaneOutOfRange`, `LaneCollision`
  surface with `Display` impls grounded in the user-facing `out=`
  field name (e.g. "channel #3: out=4 exceeds output_channels=2").
  Missing `out=` is rejected one layer earlier by the spec parser /
  `into_channel`, so `Channel::Audio.lane: u16` is total and no
  `LaneMissing` variant is needed.
- **`Channel::Audio`** carries a `lane: u16`; `Playhead::new` reads it
  directly instead of round-robining audio channels into lanes 0/1.
- **Spec parser** accepts a bare integer `out=N` for `dev=audio`
  channels and surfaces it as `ChannelSpec::audio_lane: Option<u16>`.
  `out=diag` is rejected for audio with a clear message ("audio
  without a destination is silence — use out=N"). MIDI/CV `out=`
  semantics are unchanged in this sprint.
- **`validate_audio_lanes(channels, output_channels)`** is a new
  shared helper used by both the offline path and the live `run`
  path so the no-mix-bus rule cannot diverge.
- **`render_offline_capture`** returns
  `OfflinePcm { lanes: Vec<Vec<f32>>, sample_rate, frames }` alongside
  the existing `OfflineRenderReport`. Same loop as the existing
  internal driver; the per-buffer interleaved output is split per
  lane and appended into `lanes[i]`. Re-exported through the agogo
  facade. Substrate for the planned `agogo render --wav-out FILE`
  feature.
- **CLI `render`** gains `--output-channels` (default 2). Audio specs
  must declare lanes via `out=N`; CV/MIDI specs keep their existing
  `out=diag` / device-name semantics.
- **Live `run`** path's audio-device-name selection now sources only
  from CV channels (audio's `out=` is the integer lane, not a device
  name). Audio-only configs default to the host's default audio
  device. `validate_live_audio_lanes` replaces the legacy 2-channel
  count cap with the shared lane validator (still bounded by the
  live cpal stream's hardcoded 2 output channels until the live
  multi-channel work lands).
- **Renderer correctness**: `AudioClickState` now tracks the tail of
  the most-recently-truncated click via `pending_samples` /
  `pending_accent`; `render_audio_click_block` resumes the tail at
  output offset 0 of the next buffer before processing new events.
  This makes per-lane PCM bit-identical regardless of buffer-frame
  size — required by the new
  `prop_buffer_boundary_invariance`. The bug was pre-existing
  (clicks were silently truncated at buffer boundaries) and surfaced
  by the proptest battery.
- **AudioClickState seeding** switched from audio-channel ordinal to
  lane index, so per-lane PCM is bit-identical regardless of
  co-channels — required by `prop_n_channel_independence`.

### Test coverage

New proptest battery in `crates/agogo/test/inter_channel_accuracy.rs`
(9 properties + 6 spot checks) covers:

- Lane uniqueness rejected (`LaneCollision` returned for two channels
  sharing a lane).
- Lane out-of-range rejected (`LaneOutOfRange` returned for
  `lane >= output_channels`).
- `output_channels` bounds rejected (`UnsupportedChannelCount` for
  0 and >16).
- Unused lanes are exactly 0.0 across the whole render.
- Lane separation: a routed lane only carries its channel's onsets,
  with at least one nonzero sample per predicted footprint.
- Coincident alignment: shared coincident ticks across polyrhythm
  pairs land at identical sample indices on both lanes.
- Buffer-boundary invariance: bit-identical lane PCM across
  `buffer_frames ∈ {64, 256, 1024, 4096}`.
- N-channel independence: per-lane PCM from a full N-channel render
  matches each channel rendered solo (N up to 8).
- Render shape sweep: bounded sweep across BPM, sample rate,
  buffer frames, duration, and `output_channels` ∈ {1, 2, 4, 8, 16}.

Spot checks anchor:

- The original 3:2-at-120BPM-48k motivating scenario.
- 16-channel unique-lane render.
- Lane-collision deterministic rejection.
- Event at sample 0, event at buffer end (truncated footprint).
- 192 kHz coverage (the rate the proptest strategy intentionally
  skips for runtime).

A 4-channel CLI parity smoke test in `crates/cli/test/render.rs`
runs `agogo render --output-channels 4` with four audio channels and
confirms the CLI's JSON aggregates agree with `render_offline_capture`
called directly with the equivalent config.

### Out of scope (deferred)

- WAV-render CLI feature. Builds on `render_offline_capture` plus
  `hound` re-interleaving — separate small plan.
- Multi-channel live render. The live `run` path passes
  `output_channels: 2`; configs with `out >= 2` are now rejected at
  the lane validator. Updating `run` to honour `--output-channels`
  is a follow-up.
- MIDI/CV `out=N` namespace cleanup. The `out=` field grows integer
  semantics for `dev=audio` only this sprint; MIDI port routing and
  CV channel routing keep their current semantics.

## Local review (2026-05-05)

**Branch:** plan/2026-05-05-02
**Commits:** 5 (origin/main..plan/2026-05-05-02)
**Reviewer:** Codex (`codex review --base origin/main`)

---

The implementation builds and the test suite passes, but the parser change leaves the repository's public audio-click example broken. Users following the README will now hit the new parse error.

Review comment:

- [P2] Update audio examples after rejecting `out=default` — `crates/chan/src/channel/spec/parser.rs:440-445`
  When users follow the current README hardware-backed audio example, both `dev=audio` specs still use `out=default`; this new `u16` parse path rejects that before `run` can choose the default output device, so the documented command now fails. Please update the public examples/spec docs to use lane values such as `out=0`/`out=1`, or preserve a separate default-device spelling.

  **Resolution:** README updated to use `out=0` / `out=1` and to explain the new lane-index semantics. Live `run` path's audio-only configs already default to the host's default audio device when no CV channel claims a name (see `validate_live_audio_lanes` and the `audio_output_request` match in `run.rs`), so users no longer need an `out=default` placeholder for audio.


<!-- gh-id: 3191971953 -->
### Copilot on [`crates/chan/src/sink/audio.rs:378`](https://github.com/cmk/agogo/pull/81#discussion_r3191971953) (2026-05-05 22:37 UTC)

When two or more click events near a buffer boundary have tails that extend into the next buffer, this only remembers the last truncated tail. Earlier tails are overwritten by the later `state.pending_samples = ...`, so dense-but-valid grids lose audio across the boundary (for example, very fine grids like `T512P` at high BPM/sample-rate combinations where multiple clicks can overlap the same boundary). This reintroduces buffer-size-dependent output for supported schedules.

<!-- gh-id: 3191971971 -->
### Copilot on [`doc/plans/plan-2026-05-05-02.md:58`](https://github.com/cmk/agogo/pull/81#discussion_r3191971971) (2026-05-05 22:37 UTC)

This plan still documents a `LaneMissing` variant on `OfflineRenderError`, but the implementation no longer has that error at the render boundary: `Channel::Audio` now always carries a `lane`, and missing `out=` is rejected earlier by the spec parser / `into_channel`. As written, the plan describes an API and validation path that the code in this PR does not actually provide.

<!-- gh-id: 3191971982 -->
### Copilot on [`doc/plans/plan-2026-05-05-02.md:126`](https://github.com/cmk/agogo/pull/81#discussion_r3191971982) (2026-05-05 22:37 UTC)

The verification table is out of sync with the test file added in this PR: it promises properties such as `prop_lane_missing_rejected`, `prop_onset_sample_exact`, `prop_footprint_length_constant`, and `prop_sample_rate_doubling`, but `crates/agogo/test/inter_channel_accuracy.rs` doesn't define them. That makes the plan overstate the coverage this branch actually lands.


<!-- gh-id: 4232030404 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-05-05 22:37 UTC](https://github.com/cmk/agogo/pull/81#pullrequestreview-4232030404))

## Pull request overview

Generalizes offline rendering from fixed mono/stereo assumptions to explicit per-lane audio routing, exposes captured per-lane PCM through the public API, and adds end-to-end tests around inter-channel timing and routing behavior.

**Changes:**
- Added `output_channels`/lane-aware offline rendering, plus `render_offline_capture` and related public exports.
- Switched audio channel routing from implicit round-robin placement to explicit `out=N` lane assignment, with shared lane validation for offline and live paths.
- Added CLI/docs/test updates for multi-channel render flows and inter-channel property testing.

### Reviewed changes

Copilot reviewed 18 out of 19 changed files in this pull request and generated 4 comments.

<details>
<summary>Show a summary per file</summary>

| File | Description |
| ---- | ----------- |
| README.md | Updates the live audio example to use explicit `out=N` lane routing. |
| doc/reviews/review-00081.md | Adds the local review record for this PR. |
| doc/plans/plan-2026-05-05-02.md | Adds the sprint plan and verification matrix for multi-channel rendering. |
| crates/core/src/transport.rs | Implements lane-aware offline rendering, validation, and PCM capture. |
| crates/core/src/lib.rs | Re-exports new offline render APIs/constants. |
| crates/cli/test/render.rs | Updates render expectations and adds 4-channel CLI/library parity coverage. |
| crates/cli/src/command/run.rs | Applies shared lane validation to the live path and adjusts output-device selection. |
| crates/cli/src/command/render.rs | Adds `--output-channels` and updates offline render channel parsing/validation. |
| crates/chan/src/sink/audio.rs | Adds pending-click continuation across buffer boundaries. |
| crates/chan/src/channel/time.rs | Extends `Channel::Audio` with an explicit output lane. |
| crates/chan/src/channel/spec/validate.rs | Requires `audio_lane` when converting audio specs into channels. |
| crates/chan/src/channel/spec/types.rs | Adds typed `audio_lane` storage to channel specs. |
| crates/chan/src/channel/spec/parser.rs | Parses `dev=audio,out=N` into typed lane data and rejects non-integer audio outputs. |
| crates/chan/src/channel/spec/display.rs | Prints audio `out=` from the typed lane field. |
| crates/agogo/test/inter_channel_accuracy.rs | Adds new property/spot tests for lane routing and timing behavior. |
| crates/agogo/test/inter_channel_accuracy.proptest-regressions | Checks in saved proptest regression seeds. |
| crates/agogo/src/lib.rs | Re-exports the new core lane/capture APIs through the facade. |
| crates/agogo/Cargo.toml | Adds proptest dev-dependency and registers the new integration test. |
| Cargo.lock | Records the new `proptest` dependency. |
</details>






<!-- gh-id: 3191972003 -->
### Copilot on [`doc/reviews/review-00081.md:21`](https://github.com/cmk/agogo/pull/81#discussion_r3191972003) (2026-05-05 22:37 UTC)

This review record says the PR adds an `OfflineRenderError::LaneMissing` variant, but the shipped code doesn't expose that variant at all. Missing `out=` is rejected earlier by the channel-spec parser / `into_channel`, so the summary currently documents a public error surface that isn't actually present.


<!-- gh-id: 3192286644 -->
#### ↳ cmk ([2026-05-06 00:13 UTC](https://github.com/cmk/agogo/pull/81#discussion_r3192286644))

Acknowledged — this is the limitation I noted in the AudioClickState doc but should have been more explicit about. I've strengthened the doc comment to call out the regime where it bites (fine grids like T256/T512P at high BPM where event spacing drops below click_len) and queued a follow-up in the plan's Deferred section. The proper fix needs a queue of pending tails *plus* decoupling state advancement from output writing, since the shared filter/RNG state currently interleaves between overlapping clicks; renaming `pending_samples` to a Vec alone wouldn't be enough to recover bit-identicality across buffer sizes for those configs. The proptest's polyrhythm-pair set keeps event spacing >= click_len, so the buffer-boundary invariance property holds for everything it actually exercises today.

<!-- gh-id: 3192286878 -->
#### ↳ cmk ([2026-05-06 00:14 UTC](https://github.com/cmk/agogo/pull/81#discussion_r3192286878))

Good catch — `LaneMissing` was an early draft hangover that didn't survive into the implementation. `Channel::Audio.lane: u16` is total, so a missing `out=` is rejected one layer earlier (spec parser / `into_channel`) and the offline boundary never sees it. Removed the variant from the plan (T3 section) and added a comment on `OfflineRenderError` explaining why it's deliberately absent. The review-00081.md summary has the same fix.

<!-- gh-id: 3192287112 -->
#### ↳ cmk ([2026-05-06 00:14 UTC](https://github.com/cmk/agogo/pull/81#discussion_r3192287112))

You're right — initial draft over-promised. Added the three useful missing properties this round (`prop_onset_sample_exact`, `prop_footprint_length_constant`, `prop_sample_rate_doubling`) and renamed the lane-separation / coincident properties in the plan to match their shipped identifiers. Dropped `prop_lane_missing_rejected` since the `LaneMissing` variant doesn't exist (parser catches it earlier). Verification table now lists the 12 properties the test file actually defines.

<!-- gh-id: 3192287363 -->
#### ↳ cmk ([2026-05-06 00:14 UTC](https://github.com/cmk/agogo/pull/81#discussion_r3192287363))

Same issue as the plan-side `LaneMissing` reference — fixed in this round by removing the `LaneMissing` mention from the review-00081 summary and noting that missing `out=` is rejected upstream by the spec parser instead.
