# PR #2 — time/: port Cirklon grid-and-tick algebra

## Summary

Delivers `agogo-core::time` as a complete, property-tested Rust port
of the Haskell Cirklon `Control.Cirklon.Type.Time` module, plus an
`agogo-cli time schedule` subcommand. Pure logic only — no I/O, no
audio, no tempo coupling. Tempo/sample-rate integration is
deliberately deferred (see plan-2026-04-22-01 "Deferred").

### What lands

- **`TBase`** — 14-constructor musical time base (straight
  `T1..T64` + triplet `T2t..T128t`) at 192 PPQN. Divisibility
  preorder via `connections::Ple`, closed-form `join` / `meet` on
  tick-count LCM/GCD, Heyting / co-Heyting / negation / co-negation /
  boundary. Distributive finite lattice; not Boolean.
- **`Tick` / `Time`** — `#[repr(transparent)] Tick(u32)` + 192-PPQN
  `PPQN` constant; `Time { beats, base }` with equality / ordering /
  hashing by tick count (so `Time{12, T128t} == Time{1, T16}` since
  both are 48 ticks). `time_to_tick` is exact; `from_ticks` matches
  Haskell `fromTicks = ceiling ticks`.
- **Five Galois connections** on `connections::Conn` (bare `fn`
  pointers, no closure capture):
  - `ticks() -> Conn<Tick, Time>`
  - `quantize_at(TBase) -> Conn<Tick, Time>` (14-arm match dispatch)
  - `rat_tick() -> Conn<Whole, Tick>` (`Whole = Rational64`)
  - `time() -> Conn<(Time, Time), Time>` (divisibility lattice)
  - `tbase() -> Conn<(TBase, TBase), TBase>` (divisibility lattice)
- **Swing** — `SwingConfig { amount, multiplier }` (integer-valued),
  `is_swung_step`, `effective_tick`, `is_aligned`.
- **Envelopes** — `opening`, `closing`, `s_curve` (Hermite smoothstep),
  all returning `u8` with saturating endpoints.
- **CLI** — `agogo time schedule --bpm --tbase --swing --bars`
  prints one tick offset per grid step. The build gate passes:
  `--tbase t16 --swing 0.54 --bars 2` emits 32 lines with -4-tick
  offsets on off-beats.

### Verification

142 tests pass (134 core + 8 CLI), one documented `#[ignore]`,
clippy clean at `-D warnings`. Every listed plan-Verification
property is covered. Additional coverage includes the full
adjoint-triple battery (adjoint / closed / kernel / monotonic /
idempotent) from `connections/src/conn.rs` applied to each of the
five connections — an expansion from the three properties the plan
originally named.

### Deviations from the plan

Captured in the plan's Review section. Highlights:

1. `TBase` has 14 constructors per the Haskell, not the 18 the plan
   listed (dotted variants + straight `T128` were in the plan but
   not in the source).
2. `is_swung_step` takes only a `Tick`, matching Haskell; the plan
   had a `&SwingConfig` parameter with no semantics.
3. `swing_zero_mean_over_beat` is `#[ignore]`d: Haskell swing is
   one-sided, so the per-beat offset sum is `-2 * amount *
   multiplier`, not zero. Re-enabling requires a bidirectional swing
   model (API break).
4. `time` / `tbase` connections satisfy adjoint laws under
   "refine-to" order (`a ≤ b ⟺ tc(b) | tc(a)`), not standard
   divisibility; tests use ad-hoc `*_refine_le` helpers.
5. `quantize_at`'s adjoint / kernel laws hold only for `Time` values
   on the target `tb` grid; tests constrain inputs accordingly.
6. Added `FromStr` / `Display` for `TBase` (not in plan, needed by
   the CLI), plus a proptest-feature gate on `arb.rs` so the shared
   strategies don't force `proptest` into downstream release builds.

### Follow-on

Plan 03 (channel transforms: `divider`, `shuffle`, `shift`, `offset`)
is the natural next layer; it composes on top of `Tick` and the
master `ticks` connection. A later integration sprint will add
`SampleTickConn { sr, bpm, ppqn }` for the audio boundary.

### Test plan

- `cargo test --workspace` — 142 passing, 1 ignored.
- `cargo clippy --all-targets -- -D warnings` — clean.
- `cargo run -p agogo-cli -- time schedule --bpm 120 --tbase t16 --swing 0.54 --bars 2`
  prints 32 lines with swing-shifted off-beats.
