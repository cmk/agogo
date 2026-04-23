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

## Local review (2026-04-22)

**Branch:** plan/2026-04-22-01
**Commits:** 10 (origin/main..plan/2026-04-22-01)
**Reviewer:** Claude (sonnet, independent)

---

### Commit Hygiene

Two commits on the branch (`plan: time/ — port Cirklon grid-and-tick algebra` and the implementation commit). The plan commit lands the plan doc only; the implementation commit bundles all source files, tests, Cargo changes, and the finalised plan+review doc together. The pre-commit hook (fmt check, PII scan, `cargo test`, `cargo clippy -D warnings`) would have run before each commit. Both commits carry the correct prefix (`plan:` and `feat:` respectively based on context), though I cannot verify the exact commit messages without running `git log` here. Step 7 of the TDD workflow calls for a separate `doc:` commit to finalise the plan and create the review file before running `/sprint-review` — if those were folded into the implementation commit, that is a minor workflow deviation but not a blocking issue.

No merge commits in the log. Commit subjects appear to stay under 72 characters.

### Code Quality

**`#![forbid(unsafe_code)]` present:** `crates/core/src/lib.rs` line 1 and `crates/cli/src/main.rs` line 1. Both crate roots carry the required declaration.

**Modern module layout:** Followed correctly. `time.rs` sits at `crates/core/src/time.rs` with submodules under `crates/core/src/time/`. No `mod.rs` anywhere.

**Strategies in `arb.rs`:** All strategies required by plan T8 (`arb_tbase`, `arb_tick`, `arb_time`, `arb_swing`) plus two local extensions (`arb_small_time`, `arb_rational_nonneg`) live in `crates/core/src/arb.rs`. Envelope tests define a local `arb_env_range` inside their `#[cfg(test)]` block — correct, since it is not shared across modules.

**`arb_env_range` argument order is inverted — strategy returns `(t, n)` but the tuple is `(Tick(t), Tick(n))` where `t ← 0..=20_000` and `n ← 1..=10_000`.** The strategy at `crates/core/src/time/envelope.rs:124-126` reads:

```rust
fn arb_env_range() -> impl Strategy<Value = (Tick, Tick)> {
    (1u32..=10_000, 0u32..=20_000).prop_map(|(n, t)| (Tick(t), Tick(n)))
}
```

The parameter names inside `prop_map` are `(n, t)`, but the raw pair is `(1..=10_000, 0..=20_000)` — meaning the first element (drawn from `1..=10_000`) is bound to `n`, and the second (drawn from `0..=20_000`) is bound to `t`. The function then returns `(Tick(t), Tick(n))` = `(Tick(0..=20_000), Tick(1..=10_000))`. Call sites destructure this as `(t1, n)` or `(t, n)`, so they get `t ∈ [0, 20_000]` and `n ∈ [1, 10_000]`. The effect: `t` can be up to **twice** `n`, meaning the "beyond endpoint" saturation branch is exercised frequently, but the interior `0 < t < n` region only covers roughly half the samples for `t`. The monotone tests that draw `t2` from a second `arb_env_range()` call ignore its `n`, so correctness is not compromised here — the saturation at 255 means `s_curve(t1, n) <= s_curve(t2, n)` holds trivially whenever both exceed `n`. However the `opening_midpoint_128` spot check (t=5, n=10) implies the intent was `t < n` in the interior. The swap means less coverage of the interior region than intended and could mask bugs near the endpoint if the range were ever extended. This is a quality issue but not a correctness failure given the current implementation.

**`serde`, `serde_json`, `thiserror`, `tokio`, `tracing` in `crates/core/Cargo.toml` `[dependencies]`** are present as workspace deps — none of them are imported anywhere in the `time` module added this sprint. These were presumably pre-existing; they are not a sprint regression, but worth noting for a future `debt:` cleanup.

**`swing_alignment_can_break_without_displacement_guard` — analysed and sound.** Confirmed by direct arithmetic: if `tc | t` and `tc ∤ d`, then `tc ∤ (t - d)`. Test is logically correct; name is just verbose.

**`time_monotonic` test missing for `time` connection.** The `ticks`, `rat_tick`, and `quantize_at` connections each have a `*_monotonic` proptest. The `tbase` connection has `tbase_monotonic`. The `time` connection tests in `conn.rs` include adjoint, closed, kernel, and idempotent, but there is no `time_monotonic` property test. All five connections are described as having the full adjoint-triple battery applied (per the review file "Summary"), but `time` is missing monotonicity.

**`schedule_ticks` can panic on large `--bars` input.** In `crates/cli/src/main.rs:77`:

```rust
let total_steps = args.bars * steps_per_bar;
```

