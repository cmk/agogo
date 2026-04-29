# PR #45 — Distribute kitchen-sink arb.rs to per-type submodules

## Summary

`crates/core/src/arb.rs` aggregated proptest strategies for 9 unrelated
domain types plus a runtime synthetic-signal generator (`pulse_train`).
The kitchen-sink layout violates the colocation pattern the rest of the
workspace and upstream `connections` use: every type owns its own
`arb.rs`, no aggregating root file. This PR distributes the strategies
to per-type `arb.rs` submodules and moves `pulse_train` (which isn't
testkit) to its consumer subsystem.

**Strategy migration (T1).** Each strategy moves to `<type>/arb.rs`
gated `#[cfg(any(test, feature = "testkit"))]`:

| Strategy | New location |
|---|---|
| `arb_bpm` | `time/tempo/arb.rs` |
| `arb_sample_rate` | `time/sample/arb.rs` |
| `arb_jitter_sigma` | `time/decimal/arb.rs` |
| `arb_tbase` | `time/tbase/arb.rs` |
| `arb_grid` | `time/grid/arb.rs` |
| `arb_tick` / `arb_time` / `arb_small_time` | `time/tick/arb.rs` |
| `arb_rational_nonneg` | `time/conn/arb.rs` |
| `arb_swing` | `time/swing/arb.rs` |

Inter-strategy dependencies (`arb_swing → arb_tbase`,
`arb_time → arb_grid`) become explicit cross-module imports.
`arb_integer_stc` was already a private fn inside
`sync/sample_tick.rs::tests` and stays put.

**`pulse_train` migration (T2).** Not testkit (used by `cli/sync_trace`
at runtime), so it moves to `crates/core/src/sync/pulse_train.rs` as a
regular `pub` module — declared in `sync.rs` alongside `pll`, `detect`,
`source`. The three `pulse_train_*` tests carry along inline. The
`scripts/check-floats.sh` allowlist entry follows the file (same
exception class: synthetic-PCM through lawful Conn-inverse helpers).

**Kitchen-sink deletion (T3).** `crates/core/src/arb.rs` is gone,
`pub mod arb;` removed from `lib.rs`. The defensive
`_sample_rate_sealed` bridge dropped — each per-type arb file imports
its needed traits directly.

**CLAUDE.md update.** The proptest discipline section now codifies the
per-type colocation rule (was: "shared across crates live in `arb.rs`";
now: "colocated with the type they generate, no aggregating root
file"). Pattern matches upstream Rust `connections::prop::arb` and the
Haskell `Test/Data/Connection/{Float,Int,…}.hs` layout.

External-call surface change: `agogo_core::arb::pulse_train` →
`agogo_core::sync::pulse_train::pulse_train`. Internal `crate::arb::*`
sites move to per-type paths. The `testkit` feature flag stays;
external proptest consumers (none today) would now do
`agogo_core::time::grid::arb::arb_grid` instead of
`agogo_core::arb::arb_grid` — verbose but truthful.

## Test plan

- [x] `cargo test --workspace` — 950 + 17 + 17 + 1 doctest pass (unchanged)
- [x] `cargo clippy --all-targets -- -D warnings` — clean
- [x] `cargo fmt --all -- --check` — clean
- [x] `scripts/check-floats.sh` — clean (allowlist entry moved with `pulse_train`)
- [x] `cargo build -p agogo-core` (no default features) — confirms testkit gate is correct
- [x] `cargo build -p agogo-core --features testkit` — clean
