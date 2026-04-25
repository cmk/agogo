# PR #18 — Plan 16: Polyrhythm DSL parser design

## Summary

Land the plan document and updated DSL design doc for Plan 16 — the
polyrhythm DSL parser sprint.

### What changed

- **`doc/plans/plan-2026-04-25-01.md`**: Full sprint plan covering:
  - T0a: Upstream `connections` crate — flesh out `Heyting`/`Coheyting`
    traits with provided methods (`neg`/`mid`, `coneg`/`comid`),
    rename `Coheyting::sub` → `coimp`, port 40 property tests from
    `Data.Lattice.Property`
  - T0b: `impl Heyting + Coheyting for Grid`, instantiate upstream
    properties with `arb_grid()`
  - T1–T6: Hand-written recursive descent parser producing
    `Vec<TrackSpec>` from DSL strings, with AST, lexer, evaluator,
    Display round-trip, and proptest hardening

- **`doc/designs/dsl.md`**: Updated to reflect settled design decisions:
  - Operator swap: `&` = polyrhythm (loosest), `|` = join, `^` = meet
  - New operators: `!` (neg), `>` (imp), `<` (coimp)
  - Precedence: `!` > `^` > `|` > `>/<` > `&`
  - Swing syntax: `~TBase:amount` (explicit resolution)
  - Modifier units: `@` in ticks (musical), `+` in ms (physical)

### Why

The DSL is the user-facing syntax for agogo's rhythmic algebra. This
docs-only PR lands the design before implementation begins, so the
plan can be reviewed independently of code changes.
