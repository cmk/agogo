# PR #17 — Plan 15: 960 PPQN, TBase/Grid split, SwingConfig cleanup

## Summary

Lattice & rhythm-grid extension. The time core moves from v0.1's
192-PPQN, 14-variant `TBase` enum to a 960-PPQN, 36-element `Grid`
product lattice over the binary axis × triplet × quintuplet flags.
`SwingConfig` is reshaped to direct `i8` tick offsets on a binary
resolution, replacing v0.1's `(amount, multiplier)` shape and its
hardcoded T16 detector. Folds Slot 01–03 of `version-0.2.md` plus
the `TBase`/`Grid` split that would otherwise have been Plan 16.

### What changed

- **`agogo_core::time::tick::PPQN` 192 → 960** (T1). One-line bump;
  every call site already imports the const.
- **`time::tbase::TBase`** (T2): contracted to the 9-variant binary
  axis (`T1 … T256`). `tick_count`, `exp` / `from_exp`, `Display` /
  `FromStr`, `Ple` (chain order). Used wherever a binary
  subdivision is required specifically — most prominently
  `SwingConfig.resolution`.
- **`time::grid::Grid`** (new module, T2 + T7): 36-element bounded
  distributive Heyting lattice as a product struct
  `{ n: TBase, t: bool, q: bool }`. Named consts cover every
  element (`T1` … `T256`, `T2T` … `T512T`, `T2Q` … `T512Q`,
  `T2P` … `T512P`). `tick_count` is one expression
  (`BAR / (2^n.exp() · 3^t · 5^q)`). Lattice ops factor
  component-wise. `Display` / `FromStr` are the DSL atom morphism.
  Heyting `a → b` and pseudo-complement `neg(x)` shipped.
  Property-tested for distributivity, absorption, Heyting
  adjunction, and product-of-distributive-lattices closure.
- **`time::conn`** (T2): `quantize_at(g: Grid)` dispatches over all
  36 named consts; the `qa_variant!` macro generates per-const
  `_ceil` / `_floor` fn pairs. The v0.1 `tbase()` lattice
  connection is replaced by `grid() : Conn<(Grid, Grid), Grid>`
  built on `grid::meet` / `grid::join`. Five Conns total:
  `ticks`, `rat_tick`, `quantize_at`, `time`, `grid`.
- **`time::tick::Time { beats, base }`** (T2): `base` is `Grid`,
  not `TBase`. Equality / ordering / hashing remain by tick count.
  Precision floor is now `Grid::T512P.tick_count() = 1` (was
  `TBase::T128t.tick_count() = 4` at 192 PPQN), so `from_ticks` is
  a pure canonicalisation at 960 PPQN.
- **`time::swing::SwingConfig`** (T3): reshaped to
  `{ resolution: TBase, amount: i8 }`. Unified detection rule:
  `t.0 % r.tick_count() == 0 && (t.0 / r.tick_count()) & 1 == 1`.
  Drum-machine sign convention (positive amount delays the
  off-beat). `is_aligned` widened to take `Grid` for any-track
  alignment. The plan-deviation comment from v0.1's
  `is_swung_step` shape is gone — the resolution is back as a
  parameter.
- **`channel::transform` / `channel::scheduler`** (T4): `Channel`
  divider becomes `Grid`. Scheduler's swing-window expansion
  inverts to `swing_d = -(amount as i64)` so the existing
  `min(0)/max(0)` math still holds under the new sign convention.
  `arb_divider_with_bounded_swing` rebuilt over `Grid` with
  resolution pinned to `divider.n`.
- **`machine::spec`** (T2/T3 follow-on): `--ch` mini-language
  drops `swing-mult`, gains optional `swing-res` (binary,
  defaults to `t16`). `swing` field is now `i8`. `div` parses to
  `Grid` so any of 36 lattice elements (`t8q`, `t32t`, `t2p`, …)
  is reachable from the CLI.
- **`time::exact_rates`** (new, T6): three integer-exactness
  proptests covering 48 kHz / 96 kHz at 960 PPQN. Pinned spot
  checks: `120 BPM / 48k = 25 samples/tick`, `125 BPM / 48k =
  24 samples/tick`, `60 BPM / 96k = 100 samples/tick`. A
  `non_divisor_bpm_is_not_exact` sanity check confirms the
  exactness predicate is discriminating.
- **`arb`** (T5): `arb_tbase` (binary 9), new `arb_grid` (full
  36), `arb_swing` over `(arb_tbase, -120i8..=120)`. `arb_time`
  uses `arb_grid` so `Time { beats, base }` picks up the full
  lattice.
- **CLI**: `time schedule --tbase` renamed to `--grid`;
  `--shuffle` range-checked into `i8` at the argv boundary.
  `channel trace` and `midi trace` keep `--divider` (now parses
  to `Grid`). All hardcoded 192-PPQN test expectations scaled
  to 960 PPQN.
- **`host-cpal`, `host-link`**: test fixtures updated for the
  new `Grid` divider + `SwingConfig` shape. No production-path
  change.

### Tests

