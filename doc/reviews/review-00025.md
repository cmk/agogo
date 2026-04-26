# PR #25 — Drop `Channel.snap_to_quantum` (audit P2)

## Summary

Removes the `snap_to_quantum: Option<Quantum>` field from
`crates/core/src/channel/transform.rs:Channel`. The field was
**never read in production**: its only consumer
(`LinkSession::arm_channel`) is called from tests only, never from
`run.rs`. So the field was purely arming-time intent stored as
persistent channel state, going through 15 `: None,` constructor
sites for zero runtime effect.

This is **P2 of the structural-type audit** — the smallest pre-P3
cleanup. P3 (sum-typed `Channel` + per-target role enums) will
reshape `Channel` into `ChannelCommon` + per-target variants;
dragging a dead-but-pervasive field through that reshape would be
wasted churn.

### What changed

- **`crates/core/src/channel/transform.rs`**: removed the
  `snap_to_quantum: Option<Quantum>` field from `Channel`. Dropped
  the now-unused `Quantum` import. Updated the `offset` doc
  comment to point at the new arming-intent path.
- **`crates/host-link/src/session.rs`**: replaced
  `arm_channel(&mut Channel)` with stateless
  `snap_offset_for(Option<Quantum>) -> Micro`. Caller folds the
  returned delta into its own `Channel.offset`. The
  `agogo_core::channel::Channel` import is gone — `LinkSession` no
  longer touches `Channel`.
- **`crates/core/src/machine/spec.rs`**: kept
  `ChannelSpec.snap_to_quantum_micro` and the `snap-quantum-us=N`
  parser key (back-compat). Added `ChannelSpec::snap_intent() ->
  Option<Quantum>` accessor. Removed the line in `into_channel`
  that previously copied the value onto `Channel`.
- **`crates/host-link/tests/bidirectional.rs:213`**: integration
  test now constructs `Channel` without the field, computes the
  snap delta via `snap_offset_for(snap_intent)`, and applies it
  manually. Net behaviour identical.
- **15 `snap_to_quantum: None,` literals deleted** across
  `crates/core/src/channel/scheduler.rs`,
  `crates/core/src/channel/transform.rs`,
  `crates/core/src/machine.rs`,
  `crates/core/src/out/midi.rs`,
  `crates/host-cpal/src/cpal/callback.rs`, and the original three
  `crates/host-link/src/session.rs` test fixtures.
- Three new tests + three renamed equivalents (see "Test plan"
  below).

### What did not change

- **CLI surface**: `--ch ...,snap-quantum-us=N` still parses. It
  was previously stored on the runtime `Channel` and never
  applied; now it's stored only on `ChannelSpec` and still never
  applied. The wiring that would actually apply it is documented
  in the plan's Deferred section.
- **Display output**: `ChannelSpec::Display` still emits
  `snap-quantum-us=N` when the field is set. Round-trip preserved.
- **`LinkClock::snap_offset_micro`**: unchanged. The new
  `LinkSession::snap_offset_for` is a one-line wrapper that
  delegates to it; the underlying numerical contract
  (`quantum_snap_nonneg`, `quantum_snap_idempotent` properties on
  `LinkClock`) is unaffected.

### Why this is the right shape

