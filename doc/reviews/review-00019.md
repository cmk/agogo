# PR #19 — Plan 16: DSL parser + bi-Heyting algebra + CLI unification

## Summary

Polyrhythm DSL parser sprint (Plan 16) plus CLI surface cleanup.

### What changed

- **CLI rename**: `Channel.shift` → `Channel.delay`, `MAX_SHIFT` →
  `MAX_DELAY` throughout the workspace. `--ch` key `div` → `grid`,
  `shift-ms` → `delay`. Removed `offset-ms`, `swing`, `swing-res`
  from CLI (swing and offset will come from the DSL via `grid=`
  once the parser is wired into `ChannelSpec`; until then these
  knobs default to zero).

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

<!-- gh-id: 3142789399 -->
### Copilot on [`doc/plans/plan-2026-04-25-01.md`](https://github.com/cmk/agogo/pull/19#discussion_r3142789399) (2026-04-26 00:42 UTC)

This plan document is internally inconsistent about polyrhythm and operator symbols: it says parsing `T16~T16:80 & T8t@-50` yields a `Vec<TrackSpec>` (lines 9–11), but later says the DSL produces a single `TrackSpec` and polyrhythm is via multiple `--ch` flags (lines 40–41). Additionally, the grammar defines meet as `&` (line 29) while the examples use `^` for common refinement (line 71) and `&` for polyrhythm (lines 68–69). Please reconcile grammar, examples, and the polyrhythm story so the implementation target is unambiguous.

<!-- gh-id: 3142789403 -->
### Copilot on [`doc/reviews/review-00018.md`](https://github.com/cmk/agogo/pull/19#discussion_r3142789403) (2026-04-26 00:42 UTC)

This review summary claims the operator swap is `&`=polyrhythm, `|`=join, `^`=meet and that the parser produces `Vec<TrackSpec>`, but the updated `doc/designs/dsl.md` / Plan 16 draft currently have conflicting definitions (e.g., `&` appears as both meet and polyrhythm). Once the design/plan docs are reconciled, please update this review record to match the final, unambiguous DSL spec.

<!-- gh-id: 3142789407 -->
### Copilot on [`crates/core/src/channel/transform.rs`](https://github.com/cmk/agogo/pull/19#discussion_r3142789407) (2026-04-26 00:42 UTC)

Test/property names here still use `shift_*` terminology after the `shift`→`delay` rename (see also `shift_over_300ms_saturates` at ~189 and `shift_upper_clamp`/`shift_lower_clamp` around ~293/304). Renaming these to `delay_*` would keep terminology consistent and reduce confusion when grepping for legacy `shift` references.

<!-- gh-id: 3142789411 -->
### Copilot on [`crates/core/src/machine/spec.rs:163`](https://github.com/cmk/agogo/pull/19#discussion_r3142789411) (2026-04-26 00:42 UTC)

Delay conversion here relies on `micro_from_ms`, which saturates out-of-range millisecond values to `i64::{MIN,MAX}`; after the clamp, extremely large inputs effectively become 0 ms or 300 ms without any user-visible error. Since this is user-provided `--ch delay=...`, consider rejecting out-of-range values with a `ChannelSpecError::BadValue("delay", "out of range")` (similar to `channel_trace`’s argv-boundary conversion) to avoid silently masking typos.

