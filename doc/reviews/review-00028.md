# PR #28 — Re-isolate `snap_offset_micro` f64 footprint (audit P5)

## Summary

Extracts the FFI arithmetic out of
`crates/host-link/src/link.rs:LinkClock::snap_offset_micro` into
a private `next_quantum_boundary_us` helper, and replaces the
open-coded `(quantum.0.0 as f64) / 1_000_000.0` unit shift with
a lawful `F64F06.inner(Extended::Finite(quantum.0))` Conn
inverse. Closes audit finding **H** (the f64 footprint
violation) and the M5/N6 Conn-discipline violation **at this
specific call site**.

This is **P5 of the structural-type audit, arithmetic half.**
The structural half (decoupling host-link from
`agogo_core::channel::Channel`) was delivered by P2 (PR #25's
`LinkSession::snap_offset_for(Option<Quantum>) -> Micro`).
P5 is now closed.

### What changed

- **`crates/host-link/src/link.rs`**:
  - `snap_offset_micro` body shrinks from 13 lines to 5: the
    Quantum→f64 conversion + guard + helper handoff +
    saturation tail.
  - The Quantum→f64 conversion uses `F64F06.inner(Extended::Finite(quantum.0))`
    — the 10⁶ unit shift now lives inside the property-tested
    `connections` crate, not at this call site.
  - New private helper `next_quantum_boundary_us(&mut self,
    q_f64, now_us) -> i64` contains the two FFI calls
    (`beat_at_time`, `time_at_beat`) plus the
    `(current/q).ceil() * q` boundary computation.
  - The two-line `if delta_us < 0 { Micro::ZERO } else
    { Micro(delta_us) }` saturation tail collapses to
    `Micro(... .saturating_sub(now).max(0))` (same semantics).
  - Imports gain `Extended`, `ExtendedFloat`, `F64F06` from
    `agogo_core::fxp`.

### What did not change

- **Behaviour**: zero. Every existing test
  (`snap_offset_for_*`, `quantum_snap_*`, the bidirectional
  Ableton Link integration test) passes byte-for-byte.
- **CLAUDE.md float exception 5**: still observed — at the
  public API surface (`snap_offset_micro`), the f64 lives in
  one named statement. The deeper f64 math is contained inside
  one named private function (`next_quantum_boundary_us`),
  which itself lives inside the FFI call zone.
- **Other Tempo→f64 sites at `link.rs:68` and `:160`**: unchanged.
  Those are M3/M5 territory in audit P0a, tabled until the
  upstream `connections` rev bump.

### Why this is the right shape

- **Make the f64 footprint visible at the type level.** The
  public `snap_offset_micro` reads as a 5-line function whose
  shape obviously matches CLAUDE.md exception 5 (one
  Conn-inverse line, then standard guard, then a helper
  handoff). A future reader doesn't have to scan 13 lines to
  audit whether the f64 leaks somewhere.
- **Lawful Conn over open-coded arithmetic.** The 10⁶ constant
  in `F64F06`'s definition is property-tested (`F64F06`'s
  Galois-law proptest battery in the `connections` crate
  covers it). Open-coding `/ 1_000_000.0` here meant a
  duplicated constant outside the test surface.
- **Containment over elimination.** The `(current/q).ceil() *
  q` arithmetic is genuinely f64-domain Link-FFI math — it has
  no Conn equivalent, and shouldn't. P5 doesn't try to
  eliminate it; it just contains it inside a named function so
  the public surface stays clean.

### Phasing context

