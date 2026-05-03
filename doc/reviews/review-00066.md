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