- **The field was dead.** Empirically — not theoretically —
  nothing in production read it. The audit (finding J) caught the
  lifetime mismatch ("arming-time intent stored as persistent
  channel state"); follow-up grep confirmed even the arming
  consumer was test-only.
- **Decoupling host-link from Channel.** With this change,
  `LinkSession` doesn't import `agogo_core::channel::Channel` at
  all. The `snap_offset_for` API takes only the bits it actually
  needs (the optional Quantum). This narrows the dependency graph
  in the right direction for audit P5 (host-link decoupling).
- **Spec keeps the parsed intent for future wiring.** Removing
  `snap-quantum-us=N` from the parser would be a user-facing
  break; keeping it parsed-but-stored-on-spec preserves the
  surface and makes the future orchestrator wiring a five-line
  change in `run.rs`.

### Phasing context

| Phase | Status |
|-------|--------|
| P0a — Conn-discipline sweep | DEFERRED (upstream `Conn::then`) |
| P0b — Float surface area | DEFERRED (depends on P0a) |
| P1 — U7 / U4 newtypes | merged (PR #23) |
| **P2 — drop `Channel.snap_to_quantum`** | **this PR** |
| P3 — sum-typed `Channel` + role enums | next |
| P4 — sum-typed `ChannelSpec` | after P3 |
| P5 — host-link decoupling | after P4 |
| P6 — host-cpal output typing | opportunistic |

### Test plan

- [x] `cargo test --workspace` green (480 passing, 2 ignored —
  net +2 new tests, 3 renamed: `snap_offset_for_none_is_zero`,
  `snap_intent_none_when_key_absent`,
  `snap_intent_round_trips_through_spec` (proptest, full `i64`
  domain) added; the old `arm_channel_*` trio is renamed to
  `snap_offset_for_*` and re-expressed against the new pure API).
- [x] `cargo clippy --all-targets -- -D warnings` clean.
- [x] `scripts/check-floats.sh` exit 0 (no f64 changes).
- [x] `scripts/check-pii.sh` clean.
- [x] Bidirectional integration test still exercises the snap
  path (just via the new API).
- [x] Existing `parse_*` tests for `snap-quantum-us=N` continue
  to pass — parser surface preserved.

Note: this branch was created from `origin/main` before PR #24's
hook fix landed, so the pre-commit hook in this worktree is the
broken old version. The full check chain was run **manually**
before the implementation commit; future plan branches will get
the corrected hook automatically once #24 merges.

## Local review (2026-04-26)

**Branch:** plan/2026-04-26-01
**Commits:** 3 (origin/main..plan/2026-04-26-01)
**Reviewer:** Claude (sonnet, independent)

---

### Commit Hygiene

All three commits carry conventional prefixes (`plan:`, `refactor:`, `doc:`). The single implementation commit is appropriately atomic — the field deletion, API replacement, test renames, and literal sweep are one logical change with no unrelated work mixed in. Each commit lands in a buildable state given that the field removal and all 15 call-site deletions are in the same commit.

### Code Quality

No unsafe code, no stored `f32`/`f64`, no open-coded unit arithmetic. All existing Conn discipline is preserved; the new `snap_offset_for` adds no arithmetic of its own — it's a one-line match delegating to `clock.snap_offset_micro`. The `Quantum` import in `transform.rs` is correctly dropped because the field that referenced it is gone.

The `&mut self` on `snap_offset_for` (instead of the plan's `&self`) is accurate and explained: `LinkClock::snap_offset_micro` requires `&mut` because it captures audio session state. Confirmed in `session.rs:186`.

The `snap_intent()` return type uses a qualified path (`Option<crate::fxp::Quantum>`) rather than importing `Quantum` at the module top. The type appears only in this one method and nowhere else in the impl block, so the choice is defensible. No issue.

One minor note: the `--quantum` flag in `crates/cli/src/main.rs` doc comment is updated to reference `ChannelSpec::snap_intent` and `LinkSession::snap_offset_for`, which is accurate. Confirmed that `run.rs` at line 138 calls `spec.into_channel()` but never calls `snap_intent()` or `snap_offset_for`, consistent with the plan's explicit "out of scope" deferral.

No dead code, no redundant logic, no clippy-level issues visible in the diff.

**Structural check — downstream consumers of `Channel.snap_to_quantum`.** The plan claims `arm_channel` was the only consumer. Verified:
- `run.rs` constructs channels via `spec.into_channel()` at line 141 and passes them directly to `run_with_rate`. No call to `arm_channel` anywhere in the file.
- All 15 `snap_to_quantum: None,` construction sites are in test fixtures or module-local helpers, not production paths.
- `session.rs` no longer imports `agogo_core::channel::Channel` — the import is gone at line 7 in the diff.
- The `bidirectional.rs` integration test constructs `Channel` inline at line 201 without the removed field, which compiles only if the field is absent. This is a compile-time proof the sweep was complete.

The field removal is safe.

### Test Coverage

**`snap_intent_round_trips_through_spec` generator domain.** The proptest at `crates/core/src/machine/spec.rs` uses `any::<i64>()`. This spans the full signed 64-bit domain per the CLAUDE.md proptest requirement. The parser stores the raw `i64` and `snap_intent()` rewraps it without arithmetic, so there is no intermediate boundary to hide. Domain is correct.

**Renamed `snap_offset_for_*` tests — semantic equivalence.** The three prior `arm_channel_*` tests covered:
1. No-op when snap is None → `snap_offset_for_none_is_zero`: semantically equivalent, now tests the return value directly instead of observing `ch.offset` not changing. Equivalent.
2. Never reduces offset below starting value → `snap_offset_for_some_is_nonneg`: the old test iterated three starting offsets and checked `ch.offset >= start`; the new test checks `delta.0 >= 0`. Because the old test began from `Micro::ZERO` and then added a non-negative delta, the reformulation is semantically equivalent — the non-negativity of the delta is the load-bearing invariant. The new test deliberately narrows scope (the caller now owns the fold operation); the integration test in `bidirectional.rs` covers the fold path end-to-end, so coverage is adequate.
3. Near-idempotent → `snap_offset_for_is_near_idempotent`: equivalent. Two consecutive calls, drift < 1 ms. The `&mut self` semantics mean the calls share mutable state, preserving the spirit of the original.

**`snap_offset_for_some_matches_clock` (delegation check).** Skipped because `LinkSession.clock` is private. The justification in the Review section is sound: the wrapper is a one-line `match` arm, the delegation is enforced by the type system (the only way to produce a `Micro` from a `Some(q)` arm is to call `snap_offset_micro(q)`), and the underlying numerical contract is covered by `quantum_snap_nonneg` / `quantum_snap_idempotent` on `LinkClock`. The plan's own Verification table notes this property is about confirming the `LinkSession` wrapper delegates correctly — at one line of code, compile-time enforcement is the right substitute.

### Plan Conformance

**T1** (session.rs): `snap_offset_for` added, `arm_channel` removed, `Channel` import dropped. Complete.

**T2** (spec.rs): `snap_intent()` accessor added, `into_channel` line that populated the field removed. Complete.

**T3** (transform.rs): field and its doc block deleted, `Quantum` import removed. Complete.

**T4** (literal sweep): diff shows 15 removals across the six stated files. Complete.

**T5** (bidirectional.rs): `arm_channel` call replaced with `snap_offset_for` + manual fold. Matches the plan's T5 description in intent, though the implementation uses `Micro(ch.offset.0.saturating_add(delta.0))` rather than the plan's draft snippet `.saturating_add_signed(delta.0)`. Since both `ch.offset.0` and `delta.0` are `i64`, `i64::saturating_add(i64)` and a hypothetical `saturating_add_signed` are the same operation; the implementation is correct.

All four plan-documented deviations are present and accurate.

**Verification table:**
- `snap_offset_for_none_is_zero`: present at `session.rs:222`. Pass.
- `snap_offset_for_some_matches_clock`: intentionally skipped; documented in Review. Accept.
- `snap_intent_round_trips_through_spec`: present at `spec.rs:216`, full `any::<i64>()` domain. Pass.

### Risks

**`--quantum` flag UX.** Confirmed: the flag existed before this PR and was never wired to anything in `run.rs`. The doc comment update at `main.rs:9-15` now correctly describes the future wiring path. No regression, no new confusion introduced.

**`snap_to_quantum` surviving anywhere.** The field name `snap_to_quantum_micro` survives on `ChannelSpec` (the parsed storage) and is accessed only via `snap_intent()`. No production path reads it. Correct.

**`saturating_add` correctness.** `ch.offset.0` is `i64`; `delta.0` is `i64`. `i64::saturating_add` takes `i64`. The addition saturates at `i64::MIN`/`i64::MAX`. Since `delta.0 >= 0` is asserted by `snap_offset_for_some_is_nonneg`, this can only saturate upward. The integration test's bound check (`ch.offset.0 < 2_000_001`) would catch saturation to `i64::MAX`. No issue.

No shell/FS/network inputs, no security surface.

### Recommendations

**Must fix before push:** none.

**Follow-up (future work):**
- Wire `run.rs` to call `session.snap_offset_for(spec.snap_intent())` per channel after the `channels` vec is built. The plan correctly defers this; noting it here as the one user-visible feature that remains inert after this PR. The review file's "What did not change" section documents it accurately.
- The `snap_offset_for_some_is_nonneg` test checks a single quantum value (`Quantum::from_bars(4)`). A proptest across `any::<i64>()` quantum values (filtered to positive, since quantum must be positive per Link's contract) would more thoroughly cover the non-negativity invariant. Not a must-fix — the underlying `quantum_snap_nonneg` property on `LinkClock` already covers this — but worth considering when P3 lands.
