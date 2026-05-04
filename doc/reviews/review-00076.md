# Review 00076

## Summary

### What Changed

- Added the plan for a public-demo CV pulse MVP.
- Taught channel specs to parse and lower `dev=cv,mode=pulse` into
  `Channel::Cv`.
- Added fixed-shape CV pulse rendering through the existing mono audio output
  path, including bipolar reset state across buffer boundaries.
- Extended offline render JSON with positive/negative audio peak diagnostics
  and added a hardware-free CV pulse render test.
- Updated README and roadmap docs to distinguish the CV pulse MVP from the
  later heterogeneous output/calibration work.

### Verification

- `cargo test -p agogo-chan cv --quiet`
- `cargo test -p agogo-core transport --quiet`
- `cargo test -p agogo-cli --test render --quiet`
- `cargo run -p agogo-cli --bin agogo -- render --source internal --bpm 120 --sr 48000 --duration-bars 1 --ch 'id=cv,dev=cv,mode=pulse,grid=t4,out=diag'`
- `scripts/check-pii.sh`
- `cargo test --workspace --quiet`
- `cargo clippy --all-targets --quiet -- -D warnings`

## Local review (2026-05-04)

**Branch:** plan/2026-05-04-04
**Commits:** 3 (origin/main..plan/2026-05-04-04)
**Reviewer:** Codex (`codex review --base origin/main`)

---

The offline renderer works, but the patch exposes and documents `dev=cv,mode=pulse` for `agogo run` without updating runtime sink selection, so the CV-only run path silently opens no output buffer and produces no signal.

Review comment:

- [P2] Route CV pulse channels to the audio output host — crates/chan/src/channel/spec/parser.rs:398-398
  When a user runs the newly accepted/documented CV-only spec (`dev=cv,mode=pulse,out=default`), `agogo run` still only treats `ChannelSpecRole::Audio` / `Channel::Audio` as requiring an audio output device. As a result `mix.has_audio` is false, cpal opens the input-only config with an empty `AudioIo::output`, and `render_cv_pulse_block` returns without writing any pulse. Include CV pulse channels in the output-device selection and channel mix so the advertised hardware-backed CV path actually opens an output stream.

Resolution: fixed by routing `ChannelSpecRole::Cv` and `Channel::Cv` through
the existing audio-output sink selection in `agogo run`, plus a feature-gated
CLI regression test for `dev=cv,mode=pulse`.

## Local review (2026-05-04)

**Branch:** plan/2026-05-04-04
**Commits:** 4 (origin/main..plan/2026-05-04-04)
**Reviewer:** Codex (`codex review --base origin/main`)

---

The new CV pulse renderer can drop pulses for accepted schedules where events are adjacent because the bipolar reset cancels the next event's impulse. This is a functional correctness issue in the newly added rendering path.

Review comment:

- [P2] Preserve pulse when reset overlaps next event — crates/chan/src/sink/audio.rs:291-295
  For CV channels with adjacent scheduled events (possible at very fine grids/high but validator-accepted tempos, e.g. one event per sample), the bipolar reset for event N is mixed into the same sample as the positive impulse for event N+1, so `-1.0 + 1.0` clamps to `0.0` and the second pulse disappears. This violates the advertised “every scheduled pulse writes the positive impulse at the intended sample” behavior; handle overlaps explicitly or reject configurations where bipolar pulses cannot fit.

Resolution: fixed by skipping a bipolar reset when it would land on another
scheduled pulse sample, preserving the positive impulse. Added a spot test and
extended `cv_impulse_sample_exact` to cover adjacent events.

## Local review (2026-05-04)

**Branch:** plan/2026-05-04-04
**Commits:** 5 (origin/main..plan/2026-05-04-04)
**Reviewer:** Codex (`codex review --base origin/main`)

---

The new CV pulse renderer handles adjacent events within one channel, but still allows cross-channel bipolar resets to cancel another channel's scheduled pulse on the shared mono output.

