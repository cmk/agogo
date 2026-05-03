# PR #63 — Command admission envelope

## Summary

Adds the v0.2 command admission envelope to `agogo-host` without adding
a direct stdio-core dependency.

- Adds fixed-capacity command metadata for command id, source id,
  RT-buffer deadline, coalesce key, and accepted / rejected / late
  admission outcomes.
- Wraps ordered controls in `CommandEnvelope`, tracks the current RT
  buffer epoch, and exposes missed-deadline drain faults on the
  callback side.
- Updates `AgogoDriver` tool calls to return structured admission JSON
  and to reject unsupported time domains, stale deadlines, ordered
  coalesce keys, and full queues before they can silently mutate the RT
  queue.
- Repairs the active host crate docs and exports the new admission
  types for the future stdio-core binding.

Verification:

- `cargo fmt --all -- --check`
- `scripts/check-pii.sh`
- `scripts/check-floats.sh`
- `scripts/check-layers.sh`
- `cargo test -p agogo-host --quiet`
- `cargo test --workspace --quiet`
- `cargo clippy --all-targets --quiet -- -D warnings`
- `cargo test -p agogo-cli --no-default-features --no-run`

## Local review (2026-05-02)

**Branch:** plan-2026-05-02-08
**Commits:** 3 (origin/main..plan-2026-05-02-08)
**Reviewer:** Codex (`codex review --base origin/main`)

---

The new admission contract can falsely report success for commands that miss the buffer boundary, including a silent drop through the compatibility `try_push` path. These issues break the deadline/admission guarantees introduced by the patch.

Full review comments:

- [P2] Don't accept tempo writes that miss their buffer deadline — crates/host/src/bridge.rs:362-362
  If `agogo.tempo.set` races the RT boundary, the epoch can advance after `reject_if_not_admissible` passes but before this store; `begin_buffer()` has already snapped the old tempo for the declared deadline buffer, and scalar controls have no later `MissedDeadline` drain fault. The call therefore returns `accepted` while the tempo is only observable in a later buffer, breaking the new admission contract for accepted last-value controls near buffer boundaries.

- [P2] Don't report late compatibility pushes as success — crates/host/src/bridge.rs:402-402
  When an existing caller uses `try_push` near a buffer boundary, `AdmissionMetadata::next_buffer` can choose `current_epoch + 1`, then the RT thread can call `begin_buffer()` before `admit_ordered` re-checks the epoch. In that case `admit_ordered` returns a `Late` outcome without enqueuing anything, but this wildcard arm converts it to `Ok(())`, so ordered commands are silently lost even though the API still documents visible errors on non-enqueue.

## Local review (2026-05-02)

**Branch:** plan-2026-05-02-08
**Commits:** 4 (origin/main..plan-2026-05-02-08)
**Reviewer:** Codex (`codex review --base origin/main`)

---

The patch still has a deadline race in scalar tempo admission: a late/rejected response can be returned after the RT side has already observed the command. That breaks the admission truthfulness guarantee introduced by the change.

Review comment:

- [P2] Don't publish tempo before late status is settled — crates/host/src/bridge.rs:368-376
  When a tempo admission races the RT thread after this `swap`, `begin_buffer()` can increment the epoch and load the new tempo before the second epoch check sees `deadline <= current_epoch`. The method then returns `late` and may roll back future snapshots, but the supposedly late command already affected that buffer, so callers/logs see a rejected/late admission that was not rejected before touching the RT side.

## Local review (2026-05-02)

**Branch:** plan-2026-05-02-08
**Commits:** 5 (origin/main..plan-2026-05-02-08)
**Reviewer:** Codex (`codex review --base origin/main`)

---

The patch introduces admission semantics that can report accepted tempo commands with altered deadlines and misleading coalesce metadata, plus new proptests that violate the repo's documented domain-coverage rule. These should be fixed before considering the patch correct.

Full review comments:

- [P2] Preserve caller-declared tempo deadlines — crates/host/src/bridge.rs:370-371
  When a tempo admission races the RT callback after the initial admissibility check, this branch rewrites the caller's `deadline_buffer` to a later buffer and still returns `accepted`. For an explicit next-buffer deadline that was already missed, scalar controls have no later `MissedDeadline` drain fault, so the admission log reports success against a different deadline than the caller declared.

- [P2] Reject non-tempo coalesce keys for tempo writes — crates/host/src/driver.rs:213-221
  For `agogo.tempo.set`, callers can pass `coalesce_key: null` or any arbitrary string here and the bridge will still accept the write, even though all tempo writes share one atomic scalar and therefore overwrite each other regardless of key. Two accepted tempo commands with different keys can be silently coalesced while the response echoes misleading metadata; restrict tempo admissions to the fixed `tempo` key or reject overrides.

- [P2] Cover the full deadline domain in proptests — crates/host/src/bridge.rs:709-709
  This deadline strategy only samples `0..8`, and the other new deadline properties use similarly tiny ranges, so the proptests never exercise the `u64` boundary values used by `CommandDeadline` and the epoch arithmetic. The repo's proptest rule requires the full input domain or a documented bound with explicit boundary spot checks; otherwise regressions around saturation or wrap at `u64::MAX` will not be caught.

## Local review (2026-05-02)

**Branch:** plan-2026-05-02-08
**Commits:** 6 (origin/main..plan-2026-05-02-08)
**Reviewer:** Codex (`codex review --base origin/main`)

---

The patch still has scalar tempo admission races that can report `late` for commands that actually affect the RT side, breaking the new admission truthfulness guarantee.

Full review comments:

