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
  - Offset: `@` in ticks (musical, in the DSL)
  - Delay compensation: renamed from `shift`, lives in `--ch` spec
    (not in the DSL)

- **Code rename**: `Channel.shift` → `Channel.delay`, `MAX_SHIFT` →
  `MAX_DELAY` throughout the workspace. CLI flag `shift-ms` → `delay`.
  `offset-ms` removed from CLI (offset now comes from DSL `@<ticks>`).

### Why

The DSL is the user-facing syntax for agogo's rhythmic algebra. This
PR lands the design and renames the delay compensation field for
clarity before implementation begins.
