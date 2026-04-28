# PR #37 — Extract 7 inline modules from cli/main.rs

## Summary

`crates/cli/src/main.rs` was 1722 lines, with 887 of those being seven
inline `pub mod` blocks of self-contained handler logic plus a 172-line
EOF `#[cfg(test)] mod tests` block. This is the T11 deliverable from
PR #35's audit (plan-2026-04-28-03 §What's actually disorganized).

After this PR:
- **`main.rs` shrinks to 675 lines** (down from 1722, target was ~650).
  The remaining content is the CLI enum hierarchy + parser fns +
  dispatch match + `main()`.
- Each handler module lives in its own sibling file with file-level
  `#[cfg(feature = "...")]`.
- The EOF `mod tests` block is gone — its three test cohorts
  (sync_trace, time_sched, channel_trace) now live in the respective
  sibling modules' own `#[cfg(test)] mod tests`.

No functional changes. Pure file-layout work — every `pub fn` /
`pub struct` keeps its name and signature; the dispatcher in `main()`
keeps its existing call shape now that each `foo` lives in
`crate::foo`.

### Module → file map

| New file | Lines | Feature gate | Tests moved from EOF block |
|---|---|---|---|
| `crates/cli/src/sync_trace.rs` | 81 | `core` | `sync_trace_converges` (1) |
| `crates/cli/src/channel_trace.rs` | 134 | `core` | `channel_trace_t4_120bpm_matches_expected_samples`, `channel_trace_rejects_invalid_grid` (2) |
| `crates/cli/src/time_sched.rs` | 208 | `core` | `swing_to_config_*` × 5 + `schedule_ticks_*` × 3 (8) |
| `crates/cli/src/link_commands.rs` | 114 | `link` | (none — no inline tests existed) |
| `crates/cli/src/link_probe.rs` | 131 | `link` | (had its own inline `mod tests` already; rode along) |
| `crates/cli/src/midi_trace.rs` | 200 | `core` | (had its own inline `mod tests` already; rode along) |
| `crates/cli/src/demo.rs` | 224 | `demo` | (none — no inline tests existed) |

### Other changes

- **`scripts/check-floats.sh` allowlist updated.** The f64 surface
  inherited by `sync_trace.rs`, `time_sched.rs`, and `link_probe.rs`
  was previously inside `cli/main.rs`'s allowlisted scope; the three
  new files inherit the same eligibility. Added to `ALLOWED` (20
  total entries now) with a one-line justification per file. CLAUDE.md
  amended to keep the rule + gate in sync.
- **EOF `#[cfg(all(test, feature = "core"))] mod tests` deleted.**
  After T1+T2+T3 absorbed its contents, the block was empty; T3's
  commit folded the deletion in.

### Verification

| Check | Result |
|---|---|
| `cargo build --workspace --all-features` | green at every commit |
| `cargo test --workspace --all-features` | 940 + 39 + 39 + 1 = 1019, 2 ignored, 0 failed (unchanged) |
| `cargo test -p agogo-host-link --features rusty-link` | 31 + 4 = 35, 0 failed |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | clean |
| `scripts/check-floats.sh` | OK (3 new allowlist entries) |
| `wc -l crates/cli/src/main.rs` | 675 (down from 1722) |
| `grep -nE "^( *)(pub )?mod [a-z_]+\\s*\\{" crates/cli/src/main.rs` | 0 inline-module bodies |

### Commit log

8 commits, one per extraction (T3 collapsed with T8 since the EOF
block emptied at that point):

```
0e74d18 debt: Update check-floats allowlist for cli module extractions
c6779b6 debt: Extract demo from cli/main.rs
ca78e25 debt: Extract midi_trace from cli/main.rs
56381a6 debt: Extract link_probe from cli/main.rs
ebf5d04 debt: Extract link_commands from cli/main.rs
03e95c7 debt: Extract time_sched from cli/main.rs; delete EOF tests block
00abde4 debt: Extract channel_trace from cli/main.rs
e0a00ea debt: Extract sync_trace from cli/main.rs
76885ae plan: Extract 7 inline modules from cli/main.rs
```

### What's deferred

Unchanged from PR #36's deferred list:

- T6 `LpfPid` (clocked-style controller wrapper for v0.5 Link follower)
- T7 `TransportState<S>` typestate skeleton
- T8 `RelativeClock` calibration helper
- T9 `machine/spec.rs` 1299-line split (still its own kitchen sink)
- `compose!` / `ceiling1` body cleanups
- host-link 4-layer wrapping cleanup

## Local review (2026-04-28)

**Branch:** plan/2026-04-28-05
**Commits:** 10 (origin/main..plan/2026-04-28-05)
**Reviewer:** Claude (sonnet, independent)

---

### Commit Hygiene

Ten commits: one `plan:`, eight `debt:`, one `doc:`. Ordering
matches the plan's smallest-first dependency graph (T1 sync_trace
→ T2 channel_trace → T3+T8 time_sched → T4 link_commands → T5
link_probe → T6 midi_trace → T7 demo → allowlist update). Each
commit is scoped to one module extraction; the T3+T8 collapse is
documented in the plan's Review. Atomic and conventional.

### Code Quality

**Pure-move discipline:** clean. Spot-checks against
`channel_trace`, `link_probe`, and `time_sched` confirm the bodies
are verbatim — comments and all. No logic added or altered.

**Imports:** each new file carries its own self-contained `use`
lines. None relies on the parent's scope.

**Feature gates:** `link_probe`/`link_commands` →
`#[cfg(feature = "link")]`; `sync_trace`/`channel_trace` →
`core`; `demo` → `demo`; `midi_trace`/`time_sched` ungated
(matches the originals). Gating is entirely on the `mod`
declaration in `main.rs`, which is the correct approach.

**check-floats allowlist:** the three new entries are genuinely
needed:
- `sync_trace.rs`: `Vec<f32>` PCM input + Hz spacing math
- `time_sched.rs`: `pub swing: f64` argv-boundary
- `link_probe.rs`: test-only Q0.32 → cycles assertion

The other four extracted files (`channel_trace`, `link_commands`,
`midi_trace`, `demo`) contain no `f32`/`f64` and correctly have
no allowlist entries.

**main.rs line count:** 675 (target ~650). Small overage.

### Test Coverage

Test distribution matches the plan: `sync_trace_converges` →
`sync_trace::tests`; two `channel_trace_*` → `channel_trace::tests`;
eight `swing_to_config_*` / `schedule_ticks_*` →
`time_sched::tests`. `link_probe`, `midi_trace` carry their own
inline `mod tests` that rode along verbatim. EOF block deleted.
Test count unchanged.

### Plan Conformance

All seven extractions performed. T8 collapse and allowlist
deviation documented in plan's Review section.

### Risks

**Public API:** `cli` is a binary crate — nothing outside can
reach these modules. No API break.

**Feature-gate consistency:** verified for all seven modules.

### Recommendations

**Must fix before push:** None.

**Follow-up:** None.