- [P2] Don't overwrite a pending tempo before publishing its generation — crates/host/src/bridge.rs:392-400
  With a previous tempo still pending, a second admission writes `pending_tempo_raw` and `pending_tempo_deadline` before publishing its own generation. If the RT callback begins in that window, it can claim the old generation but read the new tempo/deadline; then this admission's deadline recheck clears the new generation and returns `late`, even though the new tempo already affected the callback and is attributed to the prior command id. This breaks the declared admission metadata for back-to-back tempo writes near a buffer boundary.

- [P2] Don't time out after the RT side claims a tempo — crates/host/src/bridge.rs:504-511
  Under a boundary race where `begin_buffer()` has already claimed this pending tempo by CASing `pending_tempo_generation` to 0 but is preempted before it stores `applied_tempo_generation`, this fixed 64-spin wait can return false and make `admit_tempo` report `late`. The callback then resumes and applies the tempo, so the admission result says the command missed its deadline even though it affected the RT buffer.

## Local review (2026-05-02)

**Branch:** plan-2026-05-02-08
**Commits:** 7 (origin/main..plan-2026-05-02-08)
**Reviewer:** Codex (`codex review --base origin/main`)

---

The patch introduces a boundary race in scalar tempo admission where a later late write can prevent an earlier accepted pending tempo from being consumed. That violates the new admission truthfulness guarantee, so the patch should not be considered correct as-is.

Review comment:

- [P2] Preserve pending tempo while publishing a replacement — crates/host/src/bridge.rs:393-393
  When a second `agogo.tempo.set` starts just before `begin_buffer()` while an earlier accepted tempo is still pending for that same buffer, this write marks the seqlock odd and the RT side returns `None` instead of consuming the older value. If the new write then observes the advanced epoch and returns `late`, it clears the slot, so the earlier accepted tempo is silently dropped and never affects the deadline buffer.

## Local review (2026-05-02)

**Branch:** plan-2026-05-02-08
**Commits:** 8 (origin/main..plan-2026-05-02-08)
**Reviewer:** Codex (`codex review --base origin/main`)

---

The admission envelope, deadline checks, driver JSON responses, and compatibility paths appear consistent with the stated v0.2 contract, and the relevant host/workspace tests pass. I did not find any discrete correctness issues introduced by this patch.

<!-- gh-id: 3177550002 -->
### Copilot on [`crates/host/src/bridge.rs:738`](https://github.com/cmk/agogo/pull/63#discussion_r3177550002) (2026-05-03 03:07 UTC)

`begin_buffer()` uses `AtomicU64::fetch_add(1)` to advance `buffer_epoch`. If `buffer_epoch` ever reaches `u64::MAX`, `fetch_add` will wrap the stored epoch back to 0 (even though the returned `epoch` saturates to `u64::MAX`), causing the producer/consumer epochs to diverge and potentially making stale deadlines appear admissible. Consider using a saturating update (e.g., `fetch_update` that keeps `u64::MAX` pinned) or otherwise preventing wraparound in the stored atomic value.

<!-- gh-id: 3177550006 -->
### Copilot on [`crates/host/src/driver.rs:184`](https://github.com/cmk/agogo/pull/63#discussion_r3177550006) (2026-05-03 03:07 UTC)

`parse_metadata()` allows callers to supply an explicit `command_id`, but it does not advance `next_command_id` in that case. This means a later call that omits `command_id` can reuse an id that was previously supplied explicitly, producing duplicate ids in admission responses/logs. Consider updating `next_command_id` when an explicit id is provided (e.g., bump to at least `provided_id + 1`, with saturation) or documenting/enforcing that explicit ids must not overlap the driver-generated range.

<!-- gh-id: 4215810274 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-05-03 03:07 UTC](https://github.com/cmk/agogo/pull/63#pullrequestreview-4215810274))

## Pull request overview

Adds the v0.2 command admission envelope to `agogo-host`, introducing structured admission metadata/outcomes for both scalar (tempo) and ordered commands, and updating the driver surface to return structured admission JSON without adding a direct `stdio-core` dependency.

**Changes:**
- Introduces admission metadata types (`CommandId`, `SourceId`, `CommandDeadline`, `CoalesceKey`, etc.) and wraps ordered commands in `CommandEnvelope` with deadline-aware RT draining.
- Updates `AgogoDriver` tool handling to parse optional admission metadata and return structured admission JSON (`status`, ids, deadline, optional `reason`).
- Adds a `rust-fsm`-backed tempo-slot state machine to make scalar tempo admission truthfulness explicit under buffer-boundary races.

### Reviewed changes

Copilot reviewed 7 out of 8 changed files in this pull request and generated 2 comments.

<!-- gh-id: 3177680582 -->
#### Reply from cmk ([2026-05-03 05:36 UTC](https://github.com/cmk/agogo/pull/63#discussion_r3177680582))

Fixed. `begin_buffer()` now advances `buffer_epoch` with a saturating `fetch_update`, so the stored epoch remains pinned at `u64::MAX` instead of wrapping to zero. Added `begin_buffer_saturates_stored_epoch_at_u64_max` to cover the boundary.

<!-- gh-id: 3177680762 -->
#### Reply from cmk ([2026-05-03 05:36 UTC](https://github.com/cmk/agogo/pull/63#discussion_r3177680762))

Fixed. Explicit `command_id` values now reserve the driver's generated-id range via `reserve_generated_ids_through`, and generated ids advance with a saturating atomic update. Added `explicit_command_id_advances_generated_ids` for the explicit 42 then generated 43 case.