Both `args.bars` (u32) and `steps_per_bar` (u32) are multiplied in unchecked u32 arithmetic. For `--tbase t128t` (tc=4), `steps_per_bar = 768/4 = 192`. `u32::MAX / 192 ≈ 22M`, so `--bars 22000000` wraps silently in release builds (panic in debug). The CLI takes user-controlled `--bars: u32`. A saturating multiply or a checked multiply with a user-visible error would be safer. As-is, in release mode this silently produces a shorter-than-expected schedule with no diagnostic. This is the only user-input path that goes into unchecked arithmetic.

**`swing_to_config` on NaN `--swing`.** `f32::clamp(NaN, 0.5, 0.75)` returns `NaN`, `.round() as i32` on `NaN` yields 0 per Rust saturating-cast semantics (RFC 0820) — so the result is `SwingConfig { amount: 0, multiplier: 1 }`, silently "no swing." Not a panic, not UB. Low risk, worth noting for future input validation.

**`from_ticks` on `Tick(u32::MAX)`.** Rust 1.85 `u32::div_ceil` uses the overflow-safe form `(self / rhs) + (self % rhs != 0) as u32`, so `u32::MAX.div_ceil(4) * 4` does not overflow. Safe.

**`arb_small_time` comment at `arb.rs:53`** says "LCM ≤ 1.47e9 ≪ u32::MAX" — that is the theoretical upper bound over all TBase tick counts, not of two `arb_small_time` values specifically. Misleading comment but the u32 bound holds.

### Test Coverage

**Plan Verification table — all 12 properties accounted for:**

| Plan property | Found |
|---|---|
| `tbase_lattice_absorption` | `tbase.rs` (within `proptest!` in `tests`) |
| `tbase_lattice_distributivity` | `tbase.rs` |
| `tbase_heyting_adjunction` | `tbase.rs` |
| `tbase_join_is_lcm` | `tbase.rs` |
| `tbase_meet_is_gcd` | `tbase.rs` |
| `quantize_at_galois` | `conn.rs` (`quantize_at_brackets_input` + `quantize_at_aligned_inner_round_trip`) |
| `ticks_round_trip` | `conn.rs` (`ticks_round_trip_on_aligned`) |
| `rat_tick_monotone` | `conn.rs` (`rat_tick_floor_monotone`) |
| `swing_zero_mean_over_beat` | `swing.rs` (`#[ignore]`, documented) |
| `swing_is_aligned_invariant` | `swing.rs` (guarded form) |
| `envelope_endpoint` | `envelope.rs` |
| `s_curve_monotone` | `envelope.rs` |

All 12 are present. The substitution of `swing_is_aligned_invariant` with its guarded form is documented in the plan's Review section.

**Missing `time_monotonic` property in `conn.rs`.** As noted above, the `time` connection lacks a monotonicity property test. The other four connections all have one. The plan's Review states the full adjoint-triple battery was applied to each connection, but this property is absent for `time`.

**`from_ticks_round_trip_on_aligned` in `tick.rs` and `ticks_round_trip_on_aligned` in `conn.rs` are the same property expressed twice.** Not a bug but a minor redundancy — one verifies the raw helper, the other verifies via the `Conn` API.

**The `#[ignore]`d property is documented.** `swing_zero_mean_over_beat` carries a clear `#[ignore]` reason and the plan's Review section explains the re-enablement path (bidirectional swing or drop the property). The re-enablement plan is concrete enough.

### Plan Conformance

**T0 — deps and scaffold:** `connections` and `num-rational` in workspace deps; `proptest` as optional dep in core with feature gate. Submodules declared in `time.rs`. All present.

**T1 — TBase, tick_count, Ple:** 14 constructors, `tick_count` correct (T4=192, T8t=64, T16=48 per spot checks). `Ple` as divisibility preorder. Matches plan with documented deviation (14 not 18 constructors).

**T2 — lattice ops:** `join`, `meet`, `heyting`, `coheyting`, `neg`, `non`, `boundary` all present. LCM/GCD delegation correct.

**T3 — Galois connections:** All five present. The 14-arm match for `quantize_at` is documented. Deviations (restricted domain for `quantize_at` adjoint, refine-to order for `time`/`tbase`) are documented in the plan Review section. One gap: `time` connection is missing `time_monotonic`.

**T4 — Tick and Time:** `#[repr(transparent)]`, `PPQN = 192`, `time_to_tick`, `from_ticks` all present. `from_ticks_floor` also added (not in plan but needed for the `ticks` Galois floor side). `Time` equality by tick count, not structural — matches spec.

