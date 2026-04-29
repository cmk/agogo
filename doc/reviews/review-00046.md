# PR #46 — Bump connections to 01a2bf9

## Summary

Bumps the `connections` git dependency from
`33ec62847e8c8072a2d3d7f876533ad6d3998997` to
`01a2bf92cbe3d3e7eda7ab91f5b4dfc9cc6f4242`, picking up upstream
Plans 23, 25, and 26. The breaking-change surface from upstream is
narrow for agogo: only the `connections::int::*` module-path
removal hits, since Plan 25 merged `int/` into `fixed/`. Single
import-path update in `crates/core/src/boundary.rs`:
`connections::int::u32::I064U032` → `connections::fixed::u32::I064U032`.
The `I064U032` Conn name itself is unchanged — only the path
moved.

Other upstream changes in this rev range that agogo does **not**
consume today (no plumbing required):

- Plan 25 NonZero family with `Conn<Extended<X>, NonZero<X>>`
  orientation. agogo doesn't import `connections::NonZero*` today.
- Plan 25 intra-fixed `I*` / `U*` → `Q*` rename. agogo only uses
  cross-prefix names like `I064U032` (still `I064U032`).
- Plan 23 `time/duration.rs` std-time `Duration` family. Available
  for an FFI bridge if/when needed.
- Plan 26 cargo-fmt-blocking pre-commit on the upstream side.

`Cargo.lock` follows the rev bump (connections 0.1.0 → 0.0.0 per
Plan 25 T5's version reset).

## Test plan

- [x] `cargo build --workspace` — clean
- [x] `cargo test --workspace` — 950 + 17 + 17 + 1 doctest pass (unchanged from main)
- [x] `cargo clippy --all-targets -- -D warnings` — clean
- [x] `cargo fmt -p agogo-core -p agogo-cli -- --check` — clean
- [x] `scripts/check-floats.sh` — clean
- [x] No stranded `connections::int::*` import sites: `grep -r 'connections::int' crates/` is empty
