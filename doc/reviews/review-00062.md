# PR #62 - Audio metronome test feature

## Summary

Adds a minimal generated-tone audio metronome path for user testing:
`dev=audio,mode=click` now lowers to an audio channel and writes fixed
PCM clicks into the cpal output buffer. The CLI intentionally does not
surface sound-shaping controls; tone shape, duration, amplitude, and
accent behavior are fixed in core.

The old MIDI click path remains unchanged. MIDI ports are now opened
only when a MIDI channel exists, so an audio-only metronome spec does
not require MIDI hardware. `agogo run` uses output-only cpal for audio
click channels so `--source internal` can produce audible clicks without
an audio input device.

Notable constraints:

- `dev=audio` supports only `mode=click` in this test-feature slice.
- MIDI click keys (`note`, `vel`, `mch`, `accent-*`) are rejected on
  audio specs.
- `--source external` plus audio output is deferred until duplex
  input+output callback wiring exists.

Verification:

- `cargo fmt --all -- --check`
- `cargo fmt --manifest-path crates/host-cpal/Cargo.toml -- --check`
- `scripts/check-pii.sh`
- `scripts/check-floats.sh`
- `scripts/check-layers.sh`
- `cargo test --workspace`
- `cargo test -p agogo-cli --features run`
- `cargo test --manifest-path crates/host-cpal/Cargo.toml`
- `cargo clippy --all-targets -- -D warnings`
- `cargo clippy -p agogo-cli --features run --all-targets -- -D warnings`
- `cargo clippy --manifest-path crates/host-cpal/Cargo.toml --all-targets -- -D warnings`

## Local review (2026-05-02)

**Branch:** plan-2026-05-02-06
**Commits:** 3 (origin/main..plan-2026-05-02-06 before this review note)
**Reviewer:** Codex in-session local-only review

---

The canonical `scripts/local_review.sh` transition could not run in
this environment: it invokes `codex review --base origin/main`, and the
sandbox rejected that external review call because it would disclose
private repository context to the Codex review service. I did not try to
bypass that restriction.

Local-only review covered the target-selection path in `agogo run`, the
cpal output-only callback path, parser cross-key validation, and the
stateless audio renderer. No must-fix issues were found.

Residual risk: the generated click renderer is intentionally stateless,
so a click that starts near the end of one callback buffer is truncated
instead of continuing into the next buffer. This is documented in the
plan review section and is acceptable for the current test feature; WAV
playback or productized sound shaping should revisit it.

<!-- gh-id: 3176500187 -->
### Copilot on [`crates/cli/src/run.rs:413`](https://github.com/cmk/agogo/pull/62#discussion_r3176500187) (2026-05-02 10:26 UTC)

`Config::input_device` is set to `Some(args.audio_in.clone())` even when `args.audio_in == "default"`. To match `Config`'s documented semantics ("None selects the host's default input"), consider setting `input_device` to `None` in the default-device case (and `Some(name)` only when a specific device name was requested).

<!-- gh-id: 3176500196 -->
### Copilot on [`crates/cli/src/run.rs:148`](https://github.com/cmk/agogo/pull/62#discussion_r3176500196) (2026-05-02 10:26 UTC)

`midi_port_request` is derived from the first MIDI spec only. If multiple `dev=midi` channels specify different `out=` values, this will silently ignore all but the first, which can misroute output. Consider collecting the set of `out` values across all MIDI specs and returning an explicit error if there's more than one distinct value (until multi-port routing is implemented).

<!-- gh-id: 3176500201 -->
### Copilot on [`crates/cli/src/run.rs:152`](https://github.com/cmk/agogo/pull/62#discussion_r3176500201) (2026-05-02 10:26 UTC)

`audio_output_request` is derived from the first audio spec only. If multiple `dev=audio` channels specify different `out=` values, the CLI will silently pick one. Consider validating that all audio specs agree on `out=` (or erroring with a clear message) since the current host wiring supports only a single output device per run.

<!-- gh-id: 3176500204 -->
### Copilot on [`crates/cli/src/run.rs:391`](https://github.com/cmk/agogo/pull/62#discussion_r3176500204) (2026-05-02 10:26 UTC)

When `request == "default"`, `Config::output_device` should likely be `None` to match `Config`'s documented contract ("None selects the host's default output"). Passing `Some("default")` makes the config ambiguous for future `AudioHost` implementations that may rely on this field.

