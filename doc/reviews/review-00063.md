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
