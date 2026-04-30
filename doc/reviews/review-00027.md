# PR #27 — Drop `ChannelSpec.dev` field (audit P4)

## Summary

Drops the `dev: ChannelDev` field from
`crates/core/src/channel/spec.rs:ChannelSpec`. The `ChannelDev`
enum is removed entirely; the public re-export from
`agogo_core::control` is gone too.

This is **P4 of the structural-type audit** — the cleanup of the
last surviving parser-stage tag from the original `dev` × `mode`
cross-product (audit finding A). PR #26 (P3) reshaped `mode` into
per-target role enums, so the headline nonsense
`dev=audio,mode=AnalogPulse` was already gone. The narrower
nonsense `dev=audio,mode=MidiClock` was still representable
because `dev` and `mode` were independent fields. P4 eliminates
the `dev` field entirely; every `ChannelSpec` is now implicitly
MIDI-targeted by construction.

### What changed

- **`crates/core/src/channel/spec.rs`**: `dev: ChannelDev` field
  removed; `ChannelDev` enum + its `Display` impl removed; the
  `dev=audio` → `AudioDeferred` check moved from `into_channel`
  to the parser body. The parser still requires the `dev=` key
  (`MissingKey("dev")` contract preserved) via a local
  `dev_seen: bool` gate.
- **`crates/core/src/control.rs`**: `pub use spec::ChannelDev`
  removed from the re-export list.
- **`crates/cli/src/run.rs:122-136`**: the find-first-MIDI-spec
  filter (which was a tautology since every spec was MIDI)
  collapses to `named.first().map(...).expect(...)`. The
  empty-spec case is guarded by the existing
  `args.ch.is_empty()` check at line 95, so `.expect()` is
  structurally unreachable.
- **`Display for ChannelSpec`**: emits `dev=midi` as a literal
  string so the round-trip parser still sees the required key.
- **New test** `parse_rejects_dev_unknown` — pins the
  `dev=florble` → `BadValue("dev", "florble")` path that wasn't
  explicitly covered.

### What did not change

- **CLI surface**: `--ch dev=midi,grid=t4,...` parses identically.
  No flag rename, no removal. Display still emits `dev=midi,...`.
- **Existing parse error contracts**: `MissingKey("dev")`,
  `AudioDeferred`, `BadValue("dev", _)` all preserved.
- **`AudioDeferred` error variant**: still present on
  `ChannelSpecError`, just raised at parse time instead of
  `into_channel` time.

### Why this is the right shape

- **Make illegal states unrepresentable.** A `ChannelSpec`
  cannot represent `dev=audio` anymore — the constructor (parser)
  rejects it, and there's no field to hold the value. The pre-P4
  `if matches!(self.dev, ChannelDev::Audio) { return
  Err(AudioDeferred); }` check in `into_channel` was checking a
  state that always-already-validated could never be in. P4
  removes the dead check.
- **Defer the full sum-type reshape.** A single-variant
  `enum ChannelSpec { Midi { ... } }` is structurally equivalent
  to a struct + matchify-everywhere overhead. The right moment to
  introduce variants is when a second routing target gets a
  parser (`dev=din` or `dev=cv`) — not before.
- **Locality**: 3 files touched in production, ~50 lines net.
  The 67-reference sweep cost of P3 doesn't apply because `dev`
  was already mostly-vestigial.

### Phasing context

