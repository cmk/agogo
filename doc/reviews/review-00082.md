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


<!-- gh-id: 3194243479 -->
### Copilot on [`crates/core/src/transport.rs:897`](https://github.com/cmk/agogo/pull/82#discussion_r3194243479) (2026-05-06 09:11 UTC)

`OfflinePcm::into_planar` will panic on `channels == 0` because of `idx % channels`. Since `OfflinePcm` is a public struct with public fields, it’s possible for downstream code (or future internal code) to construct an invalid instance and hit this unexpectedly. Consider defensively handling `self.channels == 0` (e.g., return an empty `Vec`, or `debug_assert!(self.channels > 0)` plus a clear panic) before the modulo/push loop.

<!-- gh-id: 3194243558 -->
### Copilot on [`doc/plans/plan-2026-05-06-01.md:14`](https://github.com/cmk/agogo/pull/82#discussion_r3194243558) (2026-05-06 09:11 UTC)

nit: This plan uses hard line-number references that will become stale once the PR is merged (e.g., `crates/core/src/transport.rs:963-971` here, plus `(lines 963-971)` in T1 and `crates/cli/src/command/run.rs:470` / `run.rs:275`). Consider switching these to symbol-based references (function names / snippets) or updating them to the post-change locations so readers don’t chase the wrong sites.

<!-- gh-id: 4234723527 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-05-06 09:11 UTC](https://github.com/cmk/agogo/pull/82#pullrequestreview-4234723527))

## Pull request overview

This PR updates the offline capture representation to match the renderer’s native interleaved PCM layout, adds `agogo render --wav-out FILE` for emitting multi-channel float WAV output, and audits the live `agogo run` path so its output channel count is configurable (instead of stereo-only).

**Changes:**
- Refactor `OfflinePcm` from planar lanes to `interleaved: Vec<f32>` + `channels`, with `lane()`, `frame()`, and `into_planar()` accessors; offline capture loop now memcpy-extends the renderer buffer.
- Add `--wav-out` to `agogo render`, writing 32-bit float multi-channel WAV via `hound` directly from interleaved samples.
- Add `--output-channels` to `agogo run`, plumb through shared validation and into the cpal backend; relax host-cpal’s previous stereo-only guard.

### Reviewed changes

Copilot reviewed 11 out of 12 changed files in this pull request and generated 2 comments.

<details>
<summary>Show a summary per file</summary>

| File | Description |
| ---- | ----------- |
| doc/reviews/review-00082.md | Local review record for PR #82 and its resolution notes. |
| doc/plans/plan-2026-05-06-01.md | Sprint plan describing the interleaved refactor, WAV output, and live-channel audit. |
| crates/core/src/transport.rs | Implements interleaved `OfflinePcm`, adds accessors, and switches capture to `extend_from_slice`. |
| crates/agogo/test/inter_channel_accuracy.rs | Updates proptests/spot-checks to the interleaved storage + new layout properties. |
| crates/cli/src/command/render.rs | Adds `--wav-out` and WAV writing logic; switches to capture path when enabled. |
| crates/cli/test/render.rs | Adds CLI WAV round-trip tests and updates aggregate derivation to use interleaved PCM. |
| crates/cli/src/parse.rs | Introduces shared `validate_output_channels` helper for CLI commands. |
| crates/cli/src/command/run.rs | Adds `--output-channels` flag and plumbs it through validation and cpal config. |
| crates/host-cpal/src/cpal.rs | Removes stereo-only restriction; defers channel support checks to device capability selection. |
| crates/cli/Cargo.toml | Adds `hound` dependency for WAV output support. |
| Cargo.toml | Adds workspace-level `hound` version pin. |
| Cargo.lock | Locks `hound` and updates dependent crate entries. |
</details>






<!-- gh-id: 3194295719 -->
#### ↳ cmk ([2026-05-06 09:18 UTC](https://github.com/cmk/agogo/pull/82#discussion_r3194295719))

Hardened — added an early-return when channels == 0 so the modulo path can't panic on a hand-constructed instance. The renderer's UnsupportedChannelCount validator (1..=MAX_OUTPUT_CHANNELS) still rejects this shape at the boundary, but OfflinePcm has public fields so the defense is correct. New spot_into_planar_zero_channels_no_panic test pins the behaviour.

<!-- gh-id: 3194296319 -->
#### ↳ cmk ([2026-05-06 09:18 UTC](https://github.com/cmk/agogo/pull/82#discussion_r3194296319))

Fixed — replaced the absolute line numbers with symbol-anchored references (function names, constant names, and field doc-comment anchors) so the plan stays readable as the code drifts. Verified with a final grep sweep that no :NNN line refs remain.
