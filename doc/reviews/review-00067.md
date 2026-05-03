# PR #67 — Remove SampleTickConn and boundary panics

## Summary

Replaces the conn-shaped runtime `SampleTickConn` bridge with fixed-PPQN,
tempo-aware sample-rate dispatch. Musical scheduling now computes
`Tick + Tempo -> S044/S048/...` directly and then uses the existing
`SxxxI064` static whole-sample conns. Decimal `Micro` / `Pico`
conversions remain only at SI-duration boundaries such as delay and
offset.

Also adds a boundary-panic discipline: user-reachable invalid values must
be rejected at CLI/config/host-command boundaries rather than by bridge
or scheduler panics. The new `scripts/check-boundary-panics.sh` gate is
wired into pre-commit, host tempo commands now reject zero BPM, and
unsupported sample-rate dispatch returns `None` instead of panicking.

Validation:

- `cargo fmt --all -- --check`
- `scripts/check-pii.sh`
- `scripts/check-floats.sh`
- `scripts/check-layers.sh`
- `scripts/check-connections.sh`
- `scripts/check-boundary-panics.sh`
- `cargo test --workspace --quiet`
- `cargo clippy --all-targets --quiet -- -D warnings`
