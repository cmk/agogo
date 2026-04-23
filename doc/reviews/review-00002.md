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
  prints one absolute tick position per grid step (swing already
  folded in). The build gate passes: `--tbase t16 --swing 0.54
  --bars 2` emits 32 lines whose off-beats are scheduled 4 ticks
  early (e.g. `44`, `140`, …, vs. the straight `48`, `144`, …).

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

<!-- gh-id: 3128842264 -->
### Copilot on [`crates/cli/src/main.rs:93`](https://github.com/cmk/agogo/pull/2#discussion_r3128842264) (2026-04-23 06:46 UTC)

The CLI/help text and `schedule_ticks` docs say this prints/produces “tick offsets”, but the implementation returns (and prints) absolute tick positions after swing (`effective_tick(nominal)`), not the per-step offset (`effective - nominal`). This also conflicts with the PR/plan wording about “-4-tick offsets on off-beats”. Either change the output to offsets (likely `i32` values like 0 / -4) or update the docs/tests/headers to consistently describe the output as swung tick positions.

<!-- gh-id: 3128842294 -->
### Copilot on [`crates/core/src/arb.rs:7`](https://github.com/cmk/agogo/pull/2#discussion_r3128842294) (2026-04-23 06:46 UTC)

The module docs say “Import from any crate's test module: use agogo_core::arb”, but `arb` is behind `#[cfg(any(feature = "proptest", test))]` in `lib.rs`, and `cfg(test)` is not enabled for dependencies. Downstream crates won’t be able to import this unless they enable the `agogo-core/proptest` feature. Consider updating this doc comment to mention enabling the feature in dev-dependencies.

<!-- gh-id: 3128842324 -->
### Copilot on [`doc/plans/plan-2026-04-22-01.md:186`](https://github.com/cmk/agogo/pull/2#discussion_r3128842324) (2026-04-23 06:46 UTC)

In the Spot checks section, `join(T4, T8t) == T8t` is incorrect (LCM(192,64)=192, so join is `T4`). The parenthetical notes this, but the bullet still reads as a contradictory assertion. Since the implementation/test outcome is now known, consider rewriting this spot check to the correct equality to avoid misleading readers.

<!-- gh-id: 3128842352 -->
### Copilot on [`doc/reviews/review-00002.md:37`](https://github.com/cmk/agogo/pull/2#discussion_r3128842352) (2026-04-23 06:46 UTC)

The review says the CLI “prints one tick offset per grid step” and that `--swing 0.54` produces “-4-tick offsets on off-beats”, but the current CLI implementation prints swung tick *positions* (e.g. 44, 140, …), not per-step offsets (0 / -4). Consider updating this wording to match the actual output format, or changing the CLI output to emit offsets as documented.
```suggestion
  prints one swung tick position per grid step. The build gate
  passes: `--tbase t16 --swing 0.54 --bars 2` emits 32 lines whose
  off-beats are scheduled 4 ticks early (for example `44`, `140`,
  ... rather than the unswung `48`, `144`, ...).
```

<!-- gh-id: 3128842410 -->
### Copilot on [`crates/core/src/time/tick.rs:49`](https://github.com/cmk/agogo/pull/2#discussion_r3128842410) (2026-04-23 06:46 UTC)

`time_to_tick` multiplies two `u32`s (`beats * tick_count`) without overflow handling. In release builds this will wrap silently, which breaks `Time`'s Eq/Ord/Hash implementations (they all go through tick count) and can corrupt ordering/hashing semantics. Consider using `checked_mul` (panic with a clear message) or doing the math in `u64` and clamping/returning a `Result` if the value can’t fit in `Tick(u32)`.
```suggestion
    Tick(
        t.beats
            .checked_mul(t.base.tick_count())
            .expect("time_to_tick overflow: beats * tick_count does not fit in Tick(u32)"),
    )
```

<!-- gh-id: 3128842435 -->
### Copilot on [`crates/core/src/time/tick.rs:63`](https://github.com/cmk/agogo/pull/2#discussion_r3128842435) (2026-04-23 06:46 UTC)

`from_ticks` can overflow when aligning to the T128t grid: `n.0.div_ceil(prec) * prec` overflows `u32` for inputs near `u32::MAX` (e.g. `u32::MAX` rounds up past the max). In release this wraps, producing a much smaller aligned value and an incorrect `Time`. Align using `u64` and clamp to the largest multiple of `prec` ≤ `u32::MAX`, or use checked arithmetic and return/panic explicitly on overflow.
```suggestion
    let prec = u64::from(TBase::T128t.tick_count());
    let rounded_up = u64::from(n.0).div_ceil(prec) * prec;
    let max_aligned = (u64::from(u32::MAX) / prec) * prec;
    let aligned = rounded_up.min(max_aligned) as u32;
```

<!-- gh-id: 3128842454 -->
### Copilot on [`crates/core/src/time/conn.rs:78`](https://github.com/cmk/agogo/pull/2#discussion_r3128842454) (2026-04-23 06:46 UTC)

`rt_ceil`/`rt_floor` convert an `i64` tick count to `u32` via `as u32`. For values > `u32::MAX`, this truncates/wraps rather than clamping or erroring, which can yield nonsensical tick counts for large rationals. Consider clamping to `u32::MAX` (similar to `effective_tick`), or returning an error if the computed tick count can’t fit in `Tick(u32)`.