| Phase | Status |
|-------|--------|
| P0a — Conn-discipline sweep | DEFERRED (upstream `Conn::then`) |
| P0b — Float surface area | DEFERRED (depends on P0a) |
| P1 — U7 / U4 newtypes | merged (PR #23) |
| P2 — drop `Channel.snap_to_quantum` | merged (PR #25) |
| P3 — sum-typed `Channel` + role enums | merged (PR #26) |
| **P4 — drop `ChannelSpec.dev` field** | **this PR** |
| P5 — host-link decoupling | next (blocked on upstream Conn::then for the q_f64 part) |
| P6 — host-cpal output typing | opportunistic |

### Test plan

- [x] `cargo test --workspace` green (485 passing, 2 ignored).
- [x] `cargo clippy --all-targets -- -D warnings` clean.
- [x] `scripts/check-floats.sh` exit 0 (no f64 changes).
- [x] `scripts/check-pii.sh` clean.
- [x] `grep -rn "ChannelDev" crates/` returns zero matches —
  the symbol is fully gone.
- [x] Existing `parse_rejects_dev_audio`, `parse_rejects_missing_dev`,
  `parse_minimal`, `spec_round_trip`, and the rest of the parser
  test suite continue to pass.

Note: this branch was created from `origin/main` before PR #24's
hook fix landed, so the pre-commit hook in this worktree is the
broken old version. Full check chain run **manually** before
commit.

## Local review (2026-04-26)

**Branch:** plan/2026-04-26-03
**Commits:** 3 (origin/main..plan/2026-04-26-03, after autosquash)
**Reviewer:** Claude (sonnet, independent)

---

### Commit Hygiene

Three commits in the expected order: `plan:`, `refactor:`, `doc:`. Subjects are under 72 characters, present-tense imperative, conventional prefixes. Each commit is atomic to its logical unit. The `refactor:` commit carries the full production change (T1–T4) plus the planned `cli_run_accepts_only_midi_specs` smoke test (added as a fixup post-review and autosquashed per the CLAUDE.md fixup workflow).

### Code Quality

**Semantic equivalence of the `AudioDeferred` migration (T1 highest-value check).** Pre-P4, a caller could construct a `ChannelSpec` by hand with `dev: ChannelDev::Audio` and then call `into_channel()`, which returned `AudioDeferred`. Post-P4, `ChannelSpec` has no `dev` field, so constructing such a spec is a compile error. The only path into a `ChannelSpec` is `ChannelSpec::parse`, and the parser rejects `dev=audio` immediately with `AudioDeferred`. The illegal state is unrepresentable. The contract is strictly tighter, not weaker.

**`dev_seen: bool` gating** is correct. `parse` is called once per spec string; each call's `dev_seen` is independent. A spec without `dev=` produces `MissingKey("dev")`; a spec with `dev=audio` returns early with `AudioDeferred` before `dev_seen` is set, which is correct.

**`cli/run.rs` `.expect()` reachability.** `run()` returns early at line 95 if `args.ch.is_empty()`. `parse_channels` returns one entry per `args.ch` element on success. `.first()` on a non-empty `Vec` never returns `None`. The `.expect("at least one --ch spec required (checked above)")` is structurally unreachable.

No f64 outside allowlist. No unsafe. No open-coded arithmetic. No dead code — `ChannelDev`, its `Display` impl, `arb_dev()`, and the `dev` field/parser local are all removed completely.

### Test Coverage

`parse_rejects_dev_audio` and `parse_rejects_missing_dev` both remain meaningful — both rejections fire from `parse` post-P4; test bodies unchanged.

`spec_round_trip` proptest loses one dimension (`arb_dev()` was always `Just(Midi)` — a unit strategy contributing nothing). Round-trip semantics preserved.

`parse_rejects_dev_unknown` (new) pins the `dev=florble` → `BadValue("dev", "florble")` path.

`run_accepts_minimal_midi_spec_through_to_rate_dispatch` (new, added via autosquashed fixup) confirms `--ch dev=midi,grid=t32t,out=default` reaches the rate-dispatch gate (failure originates from rate allowlist, not from spec/dev rejection — proves parse cleared).

### Plan Conformance

- **T1** (drop field + enum + Display + parser audio-error move): fully delivered.
- **T2** (arb_spec + tests): `arb_dev` deleted, `arb_spec` tuple reduced, `parse_minimal`'s `assert_eq!(spec.dev, ChannelDev::Midi)` removed, `parse_rejects_dev_unknown` added.
- **T3** (CLI port-name filter): collapsed to `.first().map(...).expect(...)`.
- **T4** (final sweep): `grep -rn "ChannelDev" crates/` returns zero matches.
- **Verification spot check** `cli_run_accepts_only_midi_specs`: delivered as `run_accepts_minimal_midi_spec_through_to_rate_dispatch` (renamed for precision; same coverage).

All deviations match the plan's documented Review section.

### Risks

**`ChannelDev` public API removal.** Workspace-internal crate, no external consumer. Risk is zero for this workspace.

**Display hardcodes `"dev=midi"`.** When a future parser adds `dev=din` or `dev=cv`, `Display` will need updating alongside or it will misrepresent non-MIDI specs. The struct doc comment explicitly calls this out and names the moment when `ChannelSpec` becomes an enum.

No TODOs, stubs, or security issues introduced.

### Recommendations

**Must fix before push:** None.

**Follow-up (future work):**
- When `dev=din` or `dev=cv` lands as a parser key, `Display for ChannelSpec` must be updated; the struct doc comment flags this.
- `dev=` could be made optional (default `midi`) in a follow-up. Deferred per plan.

<!-- gh-id: 4176868946 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-04-26 11:36 UTC](https://github.com/cmk/agogo/pull/27#pullrequestreview-4176868946))

## Pull request overview

Removes the legacy routing-target tag (`dev`) from `ChannelSpec`, making the parsed spec implicitly MIDI-targeted and preserving the existing CLI/parse contracts by still requiring a `dev=` key (now validated-only).

**Changes:**
- Dropped `ChannelSpec.dev` and deleted the `ChannelDev` enum (and its public re-export).
- Moved `dev=audio` rejection to parse time; `Display for ChannelSpec` now hardcodes `dev=midi` to preserve round-tripping.
- Simplified CLI MIDI port selection logic to use the first parsed spec; added coverage for unknown `dev` values and a CLI smoke test.

### Reviewed changes

Copilot reviewed 5 out of 5 changed files in this pull request and generated 1 comment.

<details>
<summary>Show a summary per file</summary>

| File | Description |
| ---- | ----------- |
| `crates/core/src/channel/spec.rs` | Removes `dev` field/enum, validates `dev=` presence via `dev_seen`, hardcodes `dev=midi` in `Display`, updates tests/proptests accordingly. |
| `crates/core/src/control.rs` | Removes `ChannelDev` from public re-exports. |
| `crates/cli/src/run.rs` | Collapses “find first MIDI spec” logic to “first spec”; adds a regression-style CLI test to ensure minimal MIDI spec parses through to rate dispatch. |
| `doc/plans/plan-2026-04-26-03.md` | Adds Plan 22 documentation for audit P4 work. |
| `doc/reviews/review-00027.md` | Adds review record for PR #27 / audit P4. |
</details>






<!-- gh-id: 3143413244 -->
### Copilot on [`crates/cli/src/run.rs:130`](https://github.com/cmk/agogo/pull/27#discussion_r3143413244) (2026-04-26 11:36 UTC)

Using `.expect(...)` here will panic instead of returning a `Result` error if the invariant is ever violated (e.g., if the earlier guard changes or `named` can become empty in the future). Since `run()` already returns `Result<(), String>`, prefer propagating a normal error (`ok_or_else(...)` / `ok_or(...)`) so the CLI fails gracefully rather than crashing.
```suggestion
        .ok_or_else(|| "at least one --ch spec is required".to_string())
        .map(|(_, spec)| spec.out.clone().unwrap_or_else(|| "default".to_string()))?;
```

<!-- gh-id: 3143416919 -->
#### ↳ cmk ([2026-04-26 11:39 UTC](https://github.com/cmk/agogo/pull/27#discussion_r3143416919))

Fixed in 6e6efd0 — switched to `.ok_or_else(...)` + `?` per the suggestion. The empty-spec branch is still structurally unreachable today via the args.ch.is_empty guard, but the new shape stays graceful if any future caller path bypasses that guard.
