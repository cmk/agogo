## Summary

- Bump `connections` to gitlab HEAD `5bcc4ed798c7c2e28fe7644da8b0278ee16068a2`.
- Migrate agogo connection declarations from upstream `triple!` / `ViewL` / `ViewR` to `conn_k!` and `ConnL` / `ConnR`.
- Keep agogo's existing marker `.inner(...)` wrappers while updating raw L-side `Conn` calls to upstream `.upper(...)`.

## Verification

- `cargo fmt --check`
- `scripts/check-connections.sh`
- `scripts/check-layers.sh`
- `git diff --check`
- `cargo test --workspace`
- `cargo clippy --all-targets -- -D warnings`
