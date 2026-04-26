# PR #19 — Plan 16: DSL parser + bi-Heyting algebra + CLI unification

## Summary

Polyrhythm DSL parser sprint (Plan 16) plus CLI surface cleanup.

### What changed

- **CLI rename**: `Channel.shift` → `Channel.delay`, `MAX_SHIFT` →
  `MAX_DELAY` throughout the workspace. `--ch` key `div` → `grid`,
  `shift-ms` → `delay`. Removed `offset-ms`, `swing`, `swing-res`
  from CLI (swing and offset now come from the DSL via `grid=`).

- **Bi-Heyting algebra** (T0b): Rename `heyting` → `imply`. Add
  `coimp`, `coneg`, `comid`, `mid` to `grid.rs`. 40 proptests ported
  from `Data.Lattice.Property` (h0–h17, c0–c20, s1) plus spot checks
  and non-Boolean witnesses.

- **DSL parser** (T1–T6): Hand-written recursive descent parser in
  `crates/core/src/dsl/` producing `TrackSpec` from grid algebra
  expressions. Modules: `ast.rs`, `error.rs`, `lexer.rs`, `parser.rs`,
  `eval.rs`, `display.rs`. Grammar: `&` (meet) > `|` (join) >
  `>`/`<` (imply/coimp), `!` (neg prefix), `~` (swing), `@` (offset).
  85 tests (lexer 16, parser 25, eval 11, display 13, integration 20).

- **`doc/designs/dsl.md`**: Updated to match implemented grammar.

- **`doc/plans/plan-2026-04-25-01.md`**: Full sprint plan.

### Why

The DSL is the user-facing syntax for agogo's rhythmic algebra. A thin
parser over the existing Grid lattice lets users compose subdivisions,
swing, and offsets in a single `--ch grid=...` expression instead of
separate key-value flags. The CLI cleanup (shift→delay, removing
redundant keys) aligns the surface with the new single-entry-point
design.