Review comment:

- [P2] Preserve pulses across CV channels — crates/chan/src/sink/audio.rs:297-301
  When two accepted CV pulse channels share the current mono output and one channel has an event one sample before another, this reset check only sees events from the current channel, so the first channel writes `-1.0` into the second channel's positive pulse sample and the final mix clamps to `0.0`. That makes a scheduled pulse disappear for multi-channel CV specs using the same output; the overlap handling needs to account for all CV pulse events in the mixed buffer or otherwise reject these schedules.

Resolution: fixed by making CV positive pulses full-scale writes and making
CV negative resets skip samples that already contain a full-scale CV pulse.
Added cross-channel spot tests for both render orders.

## Local review (2026-05-04)

**Branch:** plan/2026-05-04-04
**Commits:** 6 (origin/main..plan/2026-05-04-04)
**Reviewer:** Codex (`codex review --base origin/main`)

---

I did not find any discrete correctness issues in the diff. The added CV pulse parsing, rendering, runtime sink selection, and offline diagnostics are covered by tests and the workspace test suite passes.


## Local review (2026-05-04)

**Branch:** plan/2026-05-04-04
**Commits:** 6 (origin/main..plan/2026-05-04-04)
**Reviewer:** Codex (`codex review --base origin/main`)

---

I did not identify any actionable correctness issues in the changes relative to the base branch. The parser, runtime routing, CV pulse rendering, and offline diagnostics appear consistent with the intended MVP behavior.


<!-- gh-id: 3180160445 -->
### Copilot on [`crates/chan/src/channel/spec/parser.rs:413`](https://github.com/cmk/agogo/pull/76#discussion_r3180160445) (2026-05-04 08:10 UTC)

`mode` parsing now accepts `pulse`/`lfo`, but the role match does not handle `dev=midi` paired with those modes. Specs like `dev=midi,mode=pulse` (or `mode=lfo`) will hit the final `_ => unreachable!(...)` and panic instead of returning a user-facing `ChannelSpecError`. Add explicit error arms for these combinations (and any other newly-admitted (dev, mode) pairs) so invalid specs fail gracefully.

<!-- gh-id: 3180160491 -->
### Copilot on [`crates/chan/src/sink/audio.rs:314`](https://github.com/cmk/agogo/pull/76#discussion_r3180160491) (2026-05-04 08:10 UTC)

`render_cv_pulse_block` calls `event_at_sample()` inside the per-event loop, and `event_at_sample()` scans the full `events` slice each time. Since the scheduler can legitimately emit up to ~`frames` events in a buffer, this makes CV rendering O(n²) per buffer in the worst case and risks RT underruns. Consider leveraging the fact that `events` are emitted in ascending `sample_index` order (e.g., compare against the next event, or use binary search / a moving index) so overlap checks are O(1) or O(log n) per event.

<!-- gh-id: 3180160525 -->
### Copilot on [`crates/chan/src/channel/spec.rs:11`](https://github.com/cmk/agogo/pull/76#discussion_r3180160525) (2026-05-04 08:10 UTC)

The module docs list `mode` values as `clock|click|pulse`, but the parser also accepts `mode=lfo` (even though it currently errors for `dev=cv`). Update the documented mode value set (and/or note which modes are currently unimplemented) so the docs match the actual CLI surface and error messages.


