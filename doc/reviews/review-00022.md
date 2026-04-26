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
