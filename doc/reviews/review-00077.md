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
