# PR #68 — Support stereo-only audio click outputs

## Summary

Fixes `dev=audio,mode=click` startup on output devices that support the
requested sample rate and f32 samples but do not expose a mono physical
stream configuration.

`host-cpal` now treats `Config::output_channels = 1` as the logical
mono callback contract, prefers a physical mono cpal output stream when
available, and otherwise opens an f32 output stream with the smallest
supported physical channel count at the requested rate. The backend
renders the core mono `AudioIo` callback into a preallocated scratch
buffer and fans that signal out to each physical output channel inside
the cpal callback.

The core audio-click renderer and `AudioIo` surface remain mono for this
test feature; this change is a backend adaptation for stereo-only
hardware, not a general multi-channel output API.

Validation:

- `cargo test --manifest-path crates/host-cpal/Cargo.toml`
- `cargo test -p agogo-cli --features run`
- `cargo clippy --manifest-path crates/host-cpal/Cargo.toml --all-targets -- -D warnings`
- `cargo clippy -p agogo-cli --features run --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo clippy --all-targets -- -D warnings`

## Local review (2026-05-03)

**Branch:** fix-audio-click-stereo-output
**Commits:** 1 (origin/main..fix-audio-click-stereo-output)
**Reviewer:** Codex (`codex review --base origin/main`)

---

No actionable correctness issues were found in the diff. The channel-selection and mono fan-out changes are covered by focused tests, and the host-cpal tests and clippy gate pass locally.

<!-- gh-id: 4216849612 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-05-03 21:13 UTC](https://github.com/cmk/agogo/pull/68#pullrequestreview-4216849612))

Copilot reviewed 4 out of 4 changed files in this pull request and generated 2 comments.

<!-- gh-id: 3178780997 -->
### Copilot on [`crates/host-cpal/src/cpal.rs:219`](https://github.com/cmk/agogo/pull/68#discussion_r3178780997) (2026-05-03 21:13 UTC)

The output callback now sizes its scratch buffer from `cfg.buffer_frames`, but `Config::buffer_frames` is only a target and back-ends may round it to a different size. If cpal delivers a larger buffer than requested, `writable_frames` truncates the core render to the scratch length, leaves the tail of the device buffer silent, and still advances `next_start` by the full callback length. That will drop audio/click events and skew timing on hosts that round the buffer size up.

<!-- gh-id: 3178781009 -->
### Copilot on [`doc/reviews/review-00068.md:38`](https://github.com/cmk/agogo/pull/68#discussion_r3178781009) (2026-05-03 21:13 UTC)

This review note is no longer accurate: the diff does introduce a correctness issue in the output callback path when the host delivers a larger buffer than `buffer_frames`. Keeping the review record saying there are no actionable correctness issues will mislead future readers about the state of this PR.

## Review round response (2026-05-03)

- `crates/host-cpal/src/cpal.rs`: fixed the rounded-up output-buffer issue by rendering the physical callback in scratch-sized mono chunks. Each chunk receives its own `AudioIo` with the correct `buffer_start_sample`, fans out only to the matching interleaved frame range, and `next_start` advances by the full physical frame count.
- `crates/host-cpal/src/cpal.rs`: added `oversized_output_callback_is_rendered_in_scratch_chunks` to cover callbacks larger than the requested `buffer_frames`.
- `doc/reviews/review-00068.md`: mirrored the GitHub comments and this response so the local review note remains historical rather than the final state.

Validation:

- `cargo test --manifest-path crates/host-cpal/Cargo.toml`
- `cargo test -p agogo-cli --features run`
- `cargo clippy --manifest-path crates/host-cpal/Cargo.toml --all-targets -- -D warnings`
- `cargo clippy -p agogo-cli --features run --all-targets -- -D warnings`

<!-- gh-id: 3178789603 -->
#### ↳ cmk ([2026-05-03 21:21 UTC](https://github.com/cmk/agogo/pull/68#discussion_r3178789603))

Fixed in the follow-up patch. The output callback now processes the physical buffer in scratch-sized mono chunks instead of truncating to `cfg.buffer_frames`: each chunk gets its own `AudioIo` with the correct `buffer_start_sample`, is fanned out to the corresponding interleaved frame range, and `next_start` advances by the full physical callback frame count. Added `oversized_output_callback_is_rendered_in_scratch_chunks` to cover the rounded-up-buffer case.

<!-- gh-id: 3178789675 -->
#### ↳ cmk ([2026-05-03 21:21 UTC](https://github.com/cmk/agogo/pull/68#discussion_r3178789675))

Fixed by mirroring the GitHub review comments into `review-00068.md` and adding a round response that supersedes the earlier local-review note. The original local review remains as history, but the record now names the discovered issue and the corrective patch.

<!-- gh-id: 4216862692 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-05-03 21:28 UTC](https://github.com/cmk/agogo/pull/68#pullrequestreview-4216862692))

Copilot reviewed 4 out of 4 changed files in this pull request and generated 1 comment.

<!-- gh-id: 3178797671 -->
### Copilot on [`crates/host-cpal/src/cpal.rs:353`](https://github.com/cmk/agogo/pull/68#discussion_r3178797671) (2026-05-03 21:28 UTC)

The new fallback-selection logic is only tested with a single non-mono candidate. There is no test covering the documented behavior of choosing the smallest supported physical channel count when several f32 configs are available at the requested rate, so a regression to "pick the first supported config" would still pass while breaking multi-channel-only devices.

## Review round response (2026-05-03, rebase)

- Rebased `fix-audio-click-stereo-output` onto `origin/main` at `d0190a1`.
- `crates/host-cpal/src/cpal.rs`: added `output_channel_selection_uses_smallest_f32_fallback`, which includes 8-channel, 6-channel, and 2-channel f32 candidates plus a mono non-f32 distractor. The test pins the documented "smallest supported f32 physical channel count" fallback and would fail if selection regressed to first-match behavior.

Validation:

- `cargo test --manifest-path crates/host-cpal/Cargo.toml`
- `cargo clippy --manifest-path crates/host-cpal/Cargo.toml --all-targets -- -D warnings`

<!-- gh-id: 3178802425 -->
#### ↳ cmk ([2026-05-03 21:35 UTC](https://github.com/cmk/agogo/pull/68#discussion_r3178802425))

Fixed after rebasing onto latest `origin/main`. Added `output_channel_selection_uses_smallest_f32_fallback`, which covers multiple f32 non-mono candidates (8, 6, and 2 channels) plus a mono non-f32 distractor. The test now pins the documented smallest-supported-f32 fallback and would fail if selection regressed to first-match behavior.
