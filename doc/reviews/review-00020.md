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
