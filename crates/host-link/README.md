# agogo-host-link

Feature-gated Ableton Link integration for `agogo`, wrapping the
upstream [`rusty_link`](https://github.com/anzbert/rusty_link) binding.

## Setup

`rusty_link` lives at `ext/rusty_link` under the workspace root. The
`ext/` tree is gitignored by agogo's convention, so each working copy
(or CI runner) must clone rusty_link itself, with submodules, at the
pinned revision:

```sh
git clone --recursive https://github.com/anzbert/rusty_link.git ext/rusty_link
git -C ext/rusty_link checkout 5b3f44e81b0aa30dae4b0650b2f5882048c0b842
git -C ext/rusty_link submodule update --init --recursive
```

The pinned rev is reproduced in the workspace `Cargo.toml` so
readers can re-check it without digging.

Building requires:

- CMake ≥ 3.15.
- A C++ compiler (Xcode Command Line Tools on macOS, `build-essential`
  on Debian/Ubuntu, Visual Studio Build Tools on Windows).

## Feature gate

The `link` feature on `agogo-cli` pulls this crate in. Default builds
skip it, so `cargo build --workspace` and CI stay CMake-free:

```sh
cargo build -p agogo-cli                    # no Link
cargo build -p agogo-cli --features link    # with Link
```