- **`agogo-core`**: 286 passed, 0 failed, 1 ignored
  (`swing_zero_mean_over_beat`, carried forward from v0.1 with
  the same rationale).
- New properties in `time::swing::tests` (T3): seven structural
  invariants — `swing_amount_zero_is_identity`,
  `swing_only_affects_swung_steps`, `swing_offset_is_exact_i8`,
  `swing_unified_detection_rule`,
  `swing_amount_bound_no_step_collision`,
  `swing_coarse_binary_aligned_unswung`,
  `swing_is_set_difference_of_binary_grids`,
  `swing_is_bar_periodic`, `swing_density_per_bar`,
  `swing_is_order_preserving`,
  `is_swung_step_factors_through_quantize_at_resolution`.
- New properties in `time::grid::tests` (T7):
  `distributive_lattice`, `heyting_pseudo_complement`,
  `meet_is_gcd`, `join_is_lcm`, `meet_join_closure`,
  `absorption`, plus product / track structure
  (`grid_factors_as_product`, `grid_track_factor_structure`,
  `grid_all_36_divisors_of_3840_present`).
- New properties in `time::exact_rates::tests` (T6):
  `stc_samples_per_tick_is_exact_at_48k`,
  `stc_samples_per_tick_is_exact_at_96k`,
  `stc_round_trip_identity_48k_96k`.
- Existing `tbase_*` Conn proptests in `time::conn::tests`
  replaced in place by `grid_*` proptests over `arb_grid`.
- **`agogo-cli`**: 22 passed (`schedule_ticks_*`,
  `swing_to_config_*`, `channel_trace_*`, `sync_trace_converges`).
  `time schedule` proptests scaled from 192-PPQN tick counts to
  960-PPQN.
- **`agogo-host-cpal`**: 10 passed.
- **`agogo-host-link`**: 27 lib + 4 integration passed (with
  `--features rusty-link`).

### Bug fix folded in

`Grid::tick_count`'s if-branches were inverted relative to the
plan formula in the worktree's first draft (`if self.t { 1 } else
{ 3 }` instead of `if self.t { 3 } else { 1 }`). The lib didn't
compile end-to-end at the time so no tests were running to catch
it. The migration's spot-check tests (e.g.
`tick_count_spot_checks`, `from_ticks_240_is_one_sixteenth`)
caught it once they could run; fixed inline.

### Migration footprint

- **20 files modified** in `feat: T1-T5 + T7` commit (1779 +
  1286 lines net). Most of it is mechanical `s/TBase/Grid/` at
  callsites holding any-subdivision values, plus the new
  `time/grid.rs` (570 lines).
- **2 files added** in `test: T6` commit (209 lines).
- One PR — the time core stays coherent across the type rename
  (per the plan's MR-split recommendation).

### Acceptance

- `cargo test --workspace` clean.
- `cargo clippy --all-targets --all-features -- -D warnings`
  clean across `agogo-core`, `agogo-cli`, `agogo-host-cpal`,
  `agogo-host-link`.
- `scripts/check-floats.sh` clean. No new float-storage sites;
  the four allowlisted files from Plan 14 (`machine.rs`,
  `machine/spec.rs`, `host-link/source.rs`, `cli/run.rs`) keep
  their existing exception comments.
- `agogo demo run --bpm 120 --sr 48000 --divider t64t
  --audio-in default --midi-out <port> --duration-ms 2000`
  emits 24 PPQN MIDI clock at 960 PPQN. (Note: the divider name
  shifts at the PPQN bump — at 192 PPQN the same role was
  played by `t32t`. Updated in the plan's build-gate line and
  Review section.)

## Local review (2026-04-25)

**Branch:** `plan/2026-04-24-04`
**Commits:** 4 (origin/main..plan/2026-04-24-04)
**Reviewer:** Claude (sonnet, independent)

---

### Commit Hygiene

All four commits use accepted prefixes (`plan:`, `feat(core):`,
`test(core):`, `doc:`) with imperative subjects under 72 chars. No
merge commits. Atomicity is clean: plan opener, mechanically-uniform
migration in one commit (per the plan's MR-split recommendation),
T6 as a follow-on, doc finalization. Acceptable.

### Code Quality

The migration follows repo conventions cleanly:

- `#![forbid(unsafe_code)]` preserved across all crate roots.
- `scripts/check-floats.sh` clean — no new stored floats outside the
  existing allowlist; the Plan 14 exception files keep their `// argv
  boundary` / `// PCM ABI` markers.
- All numerical conversions go through named `Conn`s (`F64F06`,
  `F12F06`, `pico_to_samples`, `SampleTickConn`) — no bespoke
  hardcoded helpers introduced.
- The `quantize_at` 36-arm `if`-chain instead of `match` is documented
  in the plan's Review section as a structural deviation forced by
  Rust's lack of struct-const pattern matching. Acceptable; the
  `unreachable!` final arm guards against future Grid additions.

Three stale doc comments need fixing before push (see Must Fix below).
The `swing-mult` → `swing-res` CLI rename is a breaking change at the
spec mini-language; documented in the plan's Review section and in
`machine::spec`'s module header, but `run.rs`'s `--ch` flag help text
still advertises the old `swing-mult` key — users following `--help`
will get a hard `UnknownKey` error.

