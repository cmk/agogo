# PR #49 — stdio-core integration control and observation

## Summary

See `doc/plans/plan-2026-04-30-01.md` and
`doc/plans/plan-2026-04-30-02.md` for the current sprint context.

## Local review (2026-04-30)

**Branch:** `plan-2026-04-30-01`  
**Reviewer:** Codex reviewer

---

The new stdio tool handler can panic on out-of-range user input
rather than returning an error, which can destabilize the adapter. The
rest of the scaffold and tests appear consistent with the stated plan.

### Findings

- **[P2] Return an error instead of panicking for large BPM** —
  `crates/stdio/src/driver.rs:170`

  When a stdio client calls `agogo.tempo.set` with an integer BPM
  above 4294, `parse_u32_field` accepts it but
  `Tempo::from_bpm_integer` panics on overflow. That turns malformed
  tool input into a process unwind instead of the same kind of tool
  error returned for other invalid fields, so validate the tempo range
  before constructing `Tempo`.

<!-- gh-id: 3167623074 -->
### Copilot on [`crates/stdio/src/driver.rs:171`](https://github.com/cmk/agogo/pull/49#discussion_r3167623074) (2026-04-30 11:39 UTC)

`parse_integer_bpm` hard-codes the max integer BPM (4294). Since this limit comes from `Tempo`’s internal scaling/representation, consider centralizing it (e.g., a `Tempo::MAX_BPM_INTEGER` constant or helper) to avoid future divergence if the scaling changes.


<!-- gh-id: 3167623094 -->
### Copilot on [`crates/stdio/src/rt_bridge.rs:107`](https://github.com/cmk/agogo/pull/49#discussion_r3167623094) (2026-04-30 11:39 UTC)

`tempo_raw` is used for cross-thread handoff (async tool call -> audio callback) but uses `Ordering::Relaxed` on the store. With relaxed ordering there’s no synchronization and the audio thread is allowed to observe stale values even when the write happens-before the next buffer in wall-clock time, which undermines the "applies_by next_buffer" contract. Consider using `Release` on `set_tempo` (and matching `Acquire` on loads) to establish a proper handoff.


<!-- gh-id: 3167623111 -->
### Copilot on [`crates/stdio/src/rt_bridge.rs:126`](https://github.com/cmk/agogo/pull/49#discussion_r3167623111) (2026-04-30 11:39 UTC)

The audio-thread snapshot load of `tempo_raw` also uses `Ordering::Relaxed`. If `set_tempo` is updated to use `Release`, this should typically be `Acquire` (or stronger) so the read participates in the synchronization and avoids seeing stale tempo values longer than intended.

