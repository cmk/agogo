# PR #26 — Sum-typed `Channel` + per-target role enums (audit P3)

## Summary

Replaces the flat `ChannelMode` enum with per-target role enums
(`MidiRole`, `DinRole`, `CvRole`) nested inside a sum-typed
`Channel { Midi | Din | Cv }`. Common fields (`divider`, `shuffle`,
`delay`, `offset`, `bar_multiplier`) extract into a shared
`ChannelCommon` struct.

This is **P3 of the structural-type audit** — the major reshape.
With P1 (`U7`/`U4` newtypes — PR #23) and P2 (drop `snap_to_quantum`
— PR #25) merged, the field set was stable; P3 reshapes the routing
axis itself.

### What changes structurally

Pre-P3, `render_channel_block` dispatched on `ChannelMode` and
silently no-op'd non-MIDI variants — codified by the
`non_clock_modes_are_noop` proptest as the "contract." A
`Channel { mode: ChannelMode::AnalogPulse, ... }` could be passed
to the MIDI renderer and produce zero output without warning.

Post-P3, the renderer is `render_midi_channel(&ChannelCommon,
&MidiRole, ...)`. A `Channel::Cv` variant **literally cannot** be
passed to it — it's a compile error, not a silent no-op. The
`Machine`-level dispatch matches on the outer `Channel` variant
once at the top of the per-channel loop; non-MIDI targets land in a
`Channel::Din { .. } | Channel::Cv { .. } => {}` arm that's
explicitly the no-renderer case (instead of being buried in a
catch-all inside the renderer itself).

### What changed

- **`crates/core/src/channel/role.rs`** (NEW, 156 lines):
  `ChannelCommon`, `MidiRole { Clock, Click(MidiClickConfig),
  Cc(MidiCcConfig) }`, `DinRole { Sync24 }`, `CvRole { Pulse, Lfo
  }`, plus `MidiClickConfig` / `MidiClickAccent` / `MidiCcConfig`
  (moved from the deleted `mode.rs`).
- **`crates/core/src/channel/transform.rs`**: `Channel` becomes a
  sum enum with `common()` / `common_mut()` accessors. `transform()`
  takes `&ChannelCommon` (not `&Channel`).
- **`crates/core/src/channel/scheduler.rs`**: `tick_stream` and
  `tick_stream_into` take `&ChannelCommon`. Tests use
  `ChannelCommon` directly — no `Channel` wrapper needed since the
  scheduler doesn't care about role.
- **`crates/core/src/out/midi.rs`**: `render_channel_block` is
  **deleted**; replaced by `render_midi_channel(&ChannelCommon,
  &MidiRole, ...)` which is exhaustive on `MidiRole`. The
  `non_clock_modes_are_noop` proptest is deleted (structurally
  unrepresentable now); replaced by `cc_role_is_noop_until_v02`
  for the one remaining no-op MIDI role (`Cc(_)` v0.2+ stub).
- **`crates/core/src/machine.rs`**: `Machine::on_buffer` per-channel
  loop now `match`es on the outer `Channel` variant. The
  `Channel::Midi { common, role }` arm calls `render_midi_channel`;
  `Din` / `Cv` arms are explicit no-renderer.
- **`crates/core/src/machine/spec.rs`**: `ChannelSpec.mode: MidiRole`
  (was `ChannelMode`). `into_channel` returns `Channel::Midi { ... }`
  — the parser only ever produces MIDI roles today.
- **`crates/core/src/channel/mode.rs`** is **deleted**.
- **All `Channel { mode: ChannelMode::X, ... }` literals** (test
  fixtures across 7 files) migrate to
  `Channel::Midi { common: ChannelCommon { ... }, role: MidiRole::X }`.
  67 `ChannelMode::*` references collapse to zero across the
  workspace.
- **`crates/cli/src/main.rs`**: `midi_trace` and `channel_trace`
  modules use the new shape; `demo` constructs
  `Channel::Midi { ... }`.
- **`crates/host-link/tests/bidirectional.rs`**: integration test
  builds the new variant; snap delta application uses
  `ch.common_mut()`.
- **`crates/host-cpal/src/cpal/callback.rs`**: test fixture
  migrates.

### What did not change

- **CLI surface**: `--ch dev=midi[,mode=click,note=N,vel=N,...]`
  parses identically. No user-facing flag rename or removal.
- **MIDI byte output**: every existing test's expected byte stream
  is preserved bit-for-bit.
- **`MidiSink::send_at(&[u8])` trait signature** stays untyped
  (audit finding E remains deferred until renderer signatures
  drive a typed-message migration in a separate PR).

### Why this is the right shape

- **Make illegal states unrepresentable.** A `Channel::Cv` can't be
  passed to `render_midi_channel`; the type system says no. The
  pre-P3 `non_clock_modes_are_noop` proptest existed because the
  type system *couldn't* say no — it was codifying a runtime
  workaround as a contract. P3 turns the workaround into a
  compile-time guarantee.
- **Locality of change at renderer dispatch.** The Machine-level
  match is one site; renderer narrowing happens once per target,
  not once per `(target × role)` pair like the old flat enum.
- **Migration path for v0.4 audio.** Adding `Channel::Audio { common,
  role: AudioRole::Click(AudioClickConfig) }` later is one variant
  + one renderer; nothing else needs to change.

### Phasing context

| Phase | Status |
|-------|--------|
| P0a — Conn-discipline sweep | DEFERRED (upstream `Conn::then`) |
| P0b — Float surface area | DEFERRED (depends on P0a) |
| P1 — U7 / U4 newtypes | merged (PR #23) |
| P2 — drop `Channel.snap_to_quantum` | merged (PR #25) |
| **P3 — sum-typed `Channel` + role enums** | **this PR** |
| P4 — sum-typed `ChannelSpec` | next |
| P5 — host-link decoupling | after P4 |
| P6 — host-cpal output typing | opportunistic |

### Test plan

- [x] `cargo test --workspace` green (484 passing, 2 ignored).
- [x] `cargo clippy --all-targets -- -D warnings` clean.
- [x] `cargo build --workspace --all-targets --features link`
  clean (host-link integration test compiles).
- [x] `cargo build --workspace --all-targets --features demo`
  clean (CLI demo path compiles).
- [x] `scripts/check-floats.sh` exit 0 (no f64 changes).
- [x] `scripts/check-pii.sh` clean.
- [x] No `ChannelMode` symbol anywhere in the workspace
  (`grep -r "ChannelMode" crates/` returns only doc-comment
  historical references).
- [x] Existing `parse_*`, `tick_monotonicity`,
  `scheduler_block_equivalence`, `bars_filter_*`,
  `bars_and_accent_compose_correctly`, and other proptests all
  continue to pass against the reshape.

Note: this branch was created from `origin/main` before PR #24's
hook fix landed, so the pre-commit hook in this worktree is the
broken old version. The full check chain was run **manually**
before each commit; future plan branches will get the corrected
hook automatically once #24 merges.

## Local review (2026-04-26)

**Branch:** plan/2026-04-26-02
**Commits:** 3 (origin/main..plan/2026-04-26-02)
**Reviewer:** Claude (sonnet, independent)

---

### Commit Hygiene

Three commits. The `plan:` commit lands the plan doc only. The `refactor:` commit is the full implementation — new `role.rs`, deleted `mode.rs`, all migrations, all tests. The `doc:` commit adds the review file and finalizes the plan. Each commit is individually buildable given the description. Commit messages are conventional and concise. Atomic.

Hook-didn't-fire deviation is correctly noted in the plan's Review section with the manual check chain substituted. No concern.

### Code Quality

**Repo conventions — all clean:**
- `#![forbid(unsafe_code)]` is not in this diff but that's a crate-root attribute carried over from before; no new code introduces `unsafe`.
- No f32/f64 stored or introduced. `check-floats.sh` exit 0 is reported in the review file test plan.
- No open-coded unit arithmetic. All timing remains through `micro_to_samples` and the existing Conn chain.
- `thiserror` usage is inherited; no new error types introduced.

**Exhaustiveness of the `ChannelMode` migration (the highest-value structural check):**

The deletion of `crates/core/src/channel/mode.rs` is the compiler-enforced verification. The file is gone from the diff; `pub mod mode;` is absent from `crates/core/src/channel.rs`. Every call site in the diff migrates cleanly. The `review-00026.md` test-plan checklist records `grep -r "ChannelMode" crates/` returning only doc-comment historical references. T6 succeeded: if any live reference remained, the refactor commit would not have compiled. Satisfied.

**Renderer narrowing check:**

`render_midi_channel` takes `_common: &ChannelCommon` and `role: &MidiRole`. Calling it with `&cv_channel.common()` (type `&ChannelCommon`) and `&cv_role` (type `&CvRole`) would fail at the second argument — `&CvRole` is not `&MidiRole`. The compile-time guarantee holds. Satisfied.

### Test Coverage

**`non_clock_modes_are_noop` deletion:** Justified. `Channel::Cv` and `Channel::Din` cannot be constructed to call `render_midi_channel` — the function signature rejects them at compile time. The replacement `cc_role_is_noop_until_v02` is a unit test (not a proptest), which is appropriate: `MidiRole::Cc(_)` is the single remaining no-op arm and there's nothing to vary over in a property test (any `MidiCcConfig` produces the same empty output). The test at `crates/core/src/out/midi.rs` line 488 exercises that arm with real events and a transport byte and asserts `sink.is_empty()`. Satisfied.

**All nine scheduler/transform/render proptests carry over.** The signatures changed from `&Channel` to `&ChannelCommon` but the logic is identical. Test bodies in `scheduler.rs` and `transform.rs` construct `ChannelCommon` directly; the proptest strategies (`tick_monotonicity`, `divider_rate_preservation`, `delay_upper_clamp`, `delay_lower_clamp`, `shuffle_identity_on_even_steps`, `scheduler_events_in_window`, `tick_stream_into_matches_transform_filtered`, `tick_stream_into_no_realloc`, `scheduler_block_equivalence`) are unchanged in assertion. Satisfied.

**`channel_common_borrow_returns_same_address` → `channel_common_borrow_matches_inner_field`:** The plan's verification table names a reference-equality check; the implementation uses value equality instead. The deviation is correctly documented in the plan's Review section. For correctness: `common()` returns a reference obtained directly from a `match` arm destructuring pattern (`Channel::Midi { common, .. } => common`). Rust's pattern binding here binds to the field inside the enum variant — it is a borrow, not a copy. Value equality is therefore sufficient: if `common().divider == Grid::T4`, the field in the variant is `Grid::T4`. There is no hidden copy path. Satisfied.

**`into_channel_clock_returns_midi_variant` and `into_channel_click_returns_midi_variant` spot checks from the plan's Verification table:** These exact test names are absent from the diff. Coverage is provided by adjacent tests — `into_channel_clamps_delay` calls `ch.common()` (implying `ch` is a `Channel::Midi` since that's what `into_channel` returns), and `into_channel_click_maps_mch_to_zero_based` explicitly matches `Channel::Midi { role: MidiRole::Click(cfg), .. }`. The clock variant's `Channel::Midi` shape is implicitly verified by `into_channel_clamps_delay` and `into_channel_negative_delay_clamps_to_zero`. This is thin but not a gap — the compiler enforces the variant, and the tests exercise the field path. Acceptable, but worth noting.

**`bars_filter_and_render_smoke` (machine tests):** `bars_filter_emits_every_nth_grid_event` and `bars_and_accent_compose_correctly` are referenced in the plan's Review section as passing. They use `zero_channel` and `click_channel` helpers that now construct `Channel::Midi { ... }`. Carried over correctly.

**`spec_round_trip`:** `arb_mode()` now returns `impl Strategy<Value = MidiRole>`, generating `MidiRole::Clock | Click(MidiClickConfig)`. The existing `spec_round_trip` proptest compares `parse(spec.to_string()) == spec`. Satisfied.

### Plan Conformance

- **T1** (`role.rs` with `ChannelCommon`, `MidiRole`, `DinRole`, `CvRole`, `MidiClickConfig`, `MidiClickAccent`, `MidiCcConfig`): Present. File is 162 lines with full field docs.
- **T1b** (`channel/mod.rs` re-exports, `Channel` enum in `transform.rs` with `common()`/`common_mut()`): Present.
- **T2** (`transform()` and `tick_stream*` take `&ChannelCommon`): Present. Both functions updated; callers use `ch.common()`.
- **T3** (render dispatch split — `render_midi_channel` + Machine dispatch): Present. `render_channel_block` is deleted; `Machine::on_buffer` has the three-arm match.
- **T4** (`spec.rs` `into_channel` returns `Channel::Midi`, `ChannelSpec.mode: MidiRole`): Present and correct.
- **T5** (67-reference sweep): The deletion of `mode.rs` confirms all references were cleared. 10 files touched.
- **T6** (`mode.rs` deleted): Present.

All six documented design deviations match actual code. All four verification-table properties are covered. All spot checks from the plan's Review section are present by the names listed there.

### Risks

**`MidiRole` is not `#[non_exhaustive]` — but that's correct.** The current `render_midi_channel` match is exhaustive over three arms (`Clock`, `Click`, `Cc`). Adding a new variant to `MidiRole` in a future sprint would be a compile error in `render_midi_channel` — the match would be non-exhaustive and rustc would catch it. This is the right design for a library-internal enum.

The `Channel::Din { .. } | Channel::Cv { .. } => {}` arm in `Machine::on_buffer` is similarly safe: adding `Channel::Audio` later would be a compile error at that match, forcing the dispatch to handle it. Good.

**No `Cargo.lock` changes** are present in the diff. The diff contains no `Cargo.lock` hunk — the `connections` git dep SHA was not re-canonicalized in this branch. Clean.

**No TODOs or placeholder panics** are introduced. The `MidiRole::Cc(_) => {}` and `Channel::Din { .. } | Channel::Cv { .. } => {}` arms are documented stubs with version references, which is consistent with the existing codebase pattern.

**No shell/FS/network-touching code** introduced.

### Recommendations

**Must fix before push:** None. The migration is exhaustive, the tests cover the stated contracts, and no convention violations are present.

**Follow-up (future work):**

- The plan's Verification table names `into_channel_clock_returns_midi_variant` and `into_channel_click_returns_midi_variant` as spot checks. Neither exists by that name. The coverage is real but the test names don't match the spec. This creates a minor audit-trail gap: a future reviewer checking "was this spot check implemented?" would not find it by name. Consider either adding two targeted tests with the plan-spec names or updating the plan doc to use the actual test names. Low priority — the compiler enforces what these tests would assert, so this is documentation hygiene, not a correctness gap.

- `ChannelCommon` does not derive `PartialEq`. The role enums (`MidiRole`, `DinRole`, `CvRole`) and their payload types do. This means `Channel` itself can't derive `PartialEq`, which is fine for now since `Machine` holds channels in a `Vec` and equality is never compared. If a future test needs to assert `ch_a == ch_b`, the missing derive will be a build error rather than a silent surprise. Acceptable for v0.1; worth noting for P4.
