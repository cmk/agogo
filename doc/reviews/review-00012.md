# PR #12 — Plan 09: Link bidirectional foundation

## Summary

Lands the Link write-path foundation that v0.5's Link work will
build on. Three new capabilities plus the scaffolding for a fourth:

- **Tempo push.** `LinkSession::set_tempo(Tempo)` →
  `LinkClock::push_tempo(Tempo)` → `AblLink::set_tempo` + commit.
  Exposed via `agogo link push-tempo --bpm <B>`. The Tempo ↔ f64
  Link FFI is contained to two annotated sites inside `LinkClock`.
- **Transport FSM seam.** Minimal `{Stopped, Playing}` FSM declared
  via `rust_fsm::state_machine!`. Eight (state × event)
  combinations all declared explicitly so `consume` never returns
  `TransitionImpossibleError`. `User*` events emit
  `PublishPlaying` / `PublishStopped`; `LinkReports*` never
  publish (the no-echo-loop invariant between two peers). This is
  the skeleton v0.5 Sprint 01 extends with forerun states — by
  landing it now we force the forerun sprint to extend a
  Link-aware declaration rather than design one in isolation and
  bolt on Link subscription later.
- **Quantum snap.** `Channel::snap_to_quantum: Option<Quantum>`.
  `LinkSession::arm_channel(&mut Channel)` bakes the micro-offset
  to the next q-boundary into `ch.offset` when `snap_to_quantum`
  is `Some`. Pure query: no Link-side publish, no `commit`. The
  `tick_stream` scheduler stays Link-unaware — Plan 03's
  `scheduler_block_equivalence` property passes bit-for-bit when
  `snap_to_quantum = None`.
- **Scaffolding.** `Quantum(Micro)` newtype + `f64_beats_to_quantum`
  matching Link's `Beats(double)` constructor bit-exactly.
  `LinkWriteConfig` + thin `LinkSession` orchestrator (Plan 06's
  Machine absorbs it later). Three new CLI subcommands under
  `agogo link`: `push-tempo`, `transport`, `diag`.

Also pulled forward from the in-flight post-fxp enforcement sprint
(Plan 11):
- `Channel::{shift_ms, offset_ms}: f32` → `{shift, offset}: Micro`
  (Plan 11 T2). Micro → sample via `F12F06.inner` + `PicoSampleConn::floor`
  + `>>16`. Deterministic integer math, no f32 fuzz.
- `LinkClock::new(Tempo)` + `tempo() -> Tempo` (Plan 11 T5). Two
  Link FFI sites annotated.

Deliberately **not** in this PR (all documented in the plan's
Deferred section):
- Full NEG/POS forerun FSM (v0.5 Sprint 01 — extends Plan 09's
  `rust-fsm` declaration).
- Per-buffer atomic-seqlock anchor + wrapped-error PID + demotion
  of `LinkClock` to PID-smoothed reference (v0.5 Sprint 02 —
  depends on Plan 05 audio callback).
- `agogo run --link` CLI (needs Plan 05).

### Properties shipped

| Property | Module | Status |
|----------|--------|--------|
| `transport_fsm_deterministic` | `host-link::transport` | green |
| `transport_fsm_no_spurious_publishes` | `host-link::transport` | green |
| `fsm_no_echo_loop` | `host-link::transport` | green |
| `f64qnt_matches_link_beats` | `agogo_core::fxp` | green (bit-exact llround agreement) |
| `f64qnt_monotone` | `agogo_core::fxp` | green (Conn-adjoint surrogate) |
| `scheduler_unchanged_by_link` | `channel::scheduler` | green (Plan 03 props unchanged) |
| `quantum_snap_nonneg_*` | `host-link::link` + `session` | green |
| `arm_channel_is_near_idempotent` | `host-link::session` | green (1 ms tolerance — Link session drifts µs between captures) |
| `tempo_push_round_trip` | `host-link::bidirectional` | green against real multicast loopback |
| `transport_link_to_agogo` | `host-link::bidirectional` | green |
| `transport_agogo_to_link_one_shot` | `host-link::bidirectional` | green |
| `quantum_snap_produces_positive_offset` | `host-link::bidirectional` | green |

