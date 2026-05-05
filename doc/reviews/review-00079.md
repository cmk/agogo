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

## Local review (2026-05-04)

**Branch:** debt-connections-origin-head
**Commits:** 1 (origin/main..debt-connections-origin-head)
**Reviewer:** Codex (`codex review --base origin/main`)

---

The code migration compiles and the workspace tests, layer check, connection check, and clippy pass. The only finding is non-blocking stale documentation introduced by the method rename.

Review comment:

- [P3] Update stale I064U032 method references — crates/chan/src/conn/float_boundary.rs:117-120
  After switching the implementation to `I064U032.upper`, the adjacent prose still says the widening is `I064U032.inner`, but that method no longer exists on the upgraded upstream `Conn`. This is a small docs drift that can mislead future maintenance of this boundary helper; the similar test comment below should be updated at the same time.
