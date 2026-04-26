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

## Local review (2026-04-25)

**Branch:** plan/2026-04-25-01
**Commits:** 9 (origin/main..plan/2026-04-25-01)
**Reviewer:** Claude (sonnet, independent)

---

### Commit Hygiene

All commits use conventional prefixes correctly. No unrelated changes
mixed. Commit order follows the dependency graph. Clean.

### Code Quality

**Fixed (must-fix):** Two parser tests (`trailing_garbage`,
`invalid_atom`) used `matches!()` without `assert!()` — vacuously
passing. Fixed in commit `0768773`.

**Fixed (must-fix):** `display_parse_round_trip` proptest only
exercised `Meet` nodes. Extended with `arb_expr()` strategy covering
all six `Expr` variants + nesting. Fixed in commit `0768773`.

Display precedence/parenthesization logic reviewed and confirmed
correct — left-associative right-child prec+1 pattern is sound for
all operators.

`ChannelSpec.grid` is still `Grid` (not `TrackSpec`). The DSL parser
exists but is not yet wired into `ChannelSpec::parse`. This is an
intentional deferral — see Follow-up below.

### Test Coverage

All planned verification properties are present:
- `parser_never_panics`, `tokenize_never_panics` ✓
- `display_parse_round_trip` (now with full `arb_expr`) ✓
- `single_atom_round_trips` (all 36) ✓
- `eval_preserves_{meet,join,imply,coimp,neg}` ✓
- Commutativity, associativity, distributivity ✓
- h0–h17, c0–c20, s1 lattice properties ✓

**Added:** Nesting depth limit test (33 `!`s → `NestingTooDeep`).
**Added:** `comid(T16)` spot check.

Lexer `tokenize_never_panics` uses `".*"` which includes multi-byte
UTF-8. Non-ASCII bytes hit the `UnexpectedChar` arm via `bytes[pos]
as char` — well-defined (no panic, no UB) but may produce misleading
error messages for non-ASCII input. Low severity.

### Plan Conformance

T0a (upstream connections traits): correctly deferred.
T0b: all ops implemented, all 40 proptests + spot checks present.
T1–T6: implemented as planned. `PolyAst` removed (single-track
design — `&` is now meet, not a polyrhythm separator).
Lexer returns `Ok(vec![])` on empty input (plan said `Err`); empty
detection handled in `parse_tokens`. Correct behavior, minor plan
deviation.

### Risks

No TODOs or stubs. No new dependencies. No security concerns.

### Recommendations

**Must fix before push:** All resolved in commit `0768773`.

**Follow-up (future work):**

1. Wire `dsl::parse()` into `ChannelSpec::parse` so `--ch grid=T16~T16:80`
   accepts full DSL expressions (currently only plain grid names).
2. Improve lexer UTF-8 handling — use `chars()` iterator instead of
   byte indexing to produce correct `UnexpectedChar` errors for
   non-ASCII input.
3. Plan doc references `^` for meet in some task descriptions (stale
   from before the `&` swap). Cosmetic; does not affect implementation.
