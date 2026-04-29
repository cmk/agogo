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

## Local review (2026-04-28)

**Branch:** plan/2026-04-28-08
**Commits:** 3 (origin/main..plan/2026-04-28-08)
**Reviewer:** Claude (sonnet, independent)

---

### Commit Hygiene

All three commits carry correct prefixes (`plan:`, `debt:`, `doc:`). Landing T1+T2+T3 in a single `debt:` commit is justified — the tasks are strongly interdependent (T3 cannot exist without T1+T2, and splitting them would leave a state where `crate::arb` has holes). The merge would be temporarily broken if split. Atomic landing is the right call.

### Code Quality

**Modern module layout** — all 9 new files follow `<type>.rs` + `<type>/arb.rs` sibling shape. No `mod.rs` files introduced. Correct.

**`#[cfg(any(test, feature = "testkit"))]` gate** — every new `time/*/arb.rs` file is gated at the module declaration site in the parent `.rs` file. Correct.

**`pulse_train.rs` is NOT testkit-gated** — `pub mod pulse_train;` in `sync.rs` carries no `cfg` gate. Correct; `cli/sync_trace` is a runtime consumer.

**`_sample_rate_sealed` bridge** dropped cleanly — each per-type `arb.rs` imports what it needs directly and is only compiled under test/testkit.

**Doc strings** — the 9 new `arb.rs` files each carry a module-level doc comment explaining the gate. Consistent, not boilerplate-padded. Intra-doc links spot-checked: `crate::time::conn::quantize_at`, `crate::time::tick::from_ticks`, `crate::time::conn::TICKTIME`, `crate::time::swing::SwingConfig` — all resolvable.

**`sync/source.rs` qualified-path usage** — both `crate::arb::pulse_train::<S048>` raw-qualified-path calls (not in `use` statements) updated to `crate::sync::pulse_train::pulse_train::<S048>`. Caught by the build, fully resolved.

### Test Coverage

**Three `arb_*_in_range` tests** all migrated correctly:
- `arb_bpm_in_range` → `time/tempo/arb.rs`
- `arb_sample_rate_is_standard` → `time/sample/arb.rs`
- `arb_jitter_in_range` → `time/decimal/arb.rs`

**Three `pulse_train_*` tests** all in `sync/pulse_train.rs`. Net `#[test]` count change: 0 (6 added, 6 removed from deleted `arb.rs`).

### Plan Conformance

**T1** — all 8 strategy migrations completed. `arb_integer_stc` deviation documented in the Review section.

**T2** — `pulse_train` + `PULSE_WIDTH_PS` moved, all import sites updated (pll, detect, source, sync_trace). Complete.

**T3** — `arb.rs` deleted, `pub mod arb;` removed from `lib.rs`, `_sample_rate_sealed` dropped. Complete.

**Plan doc inconsistency — stale Critical Files section** *(confidence: 82)*. The Critical Files section still lists `crates/core/src/sync/sample_tick.rs` as getting a `pub mod arb;` declaration and `sync/sample_tick/arb.rs` as a new file. The `sample_tick/arb.rs` was never created (documented deviation in the Review section, but the Critical Files list wasn't pruned to match). One-line edit to fix.

### Risks

**`testkit` feature external caller audit** — no `--features testkit` usage in cli/host-link/host-cpal. Only external `agogo_core::arb::*` site was `cli/src/sync_trace.rs`'s `pulse_train` use, handled in T2. Risk: none realised.

**CLAUDE.md wording vs. implementation** — new rule example (`time/grid.rs` declares the mod; `time/grid/arb.rs` holds `arb_grid`) matches the diff exactly.

**`check-floats.sh` allowlist** — old `crates/core/src/arb.rs` entry removed from both the ALLOWED array AND the comment header. New `crates/core/src/sync/pulse_train.rs` added to both. Symmetric.

**`arb_bpm_in_range` bound** — assertion upper bound is inclusive 400_000_000 but the strategy's widest arm stops at 399_999_999 (exclusive range). Inherited from old `arb.rs` unchanged; no regression.

### Recommendations

**Must fix before push:** none.

**Follow-up (future work):**
- `agogo-testkit` re-export crate when external proptest consumers materialize — verbose per-type paths will be ergonomically painful then. Deferred per plan; nothing to do now.
- `time/conn/arb.rs` doc references `super::Whole`. If `Whole` is ever renamed/removed in `conn.rs`, the intra-doc link becomes a rustdoc warning. Low priority.
