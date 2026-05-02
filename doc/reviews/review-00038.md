# PR #38 — Split channel/spec.rs into 5 single-concern files

## Summary

`crates/core/src/channel/spec.rs` was 1325 lines doing five jobs in
one file (struct definition, error enum, parser, display, validation).
This is the T9 deliverable from PR #35's audit — same kitchen-sink
demolition that PR #37 did for `cli/main.rs`.

After this PR:

- **Parent `crates/core/src/channel/spec.rs` shrinks to 39 lines** —
  module-level doc + `pub mod` declarations + `pub use` re-exports.
- Each concern lives in its own file under `crates/core/src/channel/spec/`:

| File | Lines | Contents |
|---|---|---|
| `spec.rs` (parent) | 39 | doc + `pub mod` + `pub use` |
| `spec/types.rs` | 76 | `ChannelSpec` struct + `snap_intent` accessor |
| `spec/error.rs` | 19 | `ChannelSpecError` enum |
| `spec/parser.rs` | 868 | `ChannelSpec::parse`, `parse_channels`, `parse_swing`, `micro_from_user_ms`, `tokenize` + ~40 parse tests |
| `spec/display.rs` | 292 | `Display for ChannelSpec` + `quote_if_needed` + 6 spot tests + `arb_*` strategies + `spec_round_trip` proptest |
| `spec/validate.rs` | 131 | `into_channel` + 5 spot tests + `snap_intent_round_trips_through_spec` proptest |

**Tests stay colocated with their module:** `parse_*` tests in
`parser::tests`, `display_*` tests + `arb_*` helpers + `spec_round_trip`
in `display::tests`, `into_channel_*` and `snap_intent_*` in
`validate::tests`.

No functional changes. Pure file-layout work — every `pub fn` /
`pub struct` keeps its name and signature; the parent
`spec.rs`'s `pub use` re-exports keep external callers
(`crate::control::ChannelSpec` etc., re-exported again from
`control/transport.rs`) resolving without changes.

### Other changes

- **`scripts/check-floats.sh` allowlist updated.** The
  `channel/spec.rs` entry (which guarded `micro_from_user_ms`'s
  argv-boundary `f64`) swaps for `channel/spec/parser.rs` —
  same `delay=ms` argv boundary, just in the parser submodule
  now. Total count stays at 20. CLAUDE.md amended.

### Commit log

6 commits, smallest-first per the plan's dependency graph (T3+T6
collapsed since after T1+T2+T4+T5 the parent shell was already
within reach):

```
29ec30a debt: Extract parser to channel/spec/parser.rs; collapse spec.rs to 39-line shell
b64318e debt: Extract Display + round-trip proptest to channel/spec/display.rs
14157bf debt: Extract into_channel + tests to channel/spec/validate.rs
83101f3 debt: Extract ChannelSpec struct + snap_intent from channel/spec.rs
cedc208 debt: Extract ChannelSpecError from channel/spec.rs
462cc15 plan: Split channel/spec.rs into 5 single-concern files
```

### Verification

| Check | Result |
|---|---|
| `cargo build --workspace --all-features` | green at every commit |
| `cargo test --workspace --all-features` | 940 + 39 + 39 + 1 = 1019 (unchanged) |
| `cargo test -p agogo-host-link --features rusty-link` | 31 + 4 = 35 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | clean |
| `scripts/check-floats.sh` | OK (allowlist swap, same total) |
| `wc -l crates/core/src/channel/spec.rs` | 39 (down from 1325) |

### What's deferred

Unchanged from PR #37's deferred list:

- T6 `LpfPid` (substantive Link-follower controller)
- T7 `TransportState<S>` typestate skeleton
- T8 `RelativeClock` calibration helper
- `compose!` / `ceiling1` body cleanups
- host-link 4-layer wrapping cleanup
- `cargo build -p agogo-cli --no-default-features` build break
- `link_probe::tests` LAN-peer assertion brittleness

## Local review (2026-04-28)

**Branch:** plan/2026-04-28-06
**Commits:** 7 (origin/main..plan/2026-04-28-06)
**Reviewer:** Claude (sonnet, independent)

---

### Commit Hygiene

All seven commits use conventional prefixes (`plan:` opener, five
`debt:` extractions, one `doc:` finalize). Subjects under 72
characters, atomic, and each commit leaves the tree green
(`cargo test --workspace --all-features` + clippy + check-floats).
The T3+T6 collapse is documented in the plan's Review section.

### Code Quality

Modern module layout — five siblings under `crates/core/src/channel/spec/`,
no `mod.rs`. Public API surface preserved through `pub use` re-exports
in the 39-line parent shell; `crates/cli/src/run.rs` and other callers
need no import changes.

### Test Coverage

All ~50 tests redistributed to colocated `#[cfg(test)] mod tests`
blocks per the plan's §Notes on test redistribution. Spot-checked:

- `parse_rejects_negative_delay` → `parser::tests` ✓
- `into_channel_clamps_delay` → `validate::tests` ✓
- `snap_intent_*` (both, including the proptest) → `validate::tests` ✓
- `arb_*` strategies + `spec_round_trip` proptest → `display::tests` ✓

**Proptest regression seed migrated.** The reviewer caught that
`crates/core/proptest-regressions/machine/spec.txt` (one saved
seed for `spec_round_trip`) would be orphaned by the test move —
proptest derives the regression file path from the test source
file location, so post-split it would look at
`channel/spec/display.txt`, not `machine/spec.txt`. Fixed in the
T4 fixup: `git mv` the seed to the new path so the regression
safety net is preserved. MEMORY.md flags this as a contract
violation we've hit before; nice catch.

### Plan Conformance

Line-count actuals vs. plan estimates:

| File | Estimate | Actual |
|---|---|---|
| `spec.rs` parent | ~30 | 39 |
| `types.rs` | ~70 | 76 |
| `error.rs` | ~20 | 19 |
| `parser.rs` | ~600 | 868 |
| `display.rs` | ~200 | 292 |
| `validate.rs` | ~120 | 131 |

Parser overage attributable to test bulk; the file boundary
matches the plan's intent. Parent at 39 (vs ~30 target) — the
overage is the module-level doc comment which must stay in the
parent per Rust convention.

`scripts/check-floats.sh` allowlist swap (`channel/spec.rs` →
`channel/spec/parser.rs`) is a clean one-for-one — total stays at
20. CLAUDE.md amended with the parenthetical noting the Plan
2026-04-28-06 T3 swap. Both correct and consistent.

### Risks

No TODOs, stubs, or behavioral drift. Pure file-layout work — every
`pub fn` / `pub struct` keeps its name and signature.

### Recommendations

**Must fix before push:** none (the proptest regression rename was
folded into the T4 fixup commit; the truncated-comment + blank-line
nits in `parser.rs` were folded into the T3 fixup commit — both
collapsed via `scripts/autosquash.sh`).

**Follow-up (future work):**
- `arb_spec` delay generator bound `[0, 300] ms` is a proptest
  anti-pattern carried forward from before this PR (CLAUDE.md
  proptest rule: don't bound generators to keep arithmetic safe).
  The delay field is `Micro(i64)` — a wider generator would
  exercise `into_channel`'s `MAX_DELAY` clamp at the
  Display/parse round-trip boundary. Pre-existing, deferred here.
