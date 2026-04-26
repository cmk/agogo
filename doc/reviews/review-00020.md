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

All six commits follow conventional format and are appropriately
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

<!-- gh-id: 3142980195 -->
### Copilot on [`crates/core/src/machine/spec.rs:233`](https://github.com/cmk/agogo/pull/20#discussion_r3142980195) (2026-04-26 04:29 UTC)

`parse_channels` allows duplicate channel IDs. Because variable lookup scans `env` linearly and returns the first match, a later channel reusing an earlier ID will silently resolve to the older grid, which is very likely unintended. Consider detecting duplicate IDs here and returning a clear error (or define/implement "last one wins" by updating the existing entry).

<!-- gh-id: 3142980198 -->
### Copilot on [`crates/core/src/machine/spec.rs:230`](https://github.com/cmk/agogo/pull/20#discussion_r3142980198) (2026-04-26 04:29 UTC)

`parse_channels` should likely reject IDs that are valid grid literals (e.g. `id=T16`). The DSL always interprets valid grid names as `Grid` atoms before considering variables, so such IDs can never be referenced and will lead to confusing "works when defined, can't be used" behavior.
```suggestion
        let explicit_id = spec.id.clone();
        let id = explicit_id
            .clone()
            .unwrap_or_else(|| format!("C{}", i + 1));
        if explicit_id.is_some() && id.parse::<Grid>().is_ok() {
            return Err(ChannelSpecError::BadValue(
                "id",
                format!(
                    "`{}` is a grid literal and cannot be used as a channel ID",
                    id
                ),
            ));
        }
```

<!-- gh-id: 3142980207 -->
### Copilot on [`crates/cli/src/run.rs:107`](https://github.com/cmk/agogo/pull/20#discussion_r3142980207) (2026-04-26 04:29 UTC)

Error reporting lost per-entry context: `parse_channels(&args.ch).map_err(|e| format!("--ch: {e}"))?` no longer indicates which `--ch` spec failed. Consider including the channel index and/or the original spec text in the error so users can quickly locate the problematic entry.
```suggestion
    let named = match agogo_core::machine::parse_channels(&args.ch) {
        Ok(named) => named,
        Err(e) => {
            if let Some((idx, spec, entry_err)) = args.ch
                .iter()
                .enumerate()
                .find_map(|(idx, spec)| {
                    agogo_core::machine::parse_channels(&args.ch[..=idx])
                        .err()
                        .map(|entry_err| (idx, spec, entry_err))
                })
            {
                return Err(format!("--ch[{}] `{}`: {entry_err}", idx + 1, spec));
            }
            return Err(format!("--ch: {e}"));
        }
    };
```

<!-- gh-id: 3142980211 -->
### Copilot on [`crates/cli/src/run.rs:127`](https://github.com/cmk/agogo/pull/20#discussion_r3142980211) (2026-04-26 04:29 UTC)

Similarly, errors from `spec.into_channel()` are surfaced as `--ch: {e}` without identifying which channel/spec caused the failure. Consider attaching the parsed ID (from `named`) or the spec index when formatting this error.
```suggestion
        .map(|(id, spec)| {
            spec.into_channel()
                .map_err(|e| format!("--ch {id}: {e}"))
        })
```

<!-- gh-id: 3142980212 -->
### Copilot on [`crates/core/src/machine/spec.rs:612`](https://github.com/cmk/agogo/pull/20#discussion_r3142980212) (2026-04-26 04:29 UTC)

The proptest `out` generator (`[a-zA-Z0-9]{1,10}`) no longer produces values requiring quotes (spaces/commas/`=`), and the earlier spot tests for quoted `out` values were removed. This reduces coverage of the tokenizer + `quote_if_needed` round-trip for realistic MIDI port names; consider restoring a spot check or widening the `out` strategy to include separators/whitespace.

<!-- gh-id: 3142980215 -->
### Copilot on [`crates/core/src/dsl/error.rs:31`](https://github.com/cmk/agogo/pull/20#discussion_r3142980215) (2026-04-26 04:29 UTC)

`DslErrorKind::InvalidAtom` looks unreachable now: the parser no longer errors on invalid grid-like idents (it falls back to `Expr::Var`), so inputs like `T3` will surface as `UnknownVariable` instead. Consider either removing `InvalidAtom` (and updating docs/tests accordingly) or reintroducing a parse-time `InvalidAtom` path for grid-looking identifiers to keep error reporting clear.

