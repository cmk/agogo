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
