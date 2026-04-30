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

## Local review (2026-04-30)

**Branch:** `plan-2026-04-30-02`  
**Reviewer:** Codex

No local review findings after the epoch-guarded snapshot read/write
fix. Residual risk is the direct stdio-core `ObservationDispatcher`
adapter, which remains deferred until the dependency can be consumed
without changing agogo's MSRV.
