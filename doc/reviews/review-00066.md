# PR #66 - Replace SampleTime helpers with explicit Conns

## Summary

This removes the `SampleTime` convenience trait and replaces its hidden
Q48.16 conversion helpers with explicit named sample connections.

The sample connection module now publishes transparent `SxxxQ016` isos
for the six supported sample-rate newtypes and composed left-sided
`SxxxI064` conns through upstream `Q016Q000` and `Q000I064`. Law battery
coverage was added for all twelve new conns, with spot checks pinning
whole-sample and negative fractional `S048I064` rounding.

The sync/control/host stack no longer carries `R: SampleTime` bounds.
Generic state containers remain rate-typed, but methods that need
conversion behavior are expanded for the six concrete `Sxxx` rates, so
future conversion policy has to be expressed as a named conn or explicit
raw-bit representation access.

Validation:

- `cargo fmt --all -- --check`
- `scripts/check-pii.sh`
- `scripts/check-floats.sh`
- `scripts/check-layers.sh`
- `scripts/check-connections.sh`
- `cargo test -p agogo-core --quiet`
- `cargo test --workspace --quiet`
- `cargo clippy --all-targets --quiet -- -D warnings`

## Local review (2026-05-03)

**Branch:** plan-2026-05-03-02
**Commits:** 3 (origin/main..plan-2026-05-03-02)
**Reviewer:** Codex (`codex review --base origin/main`)

---

The code and tests pass, but the newly introduced public sample-count connection API is awkwardly incomplete compared with the rest of the module and will fail for straightforward downstream use unless an implementation-detail trait is imported.

Review comment:

- [P2] Add inherent methods for SxxxI064 conns — crates/core/src/conn/sample.rs:331-341
  Callers using the new sample-count conns as advertised, e.g. `use agogo_core::conn::sample::S048I064; S048I064.inner(1)`, will not compile unless they also import `connections::conn::ViewL`. The existing rate and pico conns in this module expose inherent `ceil`/`inner` wrappers, so the new public `SxxxI064` family should do the same to keep the intended API usable and consistent.

## Local review (2026-05-03)

**Branch:** plan-2026-05-03-02
**Commits:** 4 (origin/main..plan-2026-05-03-02)
**Reviewer:** Codex (`codex review --base origin/main`)

---

The changes compile and the workspace tests pass. I did not find any discrete introduced defects that would break existing behavior or the documented migration to explicit sample-rate connections.


<!-- gh-id: 3178642685 -->
### Copilot on [`doc/plans/plan-2026-05-03-02.md:104`](https://github.com/cmk/agogo/pull/66#discussion_r3178642685) (2026-05-03 19:29 UTC)

nit: this plan has two small factual inconsistencies: the candidate module path here should match the repository's `crates/host-cpal/src/cpal/callback.rs`, and the earlier “Build Gates” list omits `scripts/check-connections.sh` even though the completion/verification section says it was part of the run.


<!-- gh-id: 3178642690 -->
### Copilot on [`crates/core/src/control/sync/pulse.rs:123`](https://github.com/cmk/agogo/pull/66#discussion_r3178642690) (2026-05-03 19:29 UTC)

The public `pulse_train_s044`/`_s048`/… entry points are introduced here without any API docs, while the existing pulse-train contract remains documented only on the private `pulse_train_with` helper. That leaves downstream users without rustdoc for the new public surface and effectively hides the parameter/return semantics that used to be documented on the public function.

