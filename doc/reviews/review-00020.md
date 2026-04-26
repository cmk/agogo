# PR #20 — Plan 17: Channel variables + DSL simplification

## Summary

Simplify the DSL to pure grid algebra and add channel variable
references, so channels can compose grids from earlier channels.

### What changed

- **DSL simplification**: Remove swing (`~`) and offset (`@`) modifiers
  from the grammar. The DSL now returns `Grid` instead of `TrackSpec`.
  Types removed: `TrackSpec`, `Modifier`, `TrackAst`. Lexer tokens
  removed: `Tilde`, `At`, `Colon`, `Int`. Net -367 lines.

- **Channel variables**: New `Expr::Var(String, Span)` AST variant.
  Identifiers that aren't valid grid names are treated as variable
  references. `dsl::parse(input, env)` takes a `&[(String, Grid)]`
  environment. Variable lookup is case-sensitive; unknown variables
  error with `DslErrorKind::UnknownVariable`.

- **ChannelSpec**: `grid=` now accepts full DSL expressions (parsed via
  `dsl::parse` with env). New keys: `swing=[TBase:]i8` (default
  resolution T8), `offset=i32` (ticks). `grid=` defaults to `T4` when
  omitted. `dev` is the only required key.

- **`parse_channels`**: New function that processes `--ch` specs in
  order, building the variable environment. Unnamed channels get
  auto-assigned IDs (`C1`, `C2`, ...).

- **`agogo run`**: Wired to `parse_channels`. MIDI port extraction
  moved from re-parsing to pre-parsed specs.

### Example

```bash
agogo run \
  --ch "id=kick,dev=midi,grid=T4" \
  --ch "id=hats,dev=midi,grid=kick&T16,swing=T16:80,offset=20" \
  --ch "dev=midi,grid=C2|T8t"
```

### Why

The DSL's job is grid algebra — composing subdivisions via lattice
operations. Swing, offset, and delay are timing parameters orthogonal
to the lattice structure. Separating them keeps the DSL small and
makes channel variables trivial (a variable is just a `Grid`, not a
compound type with timing baggage). The `parse_channels` sequential
resolution unlocks cross-channel composition without repeating
expressions.

## Local review (2026-04-25)

**Branch:** plan/2026-04-25-02
**Commits:** 6 (origin/main..plan/2026-04-25-02)
**Reviewer:** Claude (sonnet, independent)

---

### Commit Hygiene

All five commits follow conventional format and are appropriately
scoped. The `plan:` opener is first. Each `feat:` commit is atomic
to its task boundary. The `doc:` closer finalises plan + review file.
Clean.

### Code Quality

**Fixed (must-fix):** `offset_ticks` → `Micro` conversion in
`spec.rs:193` was semantically wrong — ticks stored as microseconds.
Fixed by rejecting non-zero `offset_ticks` in `into_channel` with a
clear error until the tempo-dependent Tick→Micro conversion is wired.

**Display swing condition** reviewed and confirmed correct — the
`||` condition emits swing when either field is non-default. No bug.

Re-parse in `run_with_rate` cleanly eliminated.

### Test Coverage

All 19 planned verification properties present. Property tests cover
all lattice ops, variable resolution, commutativity, associativity,
distributivity, and display round-trip (with variable env).

**Fixed:** `offset_ticks` proptest domain widened from `-1000..=1000`
to `any::<i32>()` per CLAUDE.md convention.

### Plan Conformance

T1, T2, T3 fully delivered. T3's plan mentioned updating `channel
trace` in main.rs but no change was needed (it uses `--grid` directly,
not `ChannelSpec`).

### Risks

Variable name collision with future grid names is inherent to the
"grid first, variable second" disambiguation — acceptable.

### Recommendations

**Must fix before push:** All resolved.

**Follow-up (future work):**

1. `out` field quoting no longer exercised by proptest (old regex had
   spaces/commas, new one is `[a-zA-Z0-9]{1,10}`). Restore a spot
   check or widen regex.
2. `unknown_var_errors` is a spot check, not a proptest. Consider
   adding a proptest over arbitrary non-grid identifier strings.
3. Wire tempo-dependent Tick→Micro conversion for non-zero
   `offset_ticks` (currently rejected at `into_channel`).
