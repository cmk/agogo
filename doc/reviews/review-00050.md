# PR #50 — agogo snapshot observation

## Summary

See `doc/plans/plan-2026-04-30-02.md` for sprint context.

This PR lands the agogo-owned snapshot observation slice:

- adds `agogo_stdio::snapshot::AgogoSnapshot` v1 with serde support,
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

- `cargo test -p agogo-stdio`
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
  `crates/stdio/src/snapshot.rs:207`

  When the async reader races the audio callback, this `Release` store
  does not prevent the following payload stores from becoming visible
  before the odd epoch marker on weakly ordered targets. A reader can
  therefore see updated fields while both epoch loads still read the
  previous even value and return a torn snapshot. Since this slot is
  intended for RT-to-async cross-thread handoff, the epoch protocol
  needs stronger ordering/fences, or another documented single-writer
  seqlock implementation, before `begin_epoch == end_epoch` is trusted.
