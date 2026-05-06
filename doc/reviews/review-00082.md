# PR #82 — Interleaved OfflinePcm, WAV render, live multi-channel audit

## Summary

PR #81 shipped `OfflinePcm` with planar storage (`lanes: Vec<Vec<f32>>`).
This sprint flips to interleaved storage so the capture loop matches
the renderer's native PCM ABI layout, lands the deferred
`agogo render --wav-out FILE` feature on top of it, and audits the
live `agogo run` path so its channel count is configurable instead of
hardcoded to stereo.

### What changed

- **`OfflinePcm`** carries `interleaved: Vec<f32>` (length `frames *
  channels`) plus `channels: u16`, with three accessors:
  - `lane(i) -> impl Iterator<Item = f32> + '_` — strided per-channel
    iteration; out-of-range `i` returns an empty iterator (the
    renderer's lane validator rejects out-of-range lanes upstream).
  - `frame(t) -> &[f32]` — borrow the `channels`-wide slice for one
    frame.
  - `into_planar() -> Vec<Vec<f32>>` — owned per-lane Vecs for tests
    that index per-lane repeatedly.
- **Capture loop** in `render_offline_with_rate` does one
  `extend_from_slice(&output)` per buffer instead of `frames *
  channels` per-sample `Vec::push` calls — a single memcpy in the
  renderer's native layout. The interleaved Vec is pre-allocated
  with `total_frames * channels` capacity. Aggregate-stats
  computation is unchanged (it already iterated the interleaved
  buffer).
- **`crates/agogo/test/inter_channel_accuracy.rs`** test sites
  rebind to `into_planar()` once per render so the existing
  per-lane indexing patterns work unchanged.
  `prop_buffer_boundary_invariance` switches to comparing the
  `interleaved` Vec directly (one allocation per render instead of
  `output_channels`). Three new properties cover the storage
  shape: `prop_interleaved_layout`, `prop_into_planar_round_trip`,
  `prop_lane_iterator_length`. `spot_pcm_field_layout` pins the
  interleaving order with a known-content render.
- **`agogo render --wav-out FILE`** writes a 32-bit float
  multi-channel WAV via `hound::WavWriter`. When set, the handler
  switches from `render_offline` to `render_offline_capture` and
  streams `pcm.interleaved` directly into the writer — no
  transposition, the renderer's native layout matches what hound
  wants. Float-PCM is lossless against the renderer's f32 output,
  so no quantisation policy is needed (separate plan for i16/i24
  formats). When `--wav-out` is omitted, behaviour is byte-identical
  to existing JSON-only callers.
- **`agogo run --output-channels CHANNELS`** opens the cpal stream
  with the requested channel count instead of always 2. Validated
  against the same `MAX_OUTPUT_CHANNELS = 16` cap as the offline
  path via the new shared `validate_output_channels` helper in
  `crates/cli/src/parse.rs`. `validate_live_audio_lanes` takes the
  parsed value instead of a deleted `LIVE_OUTPUT_CHANNELS`
  constant, so live and offline can't drift on the no-mix-bus
  rule. Fallback `2` keeps existing live invocations working
  without the flag.

### Verification

- `cargo test --workspace` — green, 22/22 inter-channel
  proptests + 7/7 CLI render tests + 51/51 CLI `--features run`
  unit tests + the rest of the workspace.
- `cargo clippy --all-targets -- -D warnings` — clean.
- `scripts/check-floats.sh` / `check-pii.sh` / `check-layers.sh` /
  `check-connections.sh` / `check-boundary-panics.sh` — clean. The
  WAV writer borrows samples through the iterator (no stored f32
  added to a non-allowlisted file).
- Test-the-test gates per AGENTS.md:
  - **T1**: replaced `extend_from_slice` with a no-op; proptests
    that assert on PCM content failed (including the three new
    layout properties); restored.
  - **T2**: truncated the WAV write to half-length; round-trip
    test failed with sample-count mismatch; restored.
  - **T3**: hardcoded `validate_audio_lanes(channels, 2)` in the
    live validator; `run_accepts_audio_lane_15_when_output_channels_16`
    failed; restored.

### Why this shape

The user asked whether the planar capture loop's per-sample push
pattern was efficient enough to keep, given the WAV-render feature
was already on the deferred list and `hound` consumes interleaved
samples natively. Bundling the storage refactor with WAV (and the
adjacent live channel-count audit) means PCM ABI work crosses the
public-API boundary exactly once instead of once per follow-on
sprint.

## Local review (2026-05-06)

**Branch:** plan/2026-05-06-01
**Commits:** 5 (origin/main..plan/2026-05-06-01)
**Reviewer:** Codex (`codex review --base origin/main`)

---

The offline WAV/interleaved capture path tests pass, but the live multi-channel CLI path advertises and accepts channel counts that the existing cpal backend still rejects. This breaks the new live `--output-channels` behavior for non-stereo output.

Review comment:

- [P2] Honor live output channel counts before advertising the flag — `crates/cli/src/command/run.rs:481-488`
  When `agogo run` is invoked with audio output and `--output-channels` set to anything other than 2, this value is forwarded into `CpalHost::run`, but `crates/host-cpal/src/cpal.rs` still rejects output configs unless `output_channels == 2` before it queries supported formats. As a result the new documented 1..=16 live-channel flag only works for the old default stereo case; either keep the run-path validation at 2 until the backend is updated, or teach `host-cpal` to open the requested channel count.

  **Addressed in branch.** `crates/host-cpal/src/cpal.rs:188` guard relaxed from `output_channels != 2` to `output_channels == 0`; the device-capability check at `validate_output_channels` / `select_f32_channels_at_rate` is the source of truth for what the device actually supports. host-cpal's 19 unit tests still pass; the existing `output_channel_selection_*` tests cover both the accept-stereo and reject-out-of-range cases. Folded into the T3 commit via `git commit --fixup`.

