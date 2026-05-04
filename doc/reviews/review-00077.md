# Review 00077

## Summary

Add deterministic property coverage for the audio/CV render path and make the
fixed-point Conn conversion that property exposed total at numeric extremes.

- Generate valid audio/CV `ChannelSpec` configs, lower them twice, render through
  `render_offline`, and compare complete `OfflineRenderReport` values.
- Generate runtime `ChannelCommon` configs for audio click / CV pulse roles and
  compare complete PCM traces from two `Playhead` runs sample-wise.
- Change fixed-ladder Conn `inner` conversion to saturating multiplication and
  add full-domain saturation / monotonicity properties.

Verification:

- `cargo test -p agogo-chan conn::fixed --quiet`
- `cargo test -p agogo-core transport::tests::playhead_generated_audio_cv_outputs_match_samplewise --quiet`
- `cargo test -p agogo-core transport --quiet`
- `cargo test --workspace --quiet`
- `cargo clippy --all-targets --quiet -- -D warnings`
- `scripts/check-pii.sh`