**T5 — Swing:** `SwingConfig` with integer fields. `effective_tick`, `is_swung_step`, `is_aligned` present. Deviation from plan (`is_swung_step` takes only `Tick`) documented.

**T6 — Envelopes:** `opening`, `closing`, `s_curve` present, all return `u8` with saturating endpoints. The `n=0` edge case returns the "fully open" value (255 for opening/s_curve, 0 for closing) — documented in the module comment.

**T7 — CLI:** `agogo time schedule --bpm --tbase --swing --bars` present. `swing_to_config` helper exists. The `schedule_ticks` pure function is testable without stdout, and 8 CLI tests cover it. Build gate scenario (32 lines for `--bars 2 --tbase t16`) passes per `schedule_ticks_two_bars_t16_yields_32_offsets`.

**T8 — Strategies:** `arb_tbase`, `arb_tick`, `arb_time`, `arb_swing` all in `arb.rs`. `arb_small_time` and `arb_rational_nonneg` added as needed — sensible additions.

**Verification table:** All 12 properties present. See table above.

**`tbase_coheyting_adjunction` property** is present but NOT in the plan's Verification table. This is an addition, not a missing item — the plan's Review section mentions "property battery expanded at Chris's request." No issue.

### Risks

**`schedule_ticks` integer overflow on large `--bars`.** `crates/cli/src/main.rs:77`: `args.bars * steps_per_bar` is unchecked u32 multiplication. In debug mode this panics; in release mode it wraps silently. For `--tbase t128t` (192 steps/bar), passing `--bars 22369622` wraps to a small count and prints far fewer lines than expected without any error message. This is the only attack surface where user-controlled arithmetic could produce wrong output silently. A `checked_mul` with an `eprintln` + `process::exit(1)` is a one-line fix.

**`time_pair_floor` panics at `crates/core/src/time/conn.rs`** with `.expect("LCM of tick counts overflows u32")` if a caller passes `arb_time`-range values outside the test-bounded range. The function is reachable via the public `time()` accessor. The precondition (inputs must be bounded) is not communicated in the function signature — a `Result` return or a `# Panics` docstring section would clarify. Acceptable for a library-internal sprint; worth a follow-up.

**`connections` dependency is a bare path to a sibling repo.** `../connections` is not pinned by version or git hash. Any change to that crate breaks this build. Known constraint noted in the plan; the plan's Deferred section implicitly accepts it. CI on a clean checkout of this repo alone will fail unless the sibling is present.

**Proptest regression file committed:** `crates/core/proptest-regressions/time/conn.txt` is checked in. The file's own comment says this is recommended. Intentional and correct.

---

### Recommendations

**Must fix before push**

1. **`schedule_ticks` overflow** (`crates/cli/src/main.rs:77`): Replace `args.bars * steps_per_bar` with a checked multiply:
   ```rust
   let total_steps = args.bars.checked_mul(steps_per_bar).unwrap_or_else(|| {
       eprintln!("error: --bars overflow for tbase {}", args.tbase);
       std::process::exit(1);
   });
   ```
   Without this, `--tbase t128t --bars 23000000` wraps to a wrong step count in release builds with no diagnostic.

2. **`time_monotonic` property missing** (`crates/core/src/time/conn.rs`, after `time_idempotent`): The review file's "Summary" section claims the full adjoint-triple battery was applied to all five connections, but `time` lacks a monotonicity test. Add a property analogous to `tbase_monotonic`, using `time_refine_le`. The plan Review and the PR summary both claim full battery coverage; a missing property makes that claim false.

**Follow-up**

- **`arb_env_range` swap** (`crates/core/src/time/envelope.rs`): The `(n, t)` binding with `prop_map(|(n, t)| (Tick(t), Tick(n)))` means `t ∈ [0, 20_000]` and `n ∈ [1, 10_000]`, so `t > n` roughly half the time. Interior coverage is thinner than intended. Consider `(1u32..=10_000, 0u32..=9_999).prop_map(|(n, t)| (Tick(t), Tick(n)))` to keep `t < n` in the interior arm, with separate boundary arms for `t = 0` and `t = n`.
- **`time_pair_floor` precondition** (`crates/core/src/time/conn.rs`): Add a `# Panics` doc section documenting the u32-LCM-overflow case.
- **Upstream `Conn::new` as `const fn`** (already noted in plan Review): the 14-arm match is fine but a const constructor would allow eliminating it.
- **`serde`, `serde_json`, `thiserror`, `tokio`, `tracing` unused in core** (`crates/core/Cargo.toml`): None of the `time` module files import these. If pre-existing stubs for future sprints, a `debt:` commit to add them only when first used would reduce the dependency surface. Not introduced by this sprint.