<!-- gh-id: 4176353829 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-04-26 00:42 UTC](https://github.com/cmk/agogo/pull/19#pullrequestreview-4176353829))

## Pull request overview

This PR lands the Plan 16 sprint plan + updated DSL design documentation, and performs a workspace-wide rename from *shift* (latency compensation) to *delay*, including CLI/spec plumbing changes to align terminology ahead of the parser implementation work.

**Changes:**
- Added Plan 16 sprint plan and a PR review record for the DSL parser sprint.
- Updated the DSL design doc with new/updated operators, modifiers, and precedence rules.
- Renamed `Channel.shift` → `Channel.delay` and `MAX_SHIFT` → `MAX_DELAY`, and updated CLI/channel-spec keys (`div`→`grid`, `shift-ms`→`delay`, removed `offset-ms`).

### Reviewed changes

Copilot reviewed 14 out of 15 changed files in this pull request and generated 6 comments.

<details>
<summary>Show a summary per file</summary>

| File | Description |
| ---- | ----------- |
| doc/reviews/review-00018.md | New PR review record summarizing Plan 16 + rename scope. |
| doc/plans/plan-2026-04-25-01.md | New Plan 16 implementation plan, grammar, and test/property checklist. |
| doc/designs/dsl.md | Updated DSL grammar/operator/precedence/modifier design notes. |
| crates/host-link/tests/bidirectional.rs | Updated tests for `MAX_DELAY` and `Channel.delay`. |
| crates/host-link/src/session.rs | Updated session tests for `MAX_DELAY` and `Channel.delay`. |
| crates/host-cpal/src/cpal/callback.rs | Updated test channel construction to use `delay`. |
| crates/core/src/out/midi.rs | Updated test channel construction to use `delay`. |
| crates/core/src/machine/spec.rs | Updated `--ch` spec parsing to `grid` + `delay`, removed swing/offset keys. |
| crates/core/src/machine.rs | Comment update to reflect delay terminology. |
| crates/core/src/channel/transform.rs | Renamed shift concepts to delay in core transform pipeline. |
| crates/core/src/channel/scheduler.rs | Updated scheduler to use `MAX_DELAY`/`delay`. |
| crates/core/src/channel.rs | Re-export rename `MAX_SHIFT`→`MAX_DELAY`. |
| crates/cli/src/run.rs | Updated `--ch` help text and tests to `grid` + `delay`. |
| crates/cli/src/main.rs | Updated CLI subcommands/trace plumbing from `divider/shift-ms` to `grid/delay`. |
| CLAUDE.md | Updated documentation for the argv-boundary float exception reference. |
</details>






<!-- gh-id: 3142789415 -->
### Copilot on [`crates/core/src/machine/spec.rs`](https://github.com/cmk/agogo/pull/19#discussion_r3142789415) (2026-04-26 00:42 UTC)

The comment says to “use grid= with DSL syntax instead”, but `grid` is still parsed as a plain `Grid` name in this PR (full DSL expressions are only planned later per the module docs). Consider rewording this to avoid implying that `grid=` already accepts the DSL today.

<!-- gh-id: 3142789420 -->
### Copilot on [`doc/designs/dsl.md:52`](https://github.com/cmk/agogo/pull/19#discussion_r3142789420) (2026-04-26 00:42 UTC)

The grammar/operator table currently assigns `&` to both polyrhythm (`poly := ... '&' ...`, line 14) and meet (`meet_expr ... '&' ...`, line 19), and the operator table also lists `&` twice (lines 49 and 53). This makes the DSL ambiguous and contradicts the stated operator swap in the PR summary; please make the symbol/precedence table and examples consistent (e.g., reserve one token for polyrhythm at the loosest level and a different token for lattice meet).

<!-- gh-id: 3142815660 -->
### Copilot on [`crates/cli/src/main.rs`](https://github.com/cmk/agogo/pull/19#discussion_r3142815660) (2026-04-26 01:13 UTC)

The `channel trace` CLI help calls this a “Grid expression” (implying the full DSL), but the code path parses it as a plain `Grid` name via `Grid::from_str` (no operators/modifiers). Consider renaming this help text/argument label to “grid name” (or explicitly noting that the full DSL is not yet accepted here) to avoid misleading users.

<!-- gh-id: 3142815664 -->
### Copilot on [`crates/core/src/channel/transform.rs`](https://github.com/cmk/agogo/pull/19#discussion_r3142815664) (2026-04-26 01:13 UTC)

These test names still use the old `shift_...` terminology (`shift_10ms_adds_exactly_480_samples`, `shift_over_300ms_saturates`, `shift_upper_clamp`, `shift_lower_clamp`) even though the field is now `delay`. Renaming them would keep terminology consistent and make grepping for delay-related behavior easier.

<!-- gh-id: 3142815666 -->
### Copilot on [`doc/designs/dsl.md`](https://github.com/cmk/agogo/pull/19#discussion_r3142815666) (2026-04-26 01:13 UTC)

The grammar assigns `&` to two different roles: `poly := track ('&' track)*` (polyrhythm separator) and `meet_expr := unary ('&' unary)*` (meet/GCD). This is ambiguous/unparseable as written and also contradicts the later statement that polyrhythm is achieved via multiple `--ch` flags. Update the doc to use distinct operators (or drop `poly` entirely) and ensure the operator table matches the actual parser surface.

<!-- gh-id: 3142815668 -->
### Copilot on [`doc/plans/plan-2026-04-25-01.md`](https://github.com/cmk/agogo/pull/19#discussion_r3142815668) (2026-04-26 01:13 UTC)

This plan describes the DSL output as `Vec<TrackSpec>` (Context/Goal) but later states “The DSL produces a single `TrackSpec` per expression” (line 40). Since this PR’s implementation exposes `dsl::parse -> TrackSpec`, the plan should be updated to consistently describe the chosen single-track API (or adjust the implementation/plan if `Vec<TrackSpec>` is still intended).

<!-- gh-id: 4176390497 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-04-26 01:13 UTC](https://github.com/cmk/agogo/pull/19#pullrequestreview-4176390497))

## Pull request overview

This PR advances Plan 16 by landing the updated DSL design docs and implementing the initial Rust DSL parser + evaluator, while also renaming channel “shift” latency compensation to “delay” and unifying the CLI channel key from `div` to `grid`.

**Changes:**
- Added a `agogo_core::dsl` module (AST/lexer/parser/eval/display/error) for parsing a single-track grid DSL into `TrackSpec`.
- Extended `Grid` algebra with bi-Heyting operations (`imply`, `neg`, `mid`, `coimp`, `coneg`, `comid`) and added spot checks + property tests.
- Renamed `Channel.shift` → `Channel.delay` and updated CLI/spec parsing (`div` → `grid`, `shift-ms` → `delay`) across the workspace.

### Reviewed changes

Copilot reviewed 23 out of 24 changed files in this pull request and generated 7 comments.

<details>
<summary>Show a summary per file</summary>

| File | Description |
| ---- | ----------- |
| doc/reviews/review-00019.md | Adds a local review record for Plan 16 / CLI surface changes. |
| doc/plans/plan-2026-04-25-01.md | Adds the full sprint plan and intended grammar/test plan. |
| doc/designs/dsl.md | Updates the DSL design document (grammar/operators/modifiers). |
| crates/host-link/tests/bidirectional.rs | Updates tests to use `MAX_DELAY` and `delay` field. |
| crates/host-link/src/session.rs | Updates LinkSession tests and constants for `delay`. |
| crates/host-cpal/src/cpal/callback.rs | Updates callback test channel struct initialization (`delay`). |
| crates/core/src/time/grid.rs | Renames heyting→imply, adds co-Heyting ops, and expands test suite. |
| crates/core/src/out/midi.rs | Updates test channel initialization (`delay`). |
| crates/core/src/machine/spec.rs | Updates `--ch` spec keys and maps into `Channel` with `delay`. |
| crates/core/src/machine.rs | Updates docs/tests for `delay` field naming. |
| crates/core/src/lib.rs | Exposes the new `dsl` module. |
| crates/core/src/dsl/parser.rs | Implements recursive descent parser (tokens → AST) + tests/proptests. |
| crates/core/src/dsl/lexer.rs | Implements single-pass lexer + tests/proptests. |
| crates/core/src/dsl/eval.rs | Implements AST evaluation to `TrackSpec` + tests. |
| crates/core/src/dsl/error.rs | Adds `DslError`/`DslErrorKind` with span-aware display. |
| crates/core/src/dsl/display.rs | Adds precedence-aware `Display` impls + tests. |
| crates/core/src/dsl/ast.rs | Defines DSL AST, spans, modifiers, and `TrackSpec`. |
| crates/core/src/dsl.rs | Adds public `dsl::parse()` API + integration tests/proptests. |
| crates/core/src/channel/transform.rs | Renames shift→delay throughout transform pipeline and tests. |
| crates/core/src/channel/scheduler.rs | Renames shift→delay in scheduler pipeline and property tests. |
| crates/core/src/channel.rs | Updates module docs and re-exports (`MAX_DELAY`). |
| crates/cli/src/run.rs | Updates `--ch` help text and error examples to `grid=`. |
| crates/cli/src/main.rs | Renames CLI args from divider/shift to grid/delay across subcommands. |
| CLAUDE.md | Updates documentation about argv-boundary float exception for `--ch delay`. |
</details>






<!-- gh-id: 3142815669 -->
### Copilot on [`doc/plans/plan-2026-04-25-01.md:74`](https://github.com/cmk/agogo/pull/19#discussion_r3142815669) (2026-04-26 01:13 UTC)

The examples use `^` for “common refinement” (lines 71 and 73), but the grammar in this same document defines meet/common refinement as `&` (line 29) and the implementation in this PR uses `&` for `Expr::Meet`. Update the examples (and any referenced token/test lists later in the plan) to reflect the current operator set.

<!-- gh-id: 3142815674 -->
### Copilot on [`doc/reviews/review-00019.md:12`](https://github.com/cmk/agogo/pull/19#discussion_r3142815674) (2026-04-26 01:13 UTC)

This summary says swing/offset now come from the DSL via `grid=` (line 12), but later in the same doc it notes the DSL parser is not yet wired into `ChannelSpec::parse` (lines 66–68). As implemented, the CLI currently can’t accept swing/offset via `grid=` and those knobs are effectively removed. Please reconcile this section with the current behavior (or wire `dsl::parse` into the `--ch grid=` path).

<!-- gh-id: 3142815680 -->
### Copilot on [`crates/core/src/machine/spec.rs:173`](https://github.com/cmk/agogo/pull/19#discussion_r3142815680) (2026-04-26 01:13 UTC)

`ChannelSpec::into_channel` currently hard-codes `shuffle` to `T16/0` and `offset` to `Micro::ZERO`, and `ChannelSpec::parse` only accepts `grid` as a plain `Grid` name. Since this PR also removes the legacy CLI keys (`swing`, `swing-res`, `offset-ms`), there’s no longer any way to configure swing/offset from `--ch` despite docs claiming they move into `grid=`. Consider (a) keeping the legacy keys until `grid=` is wired to the DSL parser, or (b) parsing `grid=` via `agogo_core::dsl::parse` and mapping at least swing immediately (and explicitly handling/rejecting tick-based offsets until the pipeline supports them).

<!-- gh-id: 3142826099 -->
### Copilot on [`doc/plans/plan-2026-04-25-01.md:75`](https://github.com/cmk/agogo/pull/19#discussion_r3142826099) (2026-04-26 01:26 UTC)

This plan document still contains multiple stale references to the old operator/AST design (e.g., `^` as meet and polyrhythm/`PolyAst` as part of the DSL). For example, the Examples section uses `^` (and mixes it with `&`), but the Grammar above defines meet as `&` and there is no polyrhythm separator in the DSL. Please update the examples/tasks/properties to consistently use the current operator set and the single-track (one `TrackSpec` per expression) design.

<!-- gh-id: 3142826107 -->
### Copilot on [`doc/plans/plan-2026-04-25-01.md:207`](https://github.com/cmk/agogo/pull/19#discussion_r3142826107) (2026-04-26 01:26 UTC)

The module layout / task breakdown still mentions `PolyAst` and `eval.rs` producing `Vec<TrackSpec>`, but the current design (and implementation in this PR) produces a single `TrackSpec` per expression and does not have `PolyAst`. Please update these sections to match the single-track API surface so the plan doesn’t contradict the code it’s describing.

<!-- gh-id: 3142826110 -->
### Copilot on [`doc/reviews/review-00019.md:12`](https://github.com/cmk/agogo/pull/19#discussion_r3142826110) (2026-04-26 01:26 UTC)

In the summary you state that `offset-ms`, `swing`, and `swing-res` were removed from the CLI because they “now come from the DSL via `grid=`”, but later in this same review you note the DSL parser is not yet wired into `ChannelSpec::parse`. As of this PR, users can’t actually set swing/offset via `grid=` yet, so this summary bullet should be reworded to reflect that it’s a follow-up step.

<!-- gh-id: 3142826113 -->
### Copilot on [`crates/core/src/machine/spec.rs:13`](https://github.com/cmk/agogo/pull/19#discussion_r3142826113) (2026-04-26 01:26 UTC)

The module docs say “When the DSL parser lands (Plan 16), it will accept full DSL expressions…”, but this PR already adds `agogo_core::dsl::parse`. What’s missing is wiring it into `ChannelSpec::parse` / `grid=`. Consider rewording this comment to avoid implying the parser doesn’t exist yet, and instead point to the follow-up integration work.

<!-- gh-id: 3142826116 -->
### Copilot on [`crates/core/src/dsl/lexer.rs:77`](https://github.com/cmk/agogo/pull/19#discussion_r3142826116) (2026-04-26 01:26 UTC)

The lexer indexes into `input.as_bytes()` and converts a single byte to `char` (`bytes[pos] as char`). For non-ASCII UTF-8 input this reports the wrong character (and the span length is always 1 byte), producing misleading `UnexpectedChar` errors and misaligned carets. Consider iterating with `char_indices()` (or `input[pos..].chars().next()`) and advancing by `len_utf8()`, so both the reported char and span are correct for Unicode input.

<!-- gh-id: 3142826120 -->
### Copilot on [`doc/designs/dsl.md:52`](https://github.com/cmk/agogo/pull/19#discussion_r3142826120) (2026-04-26 01:26 UTC)

The operator table still includes a polyrhythm row using `&`, but this document’s grammar and later text define `&` as meet (common refinement) and state polyrhythm is achieved via multiple `--ch` flags. This is internally contradictory and will mislead readers; remove the polyrhythm row (or replace it with a note outside the operator table).

<!-- gh-id: 4176398400 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-04-26 01:26 UTC](https://github.com/cmk/agogo/pull/19#pullrequestreview-4176398400))

## Pull request overview

This PR lands the Plan 16 planning/design documentation, introduces a new `agogo_core::dsl` parser for grid algebra expressions (including swing/offset modifiers), extends `Grid` with bi-Heyting operations and property tests, and unifies the CLI/channel surface by renaming shift→delay and `div`→`grid`.

**Changes:**
- Add a DSL lexer/parser/evaluator + Display round-trip support that parses a single-track expression into `TrackSpec`.
- Extend `Grid` with `imply`, `coimp`, `neg`, `coneg`, `mid`, `comid` plus extensive spot checks and proptests.
- Rename channel timing compensation from `shift` to `delay` across core/hosts/CLI, and rename channel spec key `div`→`grid`.

### Reviewed changes

Copilot reviewed 23 out of 24 changed files in this pull request and generated 7 comments.

<details>
<summary>Show a summary per file</summary>

| File | Description |
| ---- | ----------- |
| doc/reviews/review-00019.md | New review record describing Plan 16 scope and verification notes. |
| doc/plans/plan-2026-04-25-01.md | New sprint plan document for Plan 16 (parser + algebra + CLI updates). |
| doc/designs/dsl.md | Updated DSL design doc with new grammar, operators, precedence, and examples. |
| crates/host-link/tests/bidirectional.rs | Update tests for `MAX_DELAY` and `Channel.delay`. |
| crates/host-link/src/session.rs | Update Link session tests for `MAX_DELAY` and `Channel.delay`. |
| crates/host-cpal/src/cpal/callback.rs | Update callback test channel construction to use `delay`. |
| crates/core/src/time/grid.rs | Add/rename bi-Heyting ops (`imply`, `coimp`, etc.) + spot checks/proptests. |
| crates/core/src/out/midi.rs | Update tests for `Channel.delay`. |
| crates/core/src/machine/spec.rs | Update `--ch` parsing surface (`grid`, `delay`) and remove swing/offset CLI fields. |
| crates/core/src/machine.rs | Update internal test channel construction to use `delay`. |
| crates/core/src/lib.rs | Export new `dsl` module. |
| crates/core/src/dsl/parser.rs | New recursive descent parser producing `TrackAst` from tokens. |
| crates/core/src/dsl/lexer.rs | New single-pass lexer producing token stream + proptests. |
| crates/core/src/dsl/eval.rs | New evaluator mapping AST to `TrackSpec` via `Grid` operations. |
| crates/core/src/dsl/error.rs | New error types with span-aware display formatting. |
| crates/core/src/dsl/display.rs | New precedence-aware Display impls for AST/modifiers. |
| crates/core/src/dsl/ast.rs | New AST + `TrackSpec` definitions. |
| crates/core/src/dsl.rs | New public entrypoint `dsl::parse` wiring lexer→parser→eval. |
| crates/core/src/channel/transform.rs | Rename shift→delay in transform pipeline + constant `MAX_DELAY`. |
| crates/core/src/channel/scheduler.rs | Rename shift→delay in scheduling window math and tests. |
| crates/core/src/channel.rs | Re-export `MAX_DELAY` and update module docs. |
| crates/cli/src/run.rs | Update CLI docs/examples and error text for `grid`/`delay` channel spec keys. |
| crates/cli/src/main.rs | Rename CLI flags/args for grid + delay; remove shuffle/offset from `channel trace`. |
| CLAUDE.md | Update documentation reference for the argv-boundary exception module. |
</details>






<!-- gh-id: 3142826123 -->
### Copilot on [`doc/designs/dsl.md:19`](https://github.com/cmk/agogo/pull/19#discussion_r3142826123) (2026-04-26 01:26 UTC)

The PR description says `&` is the polyrhythm operator and `^` is meet, but the implemented grammar/docs here define `&` as meet and `|` as join with polyrhythm handled by multiple `--ch` flags. Please reconcile the PR description with the actual grammar (either update the description or adjust docs/code to match).

<!-- gh-id: 3142921933 -->
#### ↳ cmk ([2026-04-26 03:23 UTC](https://github.com/cmk/agogo/pull/19#discussion_r3142921933))

Fixed — reconciled plan doc: `Vec<TrackSpec>` → `TrackSpec`, all `^` → `&` in examples, removed polyrhythm references. Single-track API throughout.

<!-- gh-id: 3142922753 -->
#### ↳ cmk ([2026-04-26 03:24 UTC](https://github.com/cmk/agogo/pull/19#discussion_r3142922753))

File renamed to review-00019.md (matching PR #19) in commit a025f2b. The stale review-00018.md no longer exists in the tree.

<!-- gh-id: 3142922814 -->
#### ↳ cmk ([2026-04-26 03:24 UTC](https://github.com/cmk/agogo/pull/19#discussion_r3142922814))

Fixed — renamed all `shift_*` test names to `delay_*` (`delay_10ms_adds_exactly_480_samples`, `delay_over_300ms_saturates`, `delay_upper_clamp`, `delay_lower_clamp`).

<!-- gh-id: 3142922926 -->
#### ↳ cmk ([2026-04-26 03:24 UTC](https://github.com/cmk/agogo/pull/19#discussion_r3142922926))

Fixed — `micro_from_ms` now returns `Option<Micro>` and `into_channel` rejects out-of-range values with `ChannelSpecError::BadValue("delay", ...)` instead of silently saturating.

<!-- gh-id: 3142922995 -->
#### ↳ cmk ([2026-04-26 03:24 UTC](https://github.com/cmk/agogo/pull/19#discussion_r3142922995))

Fixed — reworded to "use `grid=` (plain grid name now; full DSL expressions in a future PR)."

<!-- gh-id: 3142923056 -->
#### ↳ cmk ([2026-04-26 03:24 UTC](https://github.com/cmk/agogo/pull/19#discussion_r3142923056))

Fixed — removed the stale `poly` rule. `&` now appears only as meet.

<!-- gh-id: 3142923256 -->
#### ↳ cmk ([2026-04-26 03:25 UTC](https://github.com/cmk/agogo/pull/19#discussion_r3142923256))

Fixed — changed to "Grid name" in the help text.

<!-- gh-id: 3142923369 -->
#### ↳ cmk ([2026-04-26 03:25 UTC](https://github.com/cmk/agogo/pull/19#discussion_r3142923369))

Fixed — reworded to "swing and offset will come from the DSL via `grid=` once the parser is wired into `ChannelSpec`; until then these knobs default to zero."

<!-- gh-id: 3142923461 -->
#### ↳ cmk ([2026-04-26 03:25 UTC](https://github.com/cmk/agogo/pull/19#discussion_r3142923461))

Deferred — wiring `dsl::parse` into `ChannelSpec::parse` is tracked as follow-up item #1 in the local review section. Not blocking this PR since the parser and the CLI surface are independently useful.

<!-- gh-id: 3142923580 -->
#### ↳ cmk ([2026-04-26 03:25 UTC](https://github.com/cmk/agogo/pull/19#discussion_r3142923580))

Fixed — reworded to acknowledge the parser exists; only the `ChannelSpec` wiring is the follow-up.

<!-- gh-id: 3142923693 -->
#### ↳ cmk ([2026-04-26 03:25 UTC](https://github.com/cmk/agogo/pull/19#discussion_r3142923693))

Deferred — tracked as follow-up item #2 in the local review. The lexer is ASCII-only by design (all DSL tokens are ASCII); non-ASCII input correctly errors but with a misleading char in the message. Switching to `char_indices()` is a clean improvement for a future PR.

<!-- gh-id: 3142923799 -->
#### ↳ cmk ([2026-04-26 03:25 UTC](https://github.com/cmk/agogo/pull/19#discussion_r3142923799))

Fixed — removed the polyrhythm row from the operator table. `&` now appears only as meet/GCD.

<!-- gh-id: 3142923937 -->
#### ↳ cmk ([2026-04-26 03:25 UTC](https://github.com/cmk/agogo/pull/19#discussion_r3142923937))

The PR description was auto-generated from the review file's Summary section, which has been corrected. The updated description will land with the next push.