<!-- gh-id: 4214833086 -->
### copilot-pull-request-reviewer[bot] - COMMENTED ([2026-05-02 10:26 UTC](https://github.com/cmk/agogo/pull/62#pullrequestreview-4214833086))

## Pull request overview

Adds an audio metronome "test feature" path (`dev=audio,mode=click`) that renders fixed PCM clicks into a cpal output buffer, while keeping the existing MIDI click path and avoiding MIDI/device requirements for audio-only runs.

**Changes:**
- Introduces `Channel::Audio` / `AudioRole::Click` plus a core generated-click renderer (`render_audio_click_block`).
- Extends `ChannelSpec` parsing/printing/lowering to support `dev=audio,mode=click` and reject MIDI-only click keys on audio specs.
- Adds output-only stream support to `host-cpal` and updates `agogo run` to open MIDI and/or audio devices conditionally based on channel mix.

### Reviewed changes

Copilot reviewed 19 out of 19 changed files in this pull request and generated 4 comments.

<details>
<summary>Show a summary per file</summary>

| File | Description |
| ---- | ----------- |
| scripts/check-floats.sh | Updates float allowlist notes for audio sink output-boundary writes. |
| doc/reviews/review-00062.md | Adds PR review record for the feature. |
| doc/plans/plan-2026-05-02-06.md | Adds plan doc for the audio metronome test feature. |
| crates/host-cpal/src/lib.rs | Updates crate docs to reflect output-only support. |
| crates/host-cpal/src/cpal.rs | Implements output-only stream creation + output device enumeration. |
| crates/host-cpal/README.md | Documents output-only support and scope constraints. |
| crates/core/src/sink/audio.rs | Adds `NoOutputDevice` error + generated audio-click renderer and tests. |
| crates/core/src/control/transport.rs | Clears output buffer and dispatches audio channels to audio renderer; adds/reset counters + tests. |
| crates/core/src/channel/time.rs | Adds `Channel::Audio` variant and common accessors + constructibility test. |
| crates/core/src/channel/spec/validate.rs | Lowers `ChannelSpecRole` to runtime `Channel::{Midi,Audio}`. |
| crates/core/src/channel/spec/types.rs | Introduces `ChannelSpecRole` and updates `ChannelSpec` role storage. |
| crates/core/src/channel/spec/parser.rs | Parses `dev=audio,mode=click` and enforces cross-key constraints. |
| crates/core/src/channel/spec/error.rs | Removes the `AudioDeferred` error variant. |
| crates/core/src/channel/spec/display.rs | Emits `dev=audio` + `mode=click` for audio specs; updates proptests. |
| crates/core/src/channel/spec.rs | Updates module docs and re-exports `ChannelSpecRole`. |
| crates/core/src/channel/role.rs | Introduces `AudioRole`. |
| crates/core/src/channel.rs | Re-exports `AudioRole` from the channel module. |
| crates/cli/src/run.rs | Detects channel mix to open MIDI conditionally and use cpal output-only for audio clicks. |
| AGENTS.md | Updates float-allowlist narrative to include audio-click output writes. |

</details>

<!-- gh-id: 3176503606 -->
#### Reply from cmk ([2026-05-02 10:31 UTC](https://github.com/cmk/agogo/pull/62#discussion_r3176503606))

Fixed. `Config::input_device` now goes through a shared helper that maps `"default"` to `None` and keeps `Some(name)` only for explicit device names.

<!-- gh-id: 3176503675 -->
#### Reply from cmk ([2026-05-02 10:31 UTC](https://github.com/cmk/agogo/pull/62#discussion_r3176503675))

Fixed. MIDI channel specs now pass through `single_target_output_request`, which scans all MIDI specs and errors if they request more than one distinct `out=` value until multi-port routing exists.

<!-- gh-id: 3176503756 -->
#### Reply from cmk ([2026-05-02 10:31 UTC](https://github.com/cmk/agogo/pull/62#discussion_r3176503756))

Fixed. Audio channel specs use the same shared-output validation path, so multiple `dev=audio` specs must agree on `out=` while this runner supports one output device.

<!-- gh-id: 3176503847 -->
#### Reply from cmk ([2026-05-02 10:31 UTC](https://github.com/cmk/agogo/pull/62#discussion_r3176503847))

Fixed. `Config::output_device` now also maps `"default"` to `None`, preserving `Some(name)` only for an explicit requested output device.
