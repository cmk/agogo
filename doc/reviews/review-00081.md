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
  `UnsupportedChannelCount`, `LaneOutOfRange`, `LaneCollision`,
  `LaneMissing` surface with `Display` impls grounded in the
  user-facing `out=` field name (e.g. "channel #3: out=4 exceeds
  output_channels=2").
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
