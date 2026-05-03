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
