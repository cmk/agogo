# PR #58 — doc: Mark facade feature gates in rustdoc

## Summary

Marks the `agogo` facade crate's feature-gated modules with docs.rs
`doc_cfg` metadata, so generated documentation can show which feature
enables each public namespace:

- `agogo::core`
- `agogo::host`
- `agogo::host::cpal`
- `agogo::host::link`
- `agogo::host::midi`

Configures the facade package's docs.rs metadata to build documentation
with all facade features and `--cfg docsrs`. The change does not add new
runtime API or alter existing feature forwarding; `host` still implies
`core`, and `link` still enables `link_impl/rusty-link`.

The external prelude suggestion is intentionally deferred until the
facade's stable convenience import set is clearer from downstream usage.

## Test plan

- `cargo fmt --all -- --check`
- `scripts/check-pii.sh`
- `scripts/check-floats.sh`
- `scripts/check-layers.sh`
- `cargo test -p agogo --quiet`
- `cargo test -p agogo --no-default-features --quiet`
- `cargo test -p agogo --no-default-features --features core --quiet`
- `cargo test -p agogo --no-default-features --features host --quiet`
- `cargo test -p agogo --features cpal,link,midi --quiet`
- `cargo clippy --all-targets --quiet -- -D warnings`
- `RUSTDOCFLAGS="--cfg docsrs" cargo +nightly doc -p agogo --all-features --no-deps`
