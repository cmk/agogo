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
