# PR #64 — Forbid hand-built unlawful connections

## Summary

Forbids agogo-local hand-built connection construction and fixes the
connection-domain issues that motivated the rule.

- Ports the Codex audit harness from `connections`, bumps the
  `connections` pin to the revision with declaration/composition
  macros, and wires a pre-commit gate that rejects direct
  `Conn::new_l` / `Conn::new_r` / local wrapper construction in
  production code.
- Makes `TICKTIME` total over the declared `Tick(u64)` domain by using
  `Time::End` as the top value, and makes `TIMETIME` total with
  explicit refinement-order law tests from the upstream law battery.
- Removes the connection-shaped `quantize_at` / `RuntimeConn<Tick,
  Time>` surface; callers use the lawful `TICKTIME` connection plus an
  explicit resolution `Time` for fixed-grid binning.
- Migrates the remaining lawful static markers (`TICKTIME`,
  `TIMETIME`, `GRIDGRID`, `U007U008`, `U004U008`) to upstream
  `connections::triple!`, removes the temporary constructor allowlist,
  and removes the unused `num-rational` / `WHOLTICK` surface.
- Updates `AGENTS.md` and the recurring audit prompt to make
  generator-cooked unlawful connections explicitly forbidden.

Verification:

- `cargo fmt -p agogo-core -p agogo-cli -- --check`
- `scripts/check-pii.sh`
- `scripts/check-floats.sh`
- `scripts/check-layers.sh`
- `scripts/check-connections.sh`
- `cargo test -p agogo-core time::conn -- --nocapture`
- `cargo test -p agogo-core conn::midi -- --nocapture`
- `cargo test --workspace --quiet`
- `cargo clippy --all-targets --quiet -- -D warnings`

## Local review (2026-05-03)

**Branch:** plan-2026-05-02-09
**Commits:** 12 (origin/main..plan-2026-05-02-09)
**Reviewer:** Codex (`codex review --base origin/main`)

---

The Rust code compiles and tests pass, but the patch introduces unresolved rustdoc links that break documentation builds when warnings are denied.

Full review comments:

- [P3] Restore `Conn` rustdoc link resolution — crates/core/src/conn/fixed.rs:34-34
  Removing `Conn` from this import leaves the module-level [`Conn`] links above unresolved; `RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps` now fails in this file. Either qualify those links as `connections::conn::Conn` or otherwise keep the name in rustdoc scope.

- [P3] Restore sample module `Conn` rustdoc links — crates/core/src/conn/sample.rs:67-67
  With `Conn` removed from this import, the module docs' [`Conn`] links no longer resolve, so docs builds with warnings denied fail for `conn/sample.rs`. Please qualify the links or keep `Conn` available for rustdoc.