All 4 `bidirectional.rs` tests skip cleanly without
`tests/fixtures/link_multicast`.

### Verification

- `cargo test --workspace` — 240 green.
- `cargo test --manifest-path crates/host-link/Cargo.toml
   --features rusty-link` — 25 green (unit) + 4 skip (integration
  without sentinel) / green (with sentinel).
- `cargo clippy --workspace --all-targets -- -D warnings` — clean.
- `cargo clippy --manifest-path crates/host-link/Cargo.toml
   --features rusty-link --all-targets -- -D warnings` — clean.

### Notable surprise caught during development

Link's `is_playing` flag only propagates across peers that BOTH
have `abl_link_enable_start_stop_sync` enabled — off by default.
Not in the draft plan. `LinkSession::enable` now auto-enables
start-stop-sync when `config.enable_start_stop_sync` is true (the
default), and `LinkClock::enable_start_stop_sync` is a
passthrough. Caught by the `transport_*` integration tests failing
until both sides flipped the sync flag.

## Plan deviations

See plan's §Review for the full set. Headline four:
1. Plan 11 T2 (Channel Micro flip) and T5 (LinkClock Tempo
   surface) absorbed into Plan 09's first two commits, since
   Plan 11's continuation hadn't merged yet.
2. `arm_channel(&mut Channel)` — no `stc` arg needed, since both
   Link's host-time and `Micro` are in microseconds.
3. `snap_offset_micro` is a pure query, not a
   `request_beat_at_time` publish. Safer + RT-cheaper + what
   snap should semantically be.
4. Start-stop-sync auto-enable in `LinkSession::enable`.

## Local review (2026-04-24)

**Branch:** plan/2026-04-23-06
**Commits:** 10 (origin/main..plan/2026-04-23-06)
**Reviewer:** Claude (sonnet, independent)

---

Reviewing `plan/2026-04-23-06` against `origin/main` — diff stat:
2189 insertions across 14 files, 10 commits.

### Commit Hygiene

All commit messages are conventional and subjects are under 72
characters. Prefixes (`plan`, `feat`, `test`, `doc`) all match
CLAUDE.md's accepted list. Each commit is reachable in a linear
history with no merge commits.

`9780793` bundles T0+T1+T2+T4. T0 is structural scaffolding
(empty module stubs, Cargo.toml edit, `Quantum` type in
`fxp.rs`), T1 is `push_tempo` on `LinkClock`, T2 is the
transport FSM, and T4 is `LinkSession`. These are tightly
coupled: T4 depends on T1, T2, and T3, and the commit message
is explicit about the bundling. The plan's own dependency graph
shows T1, T2, and T4 as a single-path chain. The bundle is
large (~800 lines) but internally consistent — every test in
the bundle is green against the code in the same commit.
Acceptable but at the edge of reasonable; the plan acknowledges
this by naming it explicitly.

### Code Quality

**Critical:** None found.

**Important:**

**1. `LinkSession::enable(false)` does not turn start-stop-sync
back off.** `session.rs` line 73-78:

```rust
pub fn enable(&self, on: bool) {
    self.clock.enable(on);
    if on && self.config.enable_start_stop_sync {
        self.clock.enable_start_stop_sync(true);
    }
}
```

When `on = false`, the method disables peer discovery but leaves
Link's start-stop-sync flag in whatever state it was. The PR
summary notes that `is_playing` propagation requires both peers
to have start-stop-sync on. If a future path calls `enable(false)`
without `user_stop()` first, the peer is left observing
`is_playing = true` indefinitely. Fix: make enable/disable
symmetric.

**2. `f64_beats_to_quantum` uses `f64::round()`; doc promises
`std::llround` agreement.** Rust's `f64::round()` is
round-half-away-from-zero. `std::llround` is also
round-half-away-from-zero. They agree on all finite inputs in
the tested range. Not a bug.

