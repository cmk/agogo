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
