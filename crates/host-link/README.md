# agogo-host-link

Feature-gated Ableton Link integration for `agogo`, wrapping the
upstream [`rusty_link`](https://github.com/anzbert/rusty_link) binding.

## Relationship to the rest of the workspace

This crate is **not** in `[workspace].members`. It's pulled in as an
optional path dep from `crates/cli/Cargo.toml` under the `link`
feature. The detachment is deliberate: it keeps
`cargo test --workspace` (the default CI path) from dragging in
`rusty_link`'s CMake + C++ build chain.

Default CI on `ubuntu-latest`:

```sh
cargo test --workspace            # compiles core + cli only
cargo clippy --all-targets        # same
```

Developers with the `link` feature active do need a working C++
toolchain (Xcode CLT on macOS, `build-essential` on Debian/Ubuntu,
Visual Studio Build Tools on Windows) plus CMake ≥ 3.15 — `rusty_link`'s
`build.rs` invokes CMake on the vendored Ableton Link sources.

## Using the Link feature

```sh
# From the workspace root:
cargo build -p agogo-cli --features link
cargo test  -p agogo-cli --features link
cargo run   -p agogo-cli --features link -- link probe

# Running this crate's tests directly (host-link is not a `-p`
# target from the workspace root since it's outside `members`):
cd crates/host-link && cargo test --features rusty-link
```

## Dependency source

`rusty_link` is fetched from crates.io, pinned to `=0.4.8`
([upstream repo](https://github.com/anzbert/rusty_link)). The
published crate bundles the Ableton Link C++ submodule, so no
additional git-submodule dance is required.

### Offline / co-development with a local rusty_link clone

If you're hacking on `rusty_link` alongside `agogo`, add a gitignored
`.cargo/config.toml` at the workspace root overriding the registry
dep with a path patch:

```toml
[patch.crates-io]
rusty_link = { path = "ext/rusty_link" }
```

With that in place, `cargo build --features link` picks up your local
clone (including any in-flight changes) instead of the registry copy.
The local clone should be recursive so Link's C++ submodule is present:

```sh
git clone --recursive https://github.com/anzbert/rusty_link.git ext/rusty_link
```
