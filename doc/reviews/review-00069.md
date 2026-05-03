# PR #69 - Declare MIDI timing capabilities

## Summary

Adds explicit MIDI output timing capability reports so current output paths
state whether `at_sample` is honored or metadata-only. The core MIDI sink
module now exposes `MidiTimingCapability`, scheduling/latency/`at_sample`
classes, and a `MidiTimingCapabilities` trait with no blanket default.

Current sinks report immediate best-effort timing: `TestSink`,
`DiagnosticSink`, `RtProducer`, and `MidirSink` preserve intended sample
indices as metadata but do not claim native timestamped scheduling. The new
`DiagnosticSink` records intended samples, payload bytes, drain order, optional
observed sample positions, and integer delay summaries outside the realtime
callback path.

The SPSC and midir tests pin the truthful capability reports, `RtProducer`
FIFO preservation of `at_sample`, and diagnostic delay summaries. v0.2/v0.3
docs now mark this as the first timestamped-output step: truthfulness and
diagnostics now, native CoreMIDI/JACK-style scheduling later.

Validation:

- `cargo fmt --all -- --check`
- `scripts/check-pii.sh`
- `scripts/check-floats.sh`
- `scripts/check-layers.sh`
- `scripts/check-connections.sh`
- `scripts/check-boundary-panics.sh`
- `cargo test --workspace --quiet`
- `cargo clippy --all-targets --quiet -- -D warnings`
- `cargo test --quiet` in `crates/host-cpal`
- `cargo test --quiet` in `crates/host-midi`

## Local review (2026-05-03)

**Branch:** plan/2026-05-03-04
**Commits:** 3 (origin/main..plan/2026-05-03-04)
**Reviewer:** Codex (`codex review --base origin/main`)

---

The capability API does not enforce or preserve timing reports through the main sink abstraction, and the host-midi verification does not actually exercise the backend implementation it claims to verify. These issues undermine the central behavior added by the patch.

Full review comments:

- [P2] Require timing reports on every MidiSink — crates/core/src/sink/midi.rs:114-115
  Because `MidiTimingCapabilities` is independent of `MidiSink`, the main drain API can still accept `Arc<dyn MidiSink + Send + Sync>` and the CLI can coerce `MidirSink` into that type, at which point the selected output sink no longer exposes `timing_capability()`. In that scenario a new backend can implement only `MidiSink` and still enter production output without any explicit timing claim, which defeats this PR's purpose of distinguishing metadata-only versus timestamped output; consider making `MidiSink` require the timing-capability trait or carrying a combined trait object through the drain path.

- [P2] Exercise MidirSink capability reporting in the test — crates/host-midi/src/midir.rs:136-136
  This test constructs the expected best-effort value directly instead of querying `MidirSink`'s implementation, so changing `MidirSink::timing_capability()` to return the wrong backend name or claim timestamped support would still pass. Since the verification table claims this pins `MidirSink`'s report, route the assertion through the backend's advertised capability, such as an associated capability used by the impl, rather than the generic constructor.

## Local review resolution (2026-05-03)

- Fixed: `MidiSink` now inherits `MidiTimingCapabilities`, so any production
  sink that enters the drain path must make an explicit timing claim.
- Fixed: `MidirSink` exposes `TIMING_CAPABILITY`, the trait impl returns that
  value, and the test asserts the backend-advertised capability rather than a
  separately constructed expected value.

<!-- gh-id: 4216949426 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-05-03 22:54 UTC](https://github.com/cmk/agogo/pull/69#pullrequestreview-4216949426))

## Pull request overview

This PR makes MIDI output timing “truth” explicit by adding capability reporting to the core `MidiSink` abstraction and updating current sinks/backends to declare that `at_sample` is preserved as metadata but not physically honored (best-effort/immediate dispatch). It also adds a diagnostic sink to record intended vs observed drain timing outside the realtime callback path, and updates docs/tests to reflect and pin these claims.

**Changes:**
- Introduces `MidiTimingCapability` (+ related enums) and requires all `MidiSink` implementations to report timing capabilities.
- Updates `MidirSink`, `RtProducer`, and `TestSink` to declare best-effort/immediate timing; adds `DiagnosticSink` + delay summaries and tests.
- Updates v0.2/v0.3 notes and adds plan/review docs describing the new timing-truth contract and diagnostics.

### Reviewed changes

Copilot reviewed 10 out of 10 changed files in this pull request and generated 2 comments.

<details>
<summary>Show a summary per file</summary>

| File | Description |
| ---- | ----------- |
| doc/versions/version-0.3.md | Notes that clock-domain work must rely on backend timing capability reports before assuming `at_sample` is honored. |
| doc/versions/version-0.2.md | Updates the timestamped-output spike notes to reflect capability reporting + diagnostics as the first step. |
| doc/reviews/review-00069.md | Adds review record describing prior issues and their resolutions (capability enforced on `MidiSink`, test asserts backend-advertised capability). |
| doc/plans/plan-2026-05-03-04.md | Adds a plan describing capability metadata, best-effort truthfulness, diagnostic sink behavior, and verification matrix. |
| crates/host-midi/src/midir.rs | Adds `MidirSink::TIMING_CAPABILITY`, implements `MidiTimingCapabilities`, and adds a capability test. |
| crates/host-midi/src/lib.rs | Updates crate-level docs to mention `MidiTimingCapability` reporting for midir-backed output. |
| crates/host-midi/README.md | Updates README to mention best-effort timing capability reporting and where that logic lives. |
| crates/host-cpal/src/cpal/control.rs | Implements `MidiTimingCapabilities` for `RtProducer` and adds tests for `at_sample` preservation + capability reporting. |
| crates/host-cpal/src/cpal.rs | Pure formatting adjustments (line wrapping). |
| crates/core/src/sink/midi.rs | Adds timing capability API, makes `MidiSink` require capability reporting, adds `DiagnosticSink` + summaries and tests. |
</details>






<!-- gh-id: 3178897176 -->
### Copilot on [`crates/host-midi/src/lib.rs:14`](https://github.com/cmk/agogo/pull/69#discussion_r3178897176) (2026-05-03 22:54 UTC)

nit: The rustdoc says "sub-us scheduling"; elsewhere in the repo the unit is consistently written as "sub-µs". Consider switching to "sub-µs" (or spelling out "sub-microsecond") to avoid ambiguity and keep terminology consistent.


<!-- gh-id: 3178897181 -->
### Copilot on [`crates/core/src/sink/midi.rs:206`](https://github.com/cmk/agogo/pull/69#discussion_r3178897181) (2026-05-03 22:54 UTC)

nit: `DiagnosticSink` is described as a diagnostic helper, but unlike `TestSink` its rustdoc doesn’t explicitly call out that it is not RT-safe (it takes a `Mutex` on every record). Consider adding an explicit "Not RT-safe" note to reduce the chance of it being used from the audio callback path by mistake.

<!-- gh-id: 3178900913 -->
#### ↳ cmk ([2026-05-03 22:57 UTC](https://github.com/cmk/agogo/pull/69#discussion_r3178900913))

Fixed in the next review-round commit by restoring the repo-standard `sub-µs` spelling in the crate rustdoc.

<!-- gh-id: 3178901000 -->
#### ↳ cmk ([2026-05-03 22:57 UTC](https://github.com/cmk/agogo/pull/69#discussion_r3178901000))

Fixed in the next review-round commit by adding an explicit not-RT-safe note to `DiagnosticSink`'s rustdoc and pointing audio callback users at `RtProducer`.
