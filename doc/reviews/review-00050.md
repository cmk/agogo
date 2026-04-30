# PR #50 — agogo snapshot observation

## Summary

See `doc/plans/plan-2026-04-30-02.md` for sprint context.

This PR lands the agogo-owned snapshot observation slice:

- adds `agogo_host::snapshot::AgogoSnapshot` v1 with serde support,
  fixed-point JSON number wrappers, and proptest round-trip coverage;
- adds a fixed-capacity atomic `SnapshotSlot` so the RT side can write
  compact snapshot frames while serialization and observation dispatch
  stay off-thread;
- adds a stdio-core-shaped `ObservationParams` publisher using
  `Other("agogo-state")`, `agogo.main`, `Create`, full-snapshot
  `Patch`, and optional `Destroy`;
- documents the v1 schema in `doc/designs/snapshot.md` and aligns
  `doc/versions/version-0.4.md` with the MSRV-driven direct dependency
  deferral.

## Test plan

- `cargo test -p agogo-host`
- `cargo test --workspace --quiet`
- `cargo clippy --all-targets --quiet -- -D warnings`
- `cargo fmt --all -- --check`
- `scripts/check-layers.sh`
- `scripts/check-floats.sh`
- `scripts/check-pii.sh`
- `git diff --check`

## Local review (2026-04-30, initial)

**Branch:** `plan-2026-04-30-02`
**Reviewer:** Codex

No local review findings after the epoch-guarded snapshot read/write
fix. Residual risk is the direct stdio-core `ObservationDispatcher`
adapter, which remains deferred until the dependency can be consumed
without changing agogo's MSRV.

## Local review (2026-04-30, seqlock follow-up)

**Branch:** `plan-2026-04-30-02`
**Reviewer:** Codex reviewer

---

The new snapshot handoff can return inconsistent observations under the
intended concurrent RT-writer/async-reader usage on weakly ordered
platforms. Tests pass, but they do not exercise this memory-ordering
race.

### Findings

- **[P2] Use a real seqlock barrier around snapshot writes** —
  `crates/host/src/snapshot.rs:207`

  When the async reader races the audio callback, this `Release` store
  does not prevent the following payload stores from becoming visible
  before the odd epoch marker on weakly ordered targets. A reader can
  therefore see updated fields while both epoch loads still read the
  previous even value and return a torn snapshot. Since this slot is
  intended for RT-to-async cross-thread handoff, the epoch protocol
  needs stronger ordering/fences, or another documented single-writer
  seqlock implementation, before `begin_epoch == end_epoch` is trusted.

<!-- gh-id: 3167843289 -->
### Copilot on [`crates/host/src/snapshot.rs:690`](https://github.com/cmk/agogo/pull/50#discussion_r3167843289) (2026-04-30 12:19 UTC)

`publish_destroy` always dispatches a `Destroy`, even when no `Create` has ever been published (`created == false` / `last_seq == 0`). This contradicts the wire contract described in the docs (first notification should be `Create`, `Destroy` only on graceful unmount after creation) and can produce a destroy for a non-existent form. Consider making `publish_destroy` a no-op unless `self.created` is true (and/or ensuring a `Create` is sent before `Destroy`), and updating `last_seq`/`created` to avoid emitting duplicate `Destroy` messages if called multiple times.
```suggestion
            if !self.created {
                return;
            }

            let seq = self.last_seq.saturating_add(1);
            sink.dispatch(ObservationParams {
                stream_id: self.stream_id.clone(),
                seq,
                op: ObservationOp::Destroy {
                    form_id: self.form_id.clone(),
                },
            });
            self.last_seq = seq;
            self.created = false;
```