<!-- gh-id: 4176521533 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-04-26 04:29 UTC](https://github.com/cmk/agogo/pull/20#pullrequestreview-4176521533))

## Pull request overview

This PR simplifies Agogo’s polyrhythm DSL down to pure grid algebra and adds channel-variable references so later `--ch` entries can compose grids from earlier channels, while moving swing/offset back into `ChannelSpec`.

**Changes:**
- Simplifies the DSL grammar and AST so `dsl::parse(input, env)` evaluates directly to `Grid`, with variables resolved from a channel environment.
- Extends `ChannelSpec` parsing to accept full DSL expressions in `grid=`, adds `swing=` and `offset=`, and introduces `parse_channels` for sequential `--ch` resolution.
- Updates `agogo run` to use `parse_channels` and avoid re-parsing specs for MIDI port selection.

### Reviewed changes

Copilot reviewed 12 out of 12 changed files in this pull request and generated 7 comments.

<details>
<summary>Show a summary per file</summary>

| File | Description |
| ---- | ----------- |
| doc/reviews/review-00020.md | Adds a local review record for Plan 17 / PR #20. |
| doc/plans/plan-2026-04-25-02.md | Adds the implementation plan and verification matrix for Plan 17. |
| crates/core/src/machine/spec.rs | Implements env-aware `ChannelSpec::parse`, adds swing/offset, and introduces `parse_channels`. |
| crates/core/src/machine.rs | Re-exports `parse_channels` from the machine module. |
| crates/core/src/dsl/ast.rs | Removes track/modifier types; adds `Expr::Var` for variables. |
| crates/core/src/dsl/lexer.rs | Simplifies tokens to identifiers/operators; removes modifier/int lexing. |
| crates/core/src/dsl/parser.rs | Parses identifiers into `Atom(Grid)` or `Var`, removes modifier parsing. |
| crates/core/src/dsl/eval.rs | Evaluates `Expr` to `Grid` with env-based variable resolution. |
| crates/core/src/dsl/display.rs | Updates `Display` to handle variables and expression-only printing. |
| crates/core/src/dsl/error.rs | Removes modifier-related errors; adds unknown-variable error kind. |
| crates/core/src/dsl.rs | Updates public `dsl::parse` API to return `Grid` and accept an env. |
| crates/cli/src/run.rs | Wires `agogo run` through `parse_channels` and pre-parsed spec extraction. |
</details>






<!-- gh-id: 3142980219 -->
### Copilot on [`doc/reviews/review-00020.md:62`](https://github.com/cmk/agogo/pull/20#discussion_r3142980219) (2026-04-26 04:29 UTC)

The commit count is inconsistent: the header says "Commits: 6" but the Commit Hygiene section starts with "All five commits...". Please reconcile the numbers (either update the count or the wording) so the review record is internally consistent.
```suggestion
All six commits follow conventional format and are appropriately
```

<!-- gh-id: 3142986105 -->
#### ↳ cmk ([2026-04-26 04:37 UTC](https://github.com/cmk/agogo/pull/20#discussion_r3142986105))

Fixed — `parse_channels` now rejects duplicate IDs with `BadValue("id", "duplicate channel ID ...")`.

<!-- gh-id: 3142986253 -->
#### ↳ cmk ([2026-04-26 04:37 UTC](https://github.com/cmk/agogo/pull/20#discussion_r3142986253))

Fixed — `parse_channels` rejects IDs that pass `Grid::from_str` with `BadValue("id", "... is a grid literal and cannot be used as a channel ID")`.

<!-- gh-id: 3142986368 -->
#### ↳ cmk ([2026-04-26 04:37 UTC](https://github.com/cmk/agogo/pull/20#discussion_r3142986368))

Fixed — error now includes channel ID: `format!("--ch {id}: {e}")`.

<!-- gh-id: 3142986438 -->
#### ↳ cmk ([2026-04-26 04:37 UTC](https://github.com/cmk/agogo/pull/20#discussion_r3142986438))

Fixed — same pattern: `format!("--ch {id}: {e}")` using the parsed ID from `named`.

<!-- gh-id: 3142986503 -->
#### ↳ cmk ([2026-04-26 04:37 UTC](https://github.com/cmk/agogo/pull/20#discussion_r3142986503))

Fixed — restored `display_quotes_values_with_spaces` and `display_quotes_values_with_commas` spot tests.

<!-- gh-id: 3142986560 -->
#### ↳ cmk ([2026-04-26 04:37 UTC](https://github.com/cmk/agogo/pull/20#discussion_r3142986560))

Fixed — removed `InvalidAtom` from `DslErrorKind`. The parser now falls back to `Expr::Var` for non-grid identifiers, so invalid grid-like names surface as `UnknownVariable` at eval time.

<!-- gh-id: 3142986663 -->
#### ↳ cmk ([2026-04-26 04:37 UTC](https://github.com/cmk/agogo/pull/20#discussion_r3142986663))

Fixed — "five" → "six".