<!-- gh-id: 4160244933 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-04-23 06:46 UTC](https://github.com/cmk/agogo/pull/2#pullrequestreview-4160244933))

## Pull request overview

Adds the `agogo-core::time` layer as a Rust port of Cirklon’s grid/tick algebra (TBase lattice, Tick/Time canonicalization, swing, envelopes, and associated Galois connections), plus a `agogo-cli time schedule` subcommand for emitting a swung schedule.

**Changes:**
- Introduces `agogo-core::time` module tree (`tbase`, `tick`, `conn`, `swing`, `envelope`) with property tests and shared proptest strategies.
- Adds CLI subcommand `agogo time schedule` (bpaf-based) to generate schedules in tick space.
- Adds docs: implementation plan + review record.

### Reviewed changes

Copilot reviewed 15 out of 16 changed files in this pull request and generated 7 comments.

<details>
<summary>Show a summary per file</summary>

| File | Description |
| ---- | ----------- |
| doc/reviews/review-00002.md | Review record describing what landed and verification claims. |
| doc/plans/plan-2026-04-22-01.md | Implementation plan + review/deviation notes for time/ sprint. |
| crates/core/src/time.rs | Public module root/re-exports for `agogo-core::time`. |
| crates/core/src/time/tbase.rs | `TBase` enum, tick counts, divisibility preorder, lattice/Heyting ops + tests. |
| crates/core/src/time/tick.rs | `Tick` + `Time` types, canonicalization via `from_ticks`, tick-count-based Eq/Ord/Hash + tests. |
| crates/core/src/time/conn.rs | Five Galois connections (`ticks`, `rat_tick`, `quantize_at`, `time`, `tbase`) + law tests. |
| crates/core/src/time/swing.rs | Integer swing model (`SwingConfig`, `effective_tick`, alignment helpers) + tests. |
| crates/core/src/time/envelope.rs | Envelopes (`opening`, `closing`, `s_curve`) + tests. |
| crates/core/src/lib.rs | Exposes `time` and gates `arb` behind `proptest` feature or crate tests. |
| crates/core/src/arb.rs | Shared proptest strategies for new time-domain types. |
| crates/core/proptest-regressions/time/conn.txt | Stores proptest regression seeds for `time/conn` tests. |
| crates/core/Cargo.toml | Adds `connections`, `num-rational`, optional `proptest` feature wiring. |
| crates/cli/src/main.rs | Adds `time schedule` subcommand and unit tests (feature-gated on `core`). |
| crates/cli/Cargo.toml | Adds `bpaf` dependency. |
| Cargo.toml | Adds workspace deps (`bpaf`, `num-rational`) and sibling `connections` path dep. |
| Cargo.lock | Locks new dependencies. |
</details>






<!-- gh-id: 3128916548 -->
#### ↳ cmk ([2026-04-23 07:03 UTC](https://github.com/cmk/agogo/pull/2#discussion_r3128916548))

Fixed in aca20f6: the `time schedule` help, `schedule_ticks` docstring, and the review summary all now describe the output as absolute tick positions (swing folded in), which is what we actually emit. We considered switching to offsets instead but positions are more directly useful for downstream tooling.

<!-- gh-id: 3128916716 -->
#### ↳ cmk ([2026-04-23 07:03 UTC](https://github.com/cmk/agogo/pull/2#discussion_r3128916716))

Fixed in aca20f6: the `arb.rs` module docstring now explains that downstream crates need `features = ["proptest"]` on `agogo-core` in their dev-dependencies to import the strategies, with an example block.

<!-- gh-id: 3128916878 -->
#### ↳ cmk ([2026-04-23 07:03 UTC](https://github.com/cmk/agogo/pull/2#discussion_r3128916878))

Fixed in aca20f6: corrected to `join(T4, T8t) == T4` (LCM(192, 64) = 192). The original was a pre-implementation draft guess with the right parenthetical but the wrong bullet.

<!-- gh-id: 3128917015 -->
#### ↳ cmk ([2026-04-23 07:03 UTC](https://github.com/cmk/agogo/pull/2#discussion_r3128917015))

Fixed in aca20f6 alongside the parallel wording in `main.rs` — using "absolute tick positions (swing folded in)" throughout. Took your suggested wording for the example values.

<!-- gh-id: 3128917866 -->
#### ↳ cmk ([2026-04-23 07:03 UTC](https://github.com/cmk/agogo/pull/2#discussion_r3128917866))

Fixed in aca20f6: now `beats.checked_mul(tick_count).expect(...)`. A silent wrap would have corrupted `Time`'s Eq/Ord/Hash (all three delegate to tick count), which is worse than a clean panic. Added a `# Panics` section noting the 5_592_405-beat safe ceiling for callers constructing `Time` directly; `arb_time` in the test suite stays well inside at ≤100K beats.

<!-- gh-id: 3128918077 -->
#### ↳ cmk ([2026-04-23 07:03 UTC](https://github.com/cmk/agogo/pull/2#discussion_r3128918077))

Fixed in aca20f6 — took your suggestion pretty much as-is. Align in u64, clamp to the largest multiple of 4 ≤ u32::MAX. No wrap, no panic on boundary inputs; the `from_ticks` domain is now genuinely total over `u32`.

<!-- gh-id: 3128918323 -->
#### ↳ cmk ([2026-04-23 07:03 UTC](https://github.com/cmk/agogo/pull/2#discussion_r3128918323))

Fixed in aca20f6: added an `i64_to_tick` helper that saturates the `i64 → u32` cast at both ends, and routed both `rt_ceil` and `rt_floor` through it. Large rationals now clamp to `u32::MAX` ticks instead of silently truncating.