<!-- gh-id: 4205014712 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-04-30 12:19 UTC](https://github.com/cmk/agogo/pull/50#pullrequestreview-4205014712))

## Pull request overview

This PR adds the agogo-owned snapshot observation payload + RT-safe handoff implementation in `agogo-host`, and documents the v1 wire contract/schema for downstream (TUI/stdio-core) consumers.

**Changes:**
- Add `agogo_host::snapshot` with `AgogoSnapshot` v1 (serde + fixed-point JSON number wrappers), an atomic RT→async `SnapshotSlot`, and a stdio-core-shaped observation publisher.
- Add/align documentation for the v1 schema/RT boundary and update sprint/version tracking docs.
- Export the new snapshot module from `agogo-host` and add the missing direct `serde` dependency.

### Reviewed changes

Copilot reviewed 7 out of 8 changed files in this pull request and generated 3 comments.

<details>
<summary>Show a summary per file</summary>

| File | Description |
| ---- | ----------- |
| doc/versions/version-0.4.md | Updates v0.4 sprint slot status/scope and links the new snapshot design doc. |
| doc/reviews/review-00050.md | Adds the PR review record and local test plan. |
| doc/plans/plan-2026-04-30-02.md | Updates sprint review notes (MSRV-driven stdio-core dependency deferral). |
| doc/designs/snapshot.md | Documents the AgogoSnapshot v1 wire contract, schema, cadence, and RT boundary. |
| crates/host/src/snapshot.rs | Implements snapshot types, fixed-point serde wrappers, atomic slot, publisher, and tests. |
| crates/host/src/lib.rs | Exposes the new `snapshot` module and re-exports `AgogoSnapshot`/`SnapshotSlot`. |
| crates/host/Cargo.toml | Adds `serde` to dependencies (required for derives). |
| Cargo.lock | Locks the new direct dependency edge for `serde`. |
</details>






<!-- gh-id: 3167843344 -->
### Copilot on [`crates/host/src/snapshot.rs`](https://github.com/cmk/agogo/pull/50#discussion_r3167843344) (2026-04-30 12:19 UTC)

The writer uses `Ordering::Release` for every field store inside the epoch-guarded write. Since readers synchronize using the `write_epoch` acquire/release handshake, these per-field `Release` operations are stronger than necessary and add overhead on the RT thread. Consider using `Relaxed` for the individual field stores (and the `seq` store) while keeping the begin/end `write_epoch` stores as `Release` to preserve the happens-before relationship.

<!-- gh-id: 3167843358 -->
### Copilot on [`crates/host/src/snapshot.rs:777`](https://github.com/cmk/agogo/pull/50#discussion_r3167843358) (2026-04-30 12:19 UTC)

`parse_scaled_decimal` depends on `serde_json::Number::to_string()` and then parses the resulting string. For numbers that `serde_json` represents as floats, `to_string()` can emit representations that differ from the input (and may use scientific notation for large magnitudes), which can lead to surprising parse failures or scale/precision errors. If the intent is to only accept plain decimal notation with up to `scale.ilog10()` fractional digits, consider explicitly detecting and rejecting scientific notation with a clearer error, or switching to a parsing approach that operates on the original JSON token/string form to avoid float reformatting artifacts.

<!-- gh-id: 3168000895 -->
#### ↳ cmk ([2026-04-30 12:46 UTC](https://github.com/cmk/agogo/pull/50#discussion_r3168000895))

Addressed in this review round. `publish_destroy` now returns without dispatching before the initial `Create`, records the destroy seq, clears `created`, and is idempotent on repeated unmount calls. Added regression coverage for both no-create and duplicate-destroy cases.

<!-- gh-id: 3168002108 -->
#### ↳ cmk ([2026-04-30 12:46 UTC](https://github.com/cmk/agogo/pull/50#discussion_r3168002108))

Addressed in this review round. The seqlock epoch transitions and fences remain the synchronization mechanism, and the individual payload field stores plus `seq` store are now `Relaxed` to reduce RT-thread overhead.

<!-- gh-id: 3168003475 -->
#### ↳ cmk ([2026-04-30 12:46 UTC](https://github.com/cmk/agogo/pull/50#discussion_r3168003475))

Addressed in this review round. `parse_scaled_decimal` now explicitly rejects scientific notation with a clear error before parsing the plain decimal form, and there is a regression test for that path.