<!-- gh-id: 4204772572 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-04-30 11:39 UTC](https://github.com/cmk/agogo/pull/49#pullrequestreview-4204772572))

## Pull request overview

Introduces an `agogo-stdio` workspace crate to scaffold stdio-core integration with an RT-safe control bridge (atomics for tempo + `rtrb` SPSC for ordered commands), and updates roadmap/docs to align v0.3/v0.4 contracts with the corresponding stdio-core plans (including the BPM overflow rejection noted in the local review).

**Changes:**
- Added `crates/stdio` (`agogo-stdio`) with an RT-safe async→audio control bridge and driver-shaped tool routing for initial agogo tools.
- Added/updated plan and review documentation for control (Plan 2026-04-30-01) and observation (Plan 2026-04-30-02), and aligned v0.3/v0.4 version docs to stdio-core contracts.
- Wired the new crate into the workspace and dependencies (`rtrb`).

### Reviewed changes

Copilot reviewed 10 out of 11 changed files in this pull request and generated 5 comments.

<details>
<summary>Show a summary per file</summary>

| File | Description |
| ---- | ----------- |
| doc/versions/version-0.4.md | Aligns v0.4 roadmap/acceptance properties with stdio-core Plan 11 agogo snapshot contract. |
| doc/versions/version-0.3.md | Aligns v0.3 roadmap/acceptance properties with stdio-core Plan 10 lifecycle/control contract. |
| doc/reviews/review-00049.md | Records local review finding about BPM overflow panic and expected fix direction. |
| doc/plans/plan-2026-04-30-02.md | Adds the v0.4 snapshot observation plan (schema, RT write, async publish, drop/seq tests). |
| doc/plans/plan-2026-04-30-01.md | Adds the v0.3 control adapter plan (RT bridge, lifecycle/tool routing, tests, docs alignment). |
| crates/stdio/src/rt_bridge.rs | Implements the atomics + `rtrb` SPSC RT-safe control bridge with unit tests. |
| crates/stdio/src/lib.rs | Defines the new crate surface and re-exports driver/bridge types. |
| crates/stdio/src/driver.rs | Implements driver-shaped tool routing, mounting gate, parsing, inverse-op scaffolding, and tests (incl. BPM cap). |
| crates/stdio/Cargo.toml | Adds package metadata and dependencies for `agogo-stdio`. |
| Cargo.toml | Adds `crates/stdio` as a workspace member and `rtrb` as a workspace dependency. |
| Cargo.lock | Locks `agogo-stdio` and `rtrb` dependency resolution. |
</details>






<!-- gh-id: 3167623125 -->
### Copilot on [`crates/stdio/src/driver.rs:181`](https://github.com/cmk/agogo/pull/49#discussion_r3167623125) (2026-04-30 11:39 UTC)

`parse_u32_field` returns "missing integer field" both when the field is absent and when it exists but is the wrong JSON type (e.g., string/float/negative). This makes tool errors misleading for callers. Consider distinguishing "missing" vs "expected unsigned integer" (or wording like "missing or non-integer field").

<!-- gh-id: 3167623143 -->
### Copilot on [`crates/stdio/src/driver.rs:145`](https://github.com/cmk/agogo/pull/49#discussion_r3167623143) (2026-04-30 11:39 UTC)

`inverse_op` for `TempoSet` trusts `prior_bpm` from `args` and returns an inverse tool call even if `prior_bpm` is out of the representable `Tempo` range. That can produce an inverse that is guaranteed to fail when applied (breaking the round-trip/undo contract). Consider validating `prior_bpm` (<= 4294 and within `u32`) before returning `Some(...)`, otherwise return `None`.


<!-- gh-id: 3167672776 -->
#### ↳ cmk ([2026-04-30 11:49 UTC](https://github.com/cmk/agogo/pull/49#discussion_r3167672776))

Done in fdb6911. The max integer BPM is now centralized as Tempo::MAX_BPM_INTEGER and the stdio parser uses that constant instead of a local literal.

<!-- gh-id: 3167673657 -->
#### ↳ cmk ([2026-04-30 11:49 UTC](https://github.com/cmk/agogo/pull/49#discussion_r3167673657))

Done in fdb6911. set_tempo now stores tempo_raw with Release ordering so the async-to-audio handoff participates in synchronization.

<!-- gh-id: 3167674405 -->
#### ↳ cmk ([2026-04-30 11:49 UTC](https://github.com/cmk/agogo/pull/49#discussion_r3167674405))

Done in fdb6911. The audio-side tempo loads in tempo() and snapshot() now use Acquire ordering to match the Release store.

<!-- gh-id: 3167675718 -->
#### ↳ cmk ([2026-04-30 11:50 UTC](https://github.com/cmk/agogo/pull/49#discussion_r3167675718))

Done in fdb6911. parse_u32_field now distinguishes a missing field from a present field with the wrong JSON type, and there is a regression test for the non-integer bpm error.

<!-- gh-id: 3167676661 -->
#### ↳ cmk ([2026-04-30 11:50 UTC](https://github.com/cmk/agogo/pull/49#discussion_r3167676661))

Done in fdb6911. inverse_op now validates prior_bpm through the same representable tempo range and returns None for out-of-range values, with a regression test covering that case.
