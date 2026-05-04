# Review 00077

## Summary

Add deterministic property coverage for the audio/CV render path and make the
runtime microsecond-to-sample conversion total at numeric extremes.

- Generate valid audio/CV `ChannelSpec` configs, lower them twice, render through
  `render_offline`, and compare complete `OfflineRenderReport` values.
- Generate runtime `ChannelCommon` configs for audio click / CV pulse roles and
  compare complete PCM traces from two `Playhead` runs sample-wise.
- Clamp extreme runtime `Micro` offsets at the `micro_to_samples` boundary
  before using the exact fixed-ladder Conn.

Verification:

- `cargo test -p agogo-chan conn::fixed --quiet`
- `cargo test -p agogo-core transport::tests::playhead_generated_audio_cv_outputs_match_samplewise --quiet`
- `cargo test -p agogo-core transport --quiet`
- `cargo test --workspace --quiet`
- `cargo clippy --all-targets --quiet -- -D warnings`
- `scripts/check-pii.sh`

## Local review (2026-05-04)

**Branch:** plan/2026-05-04-05
**Commits:** 5 (origin/main..plan/2026-05-04-05)
**Reviewer:** Codex (`codex review --base origin/main`)

---

The patch makes the fixed-point Conn total by saturating, but that violates the Conn round-trip/embedding laws over the declared domain. This is a foundational correctness issue despite tests being adjusted around it.

Review comment:

- [P1] Restore fixed Conn round trips at saturated extremes — `crates/chan/src/conn/fixed.rs:124`
  When a coarse value exceeds `i64::MAX / PREC` (for example `FD12FD06.inner(Micro(i64::MAX))`), this saturates many distinct coarse inputs to the same fine value, so `ceil(inner(c))` no longer returns `c` (`Micro(i64::MAX)` round-trips back to about `9_223_372_036_855`). That breaks the published fixed-ladder `Conn` embedding/round-trip contract and any code relying on these connections being lawful over their declared `i64` types; handle the extreme runtime offset case at the boundary instead of changing `inner` to a non-injective saturating map.

Resolution: fixed. Restored exact fixed-ladder `inner` conversion and moved
extreme-value handling to `micro_to_samples`, where `Micro` values are clamped
to the largest range that can be embedded into `Pico(i64)` before calling
`FD12FD06.inner`. Added `micro_to_samples_clamps_to_pico_safe_range`.

## Local review (2026-05-04)

**Branch:** plan/2026-05-04-05
**Commits:** 6 (origin/main..plan/2026-05-04-05)
**Reviewer:** Codex (`codex review --base origin/main`)

---

No discrete correctness issues were found in the diff. The production change makes the microsecond-to-sample conversion total at fixed-point extremes, and the added tests pass for the targeted transport suite.


<!-- gh-id: 3180838876 -->
### Copilot on [`crates/core/src/transport.rs:811`](https://github.com/cmk/agogo/pull/77#discussion_r3180838876) (2026-05-04 10:21 UTC)

`audio_cv_channel` can also construct `Channel::Midi` (it matches `ChannelSpecRole::Midi`), which makes the helper name misleading and could hide accidental expansion of the strategy to MIDI roles (the test would still pass by comparing silence). Consider either renaming this helper to reflect all supported roles, or narrowing the parameter type / match arms to only Audio+Cv and making the MIDI case unreachable.

<!-- gh-id: 4219164608 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-05-04 10:21 UTC](https://github.com/cmk/agogo/pull/77#pullrequestreview-4219164608))

## Pull request overview

Adds deterministic, property-based coverage for the audio/CV offline render + runtime playhead paths, and makes microsecond-to-sample conversion total at numeric extremes by clamping before fixed-point embedding.

**Changes:**
- Added proptest strategies + identity properties that compare full offline render reports and sample-wise PCM traces for generated audio/CV configurations.
- Updated `micro_to_samples` to clamp `Micro` inputs to the Pico-embeddable range before applying `FD12FD06.inner`, plus a new regression test.
- Added accompanying plan/review documentation for the sprint work and resolution.

### Reviewed changes

Copilot reviewed 4 out of 4 changed files in this pull request and generated 3 comments.

| File | Description |
| ---- | ----------- |
| doc/reviews/review-00077.md | Records local review notes and resolution for the fixed-point extreme handling. |
| doc/plans/plan-2026-05-04-05.md | Sprint plan describing the new audio/CV identity properties and the micro clamp approach. |
| crates/core/src/transport.rs | Adds generated audio/CV offline render identity and playhead sample-wise identity proptests + helpers/strategies. |
| crates/chan/src/channel/time.rs | Clamps extreme `Micro` offsets inside `micro_to_samples` to avoid overflow in `FD12FD06.inner`, and adds a test. |





<!-- gh-id: 3180838910 -->
### Copilot on [`crates/core/src/transport.rs:844`](https://github.com/cmk/agogo/pull/77#discussion_r3180838910) (2026-05-04 10:21 UTC)

`render_audio_trace_48k` allocates a fresh `Vec<f32>` for `output` on every buffer iteration. In a proptest that runs many cases, this repeated allocation can noticeably slow the suite. Consider allocating `output` once outside the loop and reusing it (e.g., clearing/filling with 0.0 each iteration) to reduce per-case overhead.

<!-- gh-id: 3180838932 -->
### Copilot on [`crates/chan/src/channel/time.rs:167`](https://github.com/cmk/agogo/pull/77#discussion_r3180838932) (2026-05-04 10:21 UTC)

`micro_to_samples` now clamps `Micro` inputs to the Pico-embeddable range before converting. Since this is a behavioral guarantee (it changes how extreme offsets are handled), the doc comment should mention the clamping/saturation semantics (and why it exists) so callers don’t assume full-range `i64` microsecond offsets are preserved.

<!-- gh-id: 3185153583 -->
#### ↳ cmk ([2026-05-04 23:36 UTC](https://github.com/cmk/agogo/pull/77#discussion_r3185153583))

Fixed — replaced the `ChannelSpecRole`-typed helper with an `AudioCvRole` test-only enum, so the sample-wise runtime property can only construct audio-click or CV-pulse channels.

<!-- gh-id: 3185153606 -->
#### ↳ cmk ([2026-05-04 23:36 UTC](https://github.com/cmk/agogo/pull/77#discussion_r3185153606))

Fixed — `render_audio_trace_48k` now allocates the output buffer once and reuses it across buffer epochs, filling it with silence before each callback.

<!-- gh-id: 3185153734 -->
#### ↳ cmk ([2026-05-04 23:36 UTC](https://github.com/cmk/agogo/pull/77#discussion_r3185153734))

Fixed — expanded `micro_to_samples` docs to state that extreme `Micro` values are clamped to the Pico-embeddable range before the exact Conn call, preserving total runtime scheduling without weakening Conn laws.