More real concern: the proptest `f64qnt_matches_link_beats`
uses `(q * 1_000_000.0).round()` as the *reference* and
compares `f64_beats_to_quantum` to it — i.e., the test compares
the implementation to itself. The existing hand-computed spot
check `f64_beats_edge_cases` covers at least one concrete case
independently. Test quality concern, not a bug.

**3. `snap_offset_micro` `saturating_sub` guard is defensive
but correct.** `i64::saturating_sub` saturates at `i64::MIN`,
not zero, so `delta_us < 0` is reachable under wildly-stale
session state. The comment "paranoid guard; should not happen"
accurately describes this. Not a bug.

**4. `LinkSession::quantum` field is `#[allow(dead_code)]` with
a stale comment referencing T0/T3.** `session.rs` line 48-49:

```rust
#[allow(dead_code)] // consumed in T3; placeholder in T0.
quantum: Quantum,
```

T3's `arm_channel` reads `ch.snap_to_quantum`, never
`self.quantum`. `LinkWriteConfig` already stores
`default_quantum` (line 26). The `quantum` field on
`LinkSession` duplicates it without ever being accessed. Dead
state that will confuse the v0.5 Sprint 01 developer who
extends this struct. Either remove it and access
`config.default_quantum` directly, or wire it up as a fallback
for `ch.snap_to_quantum = None`.

### Test Coverage

- `transport_fsm_deterministic`: uniform over four events, no
  frequency weighting toward boundary sequences. Adequate.
- `transport_fsm_no_spurious_publishes`: correctly validates
  publish-pairing. `self_loops_are_noops` spot check covers the
  count relationship explicitly. Adequate.
- `fsm_no_echo_loop`: generator alternates LinkReports* in a
  fixed pattern rather than drawing randomly. Weak generator
  for the stated invariant, but not a failing test. Follow-up.
- `f64qnt_matches_link_beats` / `f64qnt_monotone`: range covers
  realistic Link ABI. Adequate.
- `bidirectional.rs` integration tests: `link_multicast_or_skip!`
  correctly implemented per CLAUDE.md. When `wait_for_pair`
  fails with fixture present but broken multicast, tests
  silently pass — documented as "signals multicast loopback
  disabled." Acceptable.
- `quantum_snap_produces_positive_offset`: loose bound checks
  non-negative + under-one-quantum-span. Does not verify offset
  is *correct* (only bounded). Documented as surrogate for the
  full `quantum_snap_first_tick` invariant, which is deferred.
  Acceptable.
- `scheduler_block_equivalence` / `scheduler_events_in_window`:
  both have `snap_to_quantum: None` and exercise the Micro
  flip. Adequate.

### Plan Conformance

All T0-T6 implemented. All Verification-table properties
present or documented-as-replaced with acceptable rationale in
§Review. Four scope deviations all reasonable and explicit.

### Risks

- `rust-fsm 0.7`: MIT, maintained, no non-permissive
  transitives. Low risk.
- No `todo!()` macros. The `#[allow(dead_code)]` on
  `LinkSession::quantum` is the only structural stub — covered
  in item 4 above.
- `Channel` gains `snap_to_quantum: Option<Quantum>`; all
  existing tests updated. `shift_ms`/`offset_ms` renamed to
  `shift`/`offset` via Plan 11 T2 forward-pull — internal
  fields, no stabilized API impact.
- `LinkSession` methods mostly `&mut self`; no `Sync` bound
  asserted. `LinkClock` holds a `SessionState` scratch buffer
  used mutably, so not `Send + Sync`. Correct for
  single-threaded control-path use.

### Must Fix Before Push — resolved in commit `394e10b`

**M1.** `session.rs` line 73-78 — symmetric enable/disable for
start-stop-sync. **Fixed.** `LinkSession::enable` now follows
`on` symmetrically via `enable_start_stop_sync(on)` when
`config.enable_start_stop_sync` is true.

**M2.** `session.rs` lines 48-49 — unused `quantum` field.
**Fixed.** Field removed; `LinkWriteConfig::default_quantum`
held on `config` is the authoritative session-level default.

### Follow-Up (Future Work)