<!-- gh-id: 4218355547 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-05-04 08:10 UTC](https://github.com/cmk/agogo/pull/76#pullrequestreview-4218355547))

## Pull request overview

This PR adds a “CV pulse” MVP intended for public demos by allowing `dev=cv,mode=pulse` channel specs to lower into `Channel::Cv` and render fixed-shape CV pulses through the existing mono audio output path, with offline diagnostics and tests to validate behavior.

**Changes:**
- Extend channel spec parsing/validation/display to support `dev=cv` (defaulting to `mode=pulse`) and lower into `Channel::Cv`.
- Add fixed-shape CV pulse rendering (incl. bipolar reset across buffer boundaries) via the mono `AudioIo::output` path, wired into the playhead.
- Extend offline render JSON with positive/negative peak diagnostics and add CLI/offline tests; update docs/roadmap/README to reflect the MVP slice.

### Reviewed changes

Copilot reviewed 19 out of 19 changed files in this pull request and generated 3 comments.

<details>
<summary>Show a summary per file</summary>

| File | Description |
| ---- | ----------- |
| doc/versions/version-0.4.md | Notes the earlier CV-pulse MVP slice landing ahead of heterogeneous output work. |
| doc/reviews/review-00076.md | Adds a review record capturing the iteration history and resolutions. |
| doc/plans/plan-2026-05-04-04.md | New plan doc describing scope, tasks, and verification for the CV pulse MVP. |
| doc/designs/cv-pulse.md | Updates design doc to reflect the MVP being pulled forward. |
| crates/host-cpal/src/lib.rs | Updates backend docs to mention the mono CV-pulse path alongside audio click. |
| crates/host-cpal/README.md | Updates backend README to describe the CV pulse MVP slice and mono output. |
| crates/core/src/transport.rs | Wires `Channel::Cv` into playhead dispatch, tracks per-channel CV pulse state, and adds offline peak diagnostics. |
| crates/cli/test/render.rs | Adds a hardware-free render test asserting CV pulse activity and peak diagnostics in JSON. |
| crates/cli/src/command/run.rs | Routes CV channels through audio-output selection/mix so `agogo run` opens an output stream for CV pulse. |
| crates/cli/src/command/render.rs | Adds `positive_peak_q15` / `negative_peak_q15` to render JSON output. |
| crates/chan/src/sink/audio.rs | Implements CV pulse renderer + state, plus unit/property tests; updates `AudioIo` docs to reflect mono output usage. |
| crates/chan/src/channel/spec/validate.rs | Lowers `ChannelSpecRole::Cv` into `Channel::Cv` and adds a lowering test. |
| crates/chan/src/channel/spec/types.rs | Introduces `ChannelSpecRole::Cv(CvRole)`. |
| crates/chan/src/channel/spec/parser.rs | Parses `dev=cv` + `mode=pulse`/`lfo` and defaults `dev=cv` to pulse; adds CV-specific parser tests. |
| crates/chan/src/channel/spec/display.rs | Updates display/roundtrip role generation to include CV pulse. |
| crates/chan/src/channel/spec.rs | Updates channel spec module docs for CV pulse MVP behavior. |
| crates/chan/src/channel/role.rs | Updates CV role docs to reflect that `Pulse` now renders via audio output. |
| crates/chan/src/channel.rs | Makes `CvRole` user-facing (while keeping `DinRole` doc-hidden) and updates comments. |
| README.md | Updates status + adds example commands for offline CV pulse render and runtime smoke path. |
</details>






<!-- gh-id: 3180206577 -->
#### ↳ cmk ([2026-05-04 08:18 UTC](https://github.com/cmk/agogo/pull/76#discussion_r3180206577))

Fixed by adding explicit dev=midi + mode=pulse/lfo error arms, plus a regression test covering both modes so these now return ChannelSpecError instead of reaching unreachable!.

<!-- gh-id: 3180206786 -->
#### ↳ cmk ([2026-05-04 08:18 UTC](https://github.com/cmk/agogo/pull/76#discussion_r3180206786))

Fixed by replacing the per-event linear scan with binary_search_by_key over the sorted ScheduledEvent slice. The overlap checks are now logarithmic rather than O(n) scans inside the loop.

<!-- gh-id: 3180206816 -->
#### ↳ cmk ([2026-05-04 08:18 UTC](https://github.com/cmk/agogo/pull/76#discussion_r3180206816))

Fixed by updating the channel-spec module docs to list lfo in the accepted mode tokens and explicitly note that dev=cv,mode=lfo is parsed but rejected until the LFO renderer lands.
