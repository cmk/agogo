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