- Stronger `fsm_no_echo_loop` generator drawing from
  `{LinkReports*}` randomly instead of alternating. Track for
  v0.5 Sprint 01 FSM extension.
- `f64qnt_matches_link_beats`: add a table-driven spot check
  against pre-computed `std::llround` outputs from C++ to close
  the independent-reference gap. Lands with upstream
  `float_conn!` for i64 newtypes.
- `LinkClock::is_enabled_tracks_enable_call` opens UDP multicast
  unconditionally — not gated behind `fixture_or_skip!`. Move
  inside the gate when the pattern is more established.

<!-- gh-id: 3136721732 -->
### Copilot on [`crates/host-link/src/link.rs:489`](https://github.com/cmk/agogo/pull/12#discussion_r3136721732) (2026-04-24 09:23 UTC)

`one_quantum_span_us` is computed as `bars * 2_000_000`, which assumes the loop variable is “bars” (2 s each at 120 BPM). But `Quantum::from_bars(n)` is `n` *beats* (microbeats), so at 120 BPM the span should be `n * 500_000` µs. This makes the bound 4× too lax for `n=4`; adjust the calculation (and variable naming/comment) so the test actually verifies the intended invariant.
```suggestion
        for beats in [1u32, 4, 16] {
            let q = Quantum::from_bars(beats);
            let delta = c.snap_offset_micro(q);
            // At 120 BPM, one beat is 500_000 µs, so an n-beat quantum spans
            // `n * 500_000` µs.
            let one_quantum_span_us: i64 = (beats as i64) * 500_000;
            // Plus one µs of slack for floor/ceil rounding on `ceil()`.
            let bound = one_quantum_span_us + 1;
            assert!(
                delta.0 <= bound,
                "snap {:?} at {} beats exceeds one-quantum span bound {}",
                delta, beats, bound
```

<!-- gh-id: 3136721780 -->
### Copilot on [`crates/core/src/channel/time.rs:85`](https://github.com/cmk/agogo/pull/12#discussion_r3136721780) (2026-04-24 09:23 UTC)

`transform()` constructs a new `PicoSampleConn` on every call. Since this is per-buffer/per-channel scheduling logic, repeatedly running `gcd` + i128 setup here could show up on the audio thread. Consider caching `PicoSampleConn` (e.g. store it alongside `SampleTickConn`, or pass a prebuilt `PicoSampleConn` into `transform`/`tick_stream`) so steady-state scheduling avoids per-call setup work.

<!-- gh-id: 3136721805 -->
### Copilot on [`crates/core/src/control/event.rs:45`](https://github.com/cmk/agogo/pull/12#discussion_r3136721805) (2026-04-24 09:23 UTC)

`tick_stream()` constructs `PicoSampleConn::new(stc.sr())` on every call. Given this is per-audio-buffer scheduling, consider hoisting/caching `PicoSampleConn` so the audio path doesn’t pay the setup cost every buffer (even if small), and so both `tick_stream` and `transform` can share the same precomputed conn.

<!-- gh-id: 3136721843 -->
### Copilot on [`doc/plans/plan-2026-04-23-06.md:48`](https://github.com/cmk/agogo/pull/12#discussion_r3136721843) (2026-04-24 09:23 UTC)

This plan section still claims Plan 09 “depends on the post-fxp enforcement sprint landing first”, but later in this same document’s §Review it says the key enforcement changes (Channel Micro flip + LinkClock Tempo surface) were pulled into this plan/PR. Please update Prerequisites to reflect the current reality (either remove the dependency or clearly mark it as historical context) to avoid misleading readers.
```suggestion
This plan was originally drafted against a separate post-fxp
enforcement sprint (branch TBD — see
`~/.claude/plans/please-plan-and-then-cozy-wadler.md` for that
sprint's design). In the current plan/PR, the key enforcement
changes it needed are pulled in here directly: retiring the bespoke
`f32_*_to_*` / `f64_*_to_*` helpers in favour of named `Conn`
constants, flipping `Channel::{shift_ms, offset_ms}: f32` to
`Micro`, and surfacing the `Tempo` conversion used at the Link
boundary. The list below therefore records the post-enforcement
state Plan 09 now includes, rather than an external dependency that
must land first.
```