<!-- gh-id: 4216730905 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-05-03 19:29 UTC](https://github.com/cmk/agogo/pull/66#pullrequestreview-4216730905))

## Pull request overview

This PR removes the `SampleTime` trait from the sample-time stack and makes sample-count conversions explicit through named connections in `agogo-core`. It fits the codebase’s broader typed-connection design by replacing hidden conversion policy with concrete `SxxxQ016` / `SxxxI064` surfaces and by pushing rate-specific behavior to explicit `S044`/`S048`/… implementations.

**Changes:**
- Replaces `SampleTime` helper methods with explicit sample conversion connections in `crates/core/src/conn/sample.rs`, including new law-battery coverage.
- Refactors sync/control/host code to keep generic state containers but expose rate-specific methods for the six supported sample-rate types.
- Updates CLI/trace call sites and adds plan/review documentation for the migration.

### Reviewed changes

Copilot reviewed 12 out of 12 changed files in this pull request and generated 3 comments.

<details>
<summary>Show a summary per file</summary>

| File | Description |
| ---- | ----------- |
| `doc/reviews/review-00066.md` | Adds the PR review record and final local review notes. |
| `doc/plans/plan-2026-05-03-02.md` | Captures the implementation plan, verification targets, and completion notes for the SampleTime removal. |
| `crates/host/src/bridge.rs` | Removes the obsolete `SampleTime` bound from the bridge helper that applies control updates to a playhead. |
| `crates/host-cpal/src/cpal/callback.rs` | Switches callback state from a `SampleTime`-bounded impl to concrete per-rate `on_buffer` impls. |
| `crates/core/src/control/transport.rs` | Refactors `Playhead` to shared generic internals plus six concrete rate-specific `on_buffer` entry points. |
| `crates/core/src/control/sync/source.rs` | Reworks `PhaseSource` into shared helper logic with concrete per-rate phase/feed methods. |
| `crates/core/src/control/sync/pulse.rs` | Replaces the generic public pulse generator with a shared helper and six per-rate public constructors. |
| `crates/core/src/control/sync/pll.rs` | Splits PLL logic into generic helper methods with concrete per-rate public APIs. |
| `crates/core/src/control/sync/detect.rs` | Splits peak-detector processing into a shared helper and per-rate public `process` methods. |
| `crates/core/src/conn/sample.rs` | Removes `SampleTime`, adds explicit Q16/i64 sample connections, and expands connection test coverage. |
| `crates/cli/src/trace/sync.rs` | Updates sync tracing to the concrete S048 pulse API and raw-bit sample accessors. |
| `crates/cli/src/run.rs` | Replaces generic runner dispatch with six monomorphic rate-specific run functions. |
</details>



<details>
<summary>Comments suppressed due to low confidence (1)</summary>

**crates/cli/src/run.rs:233**
* The `run_with_rate` doc comment is now attached to `ChannelMix`, so rustdoc will describe this struct as the “rate-monomorphic body” and mention `PhaseSource`/`CallbackState` instead of the mix flags it actually contains. That comment either needs to move down to the generated runner functions or be rewritten for `ChannelMix`.
```
/// Rate-monomorphic body. The concrete sample type plumbs all the way down
/// into `PhaseSource<Sxxx>` / `Playhead<Sxxx>` / `CallbackState<Sxxx>` so the
/// audio callback never branches on rate at runtime. The
/// `Send + 'static` bound is what cpal's `data_callback` requires
/// of the moved closure.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
```
</details>



<!-- gh-id: 3178642696 -->
### Copilot on [`doc/plans/plan-2026-05-03-02.md:192`](https://github.com/cmk/agogo/pull/66#discussion_r3178642696) (2026-05-03 19:29 UTC)

The review note here no longer matches the current codebase: `crates/core/src/conn/sample.rs:322` is now the new `connections::iso!` block, not the old `SampleTime` helper site, so this line reference and description are stale and will send readers to the wrong place.


<!-- gh-id: 3178647826 -->
#### ↳ cmk ([2026-05-03 19:33 UTC](https://github.com/cmk/agogo/pull/66#discussion_r3178647826))

Fixed both plan inconsistencies: the candidate path now uses `crates/host-cpal/src/cpal/callback.rs`, and the build-gate list includes `scripts/check-connections.sh`.

<!-- gh-id: 3178648158 -->
#### ↳ cmk ([2026-05-03 19:33 UTC](https://github.com/cmk/agogo/pull/66#discussion_r3178648158))

Added rustdoc to the generated public `pulse_train_sxxx` functions so the parameter, return, jitter, seed, and panic contracts are visible on each public entry point.

<!-- gh-id: 3178648475 -->
#### ↳ cmk ([2026-05-03 19:34 UTC](https://github.com/cmk/agogo/pull/66#discussion_r3178648475))

Updated the Review entry to remove the stale line number and describe the completed cleanup in current terms: `SampleTime` helpers were replaced with named whole-sample conns, with raw-bit access left explicit on the sample newtypes.
