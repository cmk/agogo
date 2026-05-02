# PR #56 — feat: Bump connections and name cast conversions

## Summary

This PR bumps `connections` from
`01a2bf92cbe3d3e7eda7ab91f5b4dfc9cc6f4242` to
`238a8aaf913db48a31c1d7409504fb9c2745d3cd` and migrates agogo's local
connection definitions to the current kind-tagged `ConnL` / `ConnR`
API.

It keeps existing `.ceil()`, `.inner()`, and `.floor()` call sites
working through local zero-sized marker values, converts MIDI U7/U4
casts to one-sided `ConnL` values, and replaces matching manual
saturating integer casts with named upstream integer Conns.

Validation:

- `cargo fmt --all -- --check`
- `scripts/check-pii.sh`
- `scripts/check-floats.sh`
- `scripts/check-layers.sh`
- `cargo test --workspace --quiet`
- `cargo clippy --all-targets --quiet -- -D warnings`
- `cargo test -p agogo-cli --no-default-features --no-run`

## Local review (2026-05-01)

**Branch:** plan-2026-05-02-01
**Commits:** 3 (origin/main..plan-2026-05-02-01)
**Reviewer:** Codex (`codex review --base origin/main`)

---

No actionable correctness issues were found; the workspace tests, warning-denied clippy, and the no-default-features CLI check pass for the reviewed diff.