### Test Coverage

All 22 plan-listed properties are present:

- 11 swing properties in `time::swing::tests` ✅
- 7 grid lattice / Heyting properties in `time::grid::tests` ✅
- `channel_subdiv_preserves_phase_960` covered structurally by
  `scheduler_events_in_window` and `scheduler_block_equivalence` over
  `arb_grid()` (the full 36-element set) ✅
- 3 exact-rates properties in `time::exact_rates::tests` ✅

`non_divisor_bpm_is_not_exact` confirms `assert_exact` is
discriminating (137 BPM at 48k yields a non-zero residual), so the
exact-rates proptests aren't passing trivially.

**Test-coverage gap (must fix):** `arb_tick` caps at `1_000_000` with
no `Just(Tick(u32::MAX))` arm. CLAUDE.md is explicit: "Named
boundaries (`i64::MAX`, `i64::MIN`, `0`, NaN, ±∞) go in explicit
`Just(_)` arms with elevated frequency." `swing_offset_is_exact_i8`
documents bounding `t in (200u32..=10_000_000)` to "leave plenty of
headroom" but doesn't add the required spot-check at the un-sampled
boundary. The saturation path is reachable: at `T256` resolution
(tick_count=15), `Tick(u32::MAX)` is on-grid (`u32::MAX % 15 == 0`),
its step index `u32::MAX / 15 = 286_331_153` is odd so it's a swung
step, and `effective_tick` clamps to `Tick(u32::MAX)`. No test
verifies the clamp produces the right value.

### Plan Conformance

T1–T7 all implemented. The four design deviations listed in the plan's
Review section are accurate (swing-mult→swing-res rename, if-chain
dispatch, --tbase→--grid only on time-schedule, demo divider value
shift). No undocumented scope creep. The single-PR recommendation is
appropriate for the mechanically-uniform `s/TBase/Grid/` migration.

### Risks

- **Stale doc comments** (3 sites): `run.rs:41` says PPQN is 192,
  `main.rs:128–130` says T32t is 24-PPQN MIDI clock, `main.rs:288`
  references the removed `multiplier` field. Issues #3 and #4 are in
  `--help` output and will mislead users.
- **`swing-mult` → `swing-res` breaking change** documented in the
  plan Review and module header but not in any user-facing CHANGELOG.
  Project doesn't ship a CHANGELOG yet, so acceptable for now; flag
  for v0.2 release prep.
- **Inverted `Grid::tick_count` formula bug fixed in the migration.**
  No regression concern — the current test suite (spot checks +
  `meet_is_gcd` / `join_is_lcm` proptests) verifies the formula
  exhaustively across all 36 elements. The bug only escaped initial
  review because the worktree didn't compile end-to-end so no tests
  could run.
- **`quantize_at` exhaustion** is unchecked at the type level (Grid is
  a struct); future Grid additions will only signal at runtime via
  `unreachable!`. Acceptable for now; consider a compile-time check
  if Grid grows.
- No unsafe code, no security-sensitive surface in this diff.

### Recommendations

**Must fix before push:**

1. **`crates/cli/src/run.rs:72-73`** — `--ch` flag doc comment
   advertises `swing-mult` (removed) instead of `swing-res`.
   `ChannelSpec::parse` returns `UnknownKey("swing-mult")` for any
   user copying from `--help` output. Replace `swing-mult` with
   `swing-res` (default `t16`).

2. **`crates/core/src/arb.rs` (`arb_tick`)** — add
   `Just(Tick(u32::MAX))` as a frequency-weighted arm. **And** in
   `crates/core/src/time/swing.rs`, add a `#[test]` spot-check for
   `effective_tick` saturation at the upper boundary (e.g. `T256`
   resolution, `amount: 127`, `t = u32::MAX` → assert clamp to
   `Tick(u32::MAX)`). Required by CLAUDE.md's explicit rule on
   bounded-domain proptests.

3. **`crates/cli/src/run.rs:41`** — `PULSE_PPQ` comment ends with
   "(192)"; PPQN is 960. Off-by-five-times misdirection for anyone
   working on the PLL.

4. **`crates/cli/src/main.rs:127-131`** — `midi trace`'s help text
   says 192 PPQN / 8 ticks / `T32t` for 24-PPQN MIDI clock cadence.
   At 960 PPQN it's 40 ticks / `Grid::T64T`. Currently tells users
   the wrong divider name.

**Follow-up (future work):**

- `crates/cli/src/main.rs:288` — comment references removed
  `multiplier` field. Low impact (source-level only, not in
  `--help`); fold into next plan's doc pass.
- Consider a compile-time exhaustiveness check for `quantize_at`
  before Grid is extended in a future plan.
- The "non-binary divider × binary swing-res" combination is
  silently allowed — currently swing fires only when the divider
  tick happens to land on the resolution grid, which for `T8Q × T8`
  is once per bar. Worth either tightening the type to forbid
  mismatches or surfacing a CLI warning. Listed in the plan's v0.3+
  recommendations; tracking.