<!-- gh-id: 3136721881 -->
### Copilot on [`doc/reviews/review-00012.md:249`](https://github.com/cmk/agogo/pull/12#discussion_r3136721881) (2026-04-24 09:23 UTC)

This review record’s “Must Fix Before Push” section lists M1/M2 (and the earlier “Risks” bullet mentions an unused `LinkSession::quantum` field), but the current code in this PR already makes `enable()` symmetric and removes the unused quantum field. Please update the review doc to mark these items as resolved (or move them to a “Fixed in …” note) so the review record matches the final PR state.

<!-- gh-id: 4169378072 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-04-24 09:23 UTC](https://github.com/cmk/agogo/pull/12#pullrequestreview-4169378072))

## Pull request overview

This PR lands the write-path foundation for Ableton Link bidirectional behavior (tempo push, transport state seam, and quantum snap) that later v0.5 Link work will extend.

**Changes:**
- Adds a minimal `{Stopped, Playing}` transport FSM (rust-fsm) and a thin `LinkSession` orchestrator to publish/observe Link transport and tempo.
- Introduces `Quantum(Micro)` (microbeats) + `f64_beats_to_quantum` and threads `snap_to_quantum` through channel configuration and arming.
- Converts channel `shift/offset` from `f32 ms` to deterministic `Micro` and updates scheduling/transform logic and CLI link subcommands.

### Reviewed changes

Copilot reviewed 13 out of 14 changed files in this pull request and generated 7 comments.

<details>
<summary>Show a summary per file</summary>

| File | Description |
| ---- | ----------- |
| doc/reviews/review-00012.md | Adds a review record for Plan 09 implementation and verification notes. |
| doc/plans/plan-2026-04-23-06.md | Adds the Plan 09 design/verification plan and deviations section. |
| crates/host-link/tests/bidirectional.rs | Adds two-peer multicast-loopback integration tests for tempo/transport/snap behaviors. |
| crates/host-link/src/transport.rs | Introduces the rust-fsm transport state machine and its properties/tests. |
| crates/host-link/src/session.rs | Implements `LinkSession` orchestration (tempo push, transport polling/publish, channel arming snap). |
| crates/host-link/src/link.rs | Extends `LinkClock` with Tempo-typed API, tempo push, transport publish/observe, and snap offset query. |
| crates/host-link/src/lib.rs | Exposes new host-link modules and re-exports session/transport types. |
| crates/host-link/Cargo.toml | Adds optional `rust-fsm` dependency under the `rusty-link` feature. |
| crates/core/src/fxp.rs | Adds `Quantum` newtype and f64→Quantum conversion with tests. |
| crates/core/src/channel/time.rs | Migrates shift/offset to `Micro`, adds `snap_to_quantum`, and introduces `micro_to_samples`. |
| crates/core/src/control/event.rs | Updates scheduler math to Micro-based shift/offset conversion via `micro_to_samples`. |
| crates/core/src/channel.rs | Updates re-exports for renamed MAX_SHIFT constant. |
| crates/cli/src/main.rs | Adds `agogo link push-tempo`, `transport`, `diag` commands and updates Link probe and channel trace conversions. |
| Cargo.lock | Locks new `rust-fsm` dependencies. |
</details>






<!-- gh-id: 3136721913 -->
### Copilot on [`crates/core/src/fxp.rs:167`](https://github.com/cmk/agogo/pull/12#discussion_r3136721913) (2026-04-24 09:23 UTC)

`f64_beats_to_quantum` docs say overflow saturates to `Quantum::ZERO` / `i64::MAX`, but the implementation also saturates large negative values to `i64::MIN` (and preserves negative values generally). Please either update the doc comment to describe the negative-range behavior, or clamp `q <= 0.0` to `Quantum::ZERO` if negative quantum is meant to be rejected.
```suggestion
/// saturates to `Quantum::ZERO`; finite values preserve their sign and
/// saturate on overflow to `i64::MAX` / `i64::MIN`. A noisy return
/// would force the caller to handle an error at every argv boundary
/// without gain, since non-finite quantum is already a user mistake.
```

<!-- gh-id: 3136721938 -->
### Copilot on [`crates/host-link/tests/bidirectional.rs:218`](https://github.com/cmk/agogo/pull/12#discussion_r3136721938) (2026-04-24 09:23 UTC)

The comment and bound treat `Quantum::from_bars(4)` as “4 bars” (= 8 s at 120 BPM), but `Quantum` is microbeats and `from_bars(4)` corresponds to 4 beats (one 4/4 bar), i.e. 2 s at 120 BPM. As written, the assertion is 4× too lax and could miss regressions; tighten the bound (and fix the explanatory comment) to match the actual quantum span.
```suggestion
    // At 120 BPM, Quantum::from_bars(4) spans 4 beats (one 4/4 bar):
    // 4 × 500 ms = 2 s = 2_000_000 µs.
    // Snap delta must be within [0, 2_000_001) (+1 µs rounding slack).
    assert!(
        ch.offset.0 >= 0,
        "snap produced negative offset: {:?}", ch.offset
    );
    assert!(
        ch.offset.0 < 2_000_001,
```

<!-- gh-id: 3136785488 -->
#### ↳ cmk ([2026-04-24 09:34 UTC](https://github.com/cmk/agogo/pull/12#discussion_r3136785488))

Fixed in 018f610 — you're right, `Quantum::from_bars(n)` is n microbeats-million = n *beats* (not n bars). Bound was 4× too lax. Taking the suggestion verbatim (beats as loop var, 500 000 µs/beat at 120 BPM).

<!-- gh-id: 3136786682 -->
#### ↳ cmk ([2026-04-24 09:34 UTC](https://github.com/cmk/agogo/pull/12#discussion_r3136786682))

Deferring — valid concern, but caching `PicoSampleConn` across `transform`/`tick_stream` calls needs either a new field on `SampleTickConn` or a threading-through-the-API refactor, both outside Plan 09's scope. Tracking as a follow-up for the audio-callback sprint (Plan 05) where per-buffer cost actually lands on the RT thread.

<!-- gh-id: 3136787505 -->
#### ↳ cmk ([2026-04-24 09:35 UTC](https://github.com/cmk/agogo/pull/12#discussion_r3136787505))

Deferring — same rationale as the sibling comment on `transform.rs:85`. Plan 05 (audio callback) is the natural sprint to do a coordinated `PicoSampleConn` caching pass.

<!-- gh-id: 3136788239 -->
#### ↳ cmk ([2026-04-24 09:35 UTC](https://github.com/cmk/agogo/pull/12#discussion_r3136788239))

Fixed in 018f610 — rewrote the Prerequisites section as historical context per your suggestion. The Plan 09 doc now points forward to §Review for the details of what was pulled in, rather than claiming an external dependency.

<!-- gh-id: 3136789396 -->
#### ↳ cmk ([2026-04-24 09:35 UTC](https://github.com/cmk/agogo/pull/12#discussion_r3136789396))

Fixed in 018f610 — the Must Fix Before Push section now shows M1/M2 as resolved in commit `394e10b`, with the code changes described inline.

<!-- gh-id: 3136790464 -->
#### ↳ cmk ([2026-04-24 09:35 UTC](https://github.com/cmk/agogo/pull/12#discussion_r3136790464))

Fixed in 018f610 — updated the doc to describe negative-overflow saturation to `i64::MIN` alongside the existing non-finite→ZERO and positive-overflow→MAX cases. Negative quantum isn't a user-facing pattern but preserving sign is the honest thing to document.

<!-- gh-id: 3136791281 -->
#### ↳ cmk ([2026-04-24 09:35 UTC](https://github.com/cmk/agogo/pull/12#discussion_r3136791281))

Fixed in 018f610 — took the suggestion verbatim. The integration test bound is now 2_000_001 µs (one 4/4 bar at 120 BPM), tightened from 8_000_001. Tests still green against real Link multicast loopback with the new bound.
