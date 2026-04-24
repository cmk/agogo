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

### Must Fix Before Push

**M1. `session.rs` line 73-78 — symmetric enable/disable for
start-stop-sync.**

```rust
pub fn enable(&self, on: bool) {
    self.clock.enable(on);
    if self.config.enable_start_stop_sync {
        self.clock.enable_start_stop_sync(on);
    }
}
```

**M2. `session.rs` lines 48-49 — remove the unused `quantum`
field** (or wire it up as the `ch.snap_to_quantum = None`
fallback). As-is it duplicates `config.default_quantum` and
the `#[allow(dead_code)]` suppresses a warning that signals a
genuine omission.

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