| Phase | Status |
|-------|--------|
| P0a — Conn-discipline sweep | TABLED until connections rev bump |
| P0b — Float surface area | TABLED, depends on P0a |
| P1 — U7 / U4 newtypes | merged (PR #23) |
| P2 — drop `Channel.snap_to_quantum` + host-link decouple | merged (PR #25) |
| P3 — sum-typed `Channel` + role enums | merged (PR #26) |
| P4 — drop `ChannelSpec.dev` field | merged (PR #27) |
| **P5 — host-link `snap_offset_micro` f64 isolation** | **this PR** |
| P6 — host-cpal output typing | opportunistic, defer until v0.4 |
| Finding E — typed `MidiMessage` for `MidiSink` | deferred |

### Test plan

- [x] `cargo test --workspace` green (485 passing, 2 ignored —
  unchanged from main).
- [x] `cargo test --workspace --features link` green
  (host-link/session unit tests + bidirectional integration
  test pass byte-for-byte).
- [x] `cargo clippy --all-targets --features link -- -D
  warnings` clean.
- [x] `scripts/check-floats.sh` exit 0.
- [x] `scripts/check-pii.sh` clean.
- [x] No new f64 storage; the inline f64 in `link.rs` is still
  allowlisted under exception 5, but its lifetime at the public
  API surface shrinks visibly.

Note: this branch was created from `origin/main` before PR #24's
hook fix landed. Full check chain run **manually** before commit.

## Local review (2026-04-26)

**Branch:** plan/2026-04-26-04
**Commits:** 3 (origin/main..plan/2026-04-26-04)
**Reviewer:** Claude (sonnet, independent)

---

### Commit Hygiene

Three commits in conventional order: `plan:` opener, `refactor(host-link):` implementation, `doc:` finalizer. Scopes match the affected crate. All three should be individually buildable since the refactor is a pure extraction with no API surface change. Clean.

### Code Quality

**Conn-substitution semantic equivalence — confirmed correct.** Pre-P5: `(quantum.0.0 as f64) / 1_000_000.0`. Post-P5: `F64F06.inner(Extended::Finite(quantum.0))`. From `connections/src/conn/fixed.rs` lines 232–239, `inner` for any `F64F06`-instance `Extended::Finite(r)` returns `ExtendedFloat::Finite((r.0 as f64) / 1_000_000.0)`. For any finite `Micro` the payload is algebraically identical to the open-coded form.

**`Bot/Top` arm reachability — defensive but correct.** `F64F06.inner` only returns `Bot` for `Extended::NegInf` and `Top` for `Extended::PosInf`; `Extended::Finite(_)` always yields `ExtendedFloat::Finite(_)`. The `Bot | Top => return Micro::ZERO` arm is unreachable for any caller-supplied value. Producing well-defined panic-free behavior on a logically unreachable path is acceptable; the inline block comment explains the choice.

**Edge case `F64F06.inner(Extended::Finite(Micro(i64::MIN)))`:** returns a large negative finite f64 (~-9.2e12), falls through to the `q_f64 <= 0.0` guard, returns `Micro::ZERO`. Pre-P5 code did the same. Edge behavior unchanged.

**Saturation tail equivalence — confirmed.** Pre-P5: `let delta_us = saturating_sub(now); if delta_us < 0 { Micro::ZERO } else { Micro(delta_us) }`. Post-P5: `Micro(... saturating_sub(now).max(0))`. `i64::saturating_sub` returns `i64::MIN` on underflow, `.max(0)` clamps to 0 — equivalent for all i64 inputs.

**`next_quantum_boundary_us` extraction — arguments and return value match pre-P5.** The `now` variable in the public function is passed as `now_us` to the helper; the `saturating_sub(now)` in the public function then subtracts the same value that was passed in. Algebraic identity preserved.

**Plan Goal 3 grep marker:** missing from the source. The plan's Review section now documents that the Conn substitution made the marker obsolete (no open-coded shift remains at this site to grep for). Acknowledged deviation, not a code defect.

**Float convention compliance — confirmed.** No new `f64` storage. Three new imports (`Extended`, `ExtendedFloat`, `F64F06`) all used within the method body, no dead-import risk.

### Test Coverage

Plan 23 explicitly does not add new tests, relying on existing `snap_offset_for_*` properties (Plan 20), the `quantum_snap_*` proptest battery on `LinkClock`, and the `bidirectional_snap_arms_within_one_quantum_span` integration test. Given the refactor is a pure structural extraction with no new return paths and the public signature is unchanged, this coverage posture is adequate. The bidirectional test exercises the full path against a real Ableton Link peer.

### Plan Conformance

T1 + T2 (extraction + Conn substitution): delivered as specified. T3 (verify): all listed test names exist and pass. Three documented design deviations all match actual code.

### Risks

`F64F06.inner` at extreme `Micro` values: confirmed safe. `Bot | Top` arm: defensive and correct. No TODOs, stubs, or placeholders. No shell/FS/network surface introduced.

### Recommendations

**Must fix before push:** None.

**Follow-up (future work):**
- The `Bot | Top => return Micro::ZERO` arm could be replaced with `unreachable!()` once the team is confident in the upstream `connections::F64F06.inner` contract. Adding a `debug_assert!(false, ...)` before the `return` would make the intent self-documenting without penalizing release builds. Acceptable as-is; not blocking.
