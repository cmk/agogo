# PR #22 — Plan 18: Bump connections + Grid lattice trait impls

## Summary

Bump `connections` to `d1ac1ea` and implement `Join`, `Meet`,
`Heyting`, `Coheyting` for `Grid`, replacing standalone functions
with trait method syntax across the workspace.

### What changed

- **Cargo.toml**: `connections` rev bumped to `d1ac1ea` (MR !8 —
  bi-Heyting trait methods + 55 property test functions).

- **grid.rs**: `PartialOrd` impl (divisibility preorder). `Join`,
  `Meet`, `Heyting`, `Coheyting` trait impls. Standalone functions
  (`meet`, `join`, `imply`, `neg`, `mid`, `coimp`, `coneg`, `comid`)
  removed. Provided defaults (`neg`, `mid`, `coneg`, `comid`) come
  from upstream — no local implementation.

- **Call site migration**: All `grid::meet(a, b)` → `a.meet(&b)`,
  `grid::neg(x)` → `x.neg()`, etc. across `dsl/eval.rs`, `dsl.rs`,
  `time/conn.rs`, and `time/grid.rs` tests.

### Why

The `connections` crate now defines the canonical lattice trait
hierarchy. Grid's algebra should implement those traits rather than
duplicating the operations as standalone functions.

## Local review (2026-04-25)

**Branch:** plan/2026-04-25-03
**Commits:** 3 (origin/main..plan/2026-04-25-03)
**Reviewer:** Claude (sonnet, independent)

---

### Commit Hygiene

Clean. Three commits with correct conventional prefixes, atomic scope.

### Code Quality

`PartialOrd` impl is consistent with `Eq` and `Ple` — structural
equality and divisibility preorder agree. All four trait impls
(`Join`, `Meet`, `Heyting`, `Coheyting`) match the removed standalone
functions exactly. Call-site migration complete — zero `grid::` calls
remain. Default methods (`neg`, `mid`, `coneg`, `comid`) come from
upstream with no local implementation needed.

### Test Coverage

All 40+ proptests migrate mechanically — semantic content unchanged.

### Risks

None. `PartialOrd` is additive (Grid didn't implement it before).
Rev bump is fully pinned in `Cargo.lock`.

### Recommendations

**Must fix before push:** None.

**Follow-up:**

1. Add a `partial_ord_agrees_with_ple` proptest linking the two
   orderings formally.
2. Restore the deleted comment in `eval.rs::forward_ref_errors`
   explaining what "forward reference" means.

<!-- gh-id: 4176612798 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-04-26 06:33 UTC](https://github.com/cmk/agogo/pull/22#pullrequestreview-4176612798))

## Pull request overview

This PR updates the workspace to the new `connections` lattice trait APIs and migrates `Grid` to implement those traits directly, replacing the prior standalone lattice-operation functions.

**Changes:**
- Bump the `connections` git revision and update `Cargo.lock` accordingly.
- Implement `PartialOrd`, `Join`, `Meet`, `Heyting`, and `Coheyting` for `Grid`, removing the standalone lattice functions.
- Migrate all call sites (DSL + time connection code + tests) from `grid::op(a,b)` to trait method syntax (`a.op(&b)`, `x.neg()`, etc.).

### Reviewed changes

Copilot reviewed 7 out of 8 changed files in this pull request and generated 2 comments.

<details>
<summary>Show a summary per file</summary>

| File | Description |
| ---- | ----------- |
| `Cargo.toml` | Updates the pinned `connections` git rev used across the workspace. |
| `Cargo.lock` | Records the resolved git source for the updated `connections` rev. |
| `crates/core/src/time/grid.rs` | Adds `PartialOrd` and lattice trait impls for `Grid`; updates tests to use trait methods. |
| `crates/core/src/time/conn.rs` | Migrates grid connection ceil/floor operations to `meet`/`join` trait methods. |
| `crates/core/src/dsl/eval.rs` | Migrates evaluator operations to lattice trait methods and updates unit tests. |
| `crates/core/src/dsl.rs` | Migrates DSL integration tests/proptests to lattice trait methods. |
| `doc/plans/plan-2026-04-25-03.md` | Adds a plan document describing the migration steps and verification checklist. |
| `doc/reviews/review-00022.md` | Adds a review record summarizing the change set, risks, and follow-ups. |
</details>






<!-- gh-id: 3143092544 -->
### Copilot on [`Cargo.toml:42`](https://github.com/cmk/agogo/pull/22#discussion_r3143092544) (2026-04-26 06:33 UTC)

nit: `rev = "d1ac1ea"` is a short git SHA. To keep the dependency pin unambiguous and consistent with the prior full-length rev, consider using the full commit hash here as well (Cargo.lock will still record it, but Cargo.toml is the source of truth when regenerating the lockfile).

<!-- gh-id: 3143092554 -->
### Copilot on [`crates/core/src/dsl/eval.rs:131`](https://github.com/cmk/agogo/pull/22#discussion_r3143092554) (2026-04-26 06:33 UTC)

nit: The explanatory comment describing what “forward reference” means was removed here; consider restoring it (or an equivalent brief note) so the intent of this test stays clear to future readers.
```suggestion
    fn forward_ref_errors() {
        // "Forward reference" here means referring to a positional
        // variable before it has been provided in the environment.
```

<!-- gh-id: 3143107481 -->
#### ↳ cmk ([2026-04-26 06:48 UTC](https://github.com/cmk/agogo/pull/22#discussion_r3143107481))

Fixed — expanded to full 40-char SHA.

<!-- gh-id: 3143107769 -->
#### ↳ cmk ([2026-04-26 06:48 UTC](https://github.com/cmk/agogo/pull/22#discussion_r3143107769))

Fixed — restored comment explaining strict monotonicity (channel N can only reference channels 1..N-1).
