# PR #42 — Three-mess cleanup: widen Tick, drop Ple, rename Conns

## Summary

Three intertwined cleanups in `crates/core/`:

**1. Widen `Tick` from u32 to u64.** `time_to_tick`'s
`beats: u32 × tick_count: u32` arithmetic could overflow u32; the
existing `checked_mul().expect()` was a backstop, not a fix, and
`arb_time` further dodged the bug by clamping `beats` to
`0..=100_000` — the proptest-coverage-faking anti-pattern called out
in `feedback_proptest_coverage_faking.md`. With the master counter
widened to u64, the panic path is gone (`u64::from(u32) * u64::from(u32)`
fits in u64 with ~32 bits of headroom) and `arb_time` uses
`any::<u32>()` honestly.

`from_ticks` and `from_ticks_floor` become partial — they return
`Option<Time>` because `Time.beats` stays u32, so `Tick(huge)` values
where `huge / chosen_tc > u32::MAX` have no representable result. The
`ticktime` Conn unwraps under a documented precondition (`arb_tick`
caps at `u32::MAX × Grid::T1.tick_count()`); runtime callers
(transport, scheduler) call `from_ticks` directly and pick their own
out-of-range semantics.

The widening rippled to ~30 cast sites in `swing.rs` (i64 → i128
arithmetic on `tick + amount`), `sample_tick.rs` (u128 saturation
clamp at `u64::MAX`), `envelope.rs` (`linear_u8` / `smoothstep_u8`
widened to u64 with u128 Q-frac internals), and CLI test fixtures.

**2. Remove the `Ple` trait and `crates/core/src/preorder.rs`.**
Upstream `connections` removed `Ple` because the lawful framework
now consumes `Eq + PartialOrd` directly. agogo had kept it because
`Grid`'s divisibility preorder isn't the natural order on its
fields — but `PartialOrd for Grid` was already defined via `ple()`,
so `<=` already meant divisibility. For `Tick`, `U7`, `U4`, `Time`
the derived `PartialOrd` matched `Ple` 1:1.

The only conflict was `TBase`: derived `PartialOrd` ran in
declaration order (`T1 < T2 < … < T256`), opposite of its
divisibility `Ple`. Custom `Ord` / `PartialOrd` impls now mirror
divisibility (`a ≤ b ⟺ a.exp() ≥ b.exp()`), matching the shape Grid
uses. Audit confirmed no external caller relied on the old
declaration-order direction. The 75 `.ple(&x)` call-sites collapse
to `<= x`, six `impl Ple for …` blocks come out, and `preorder.rs`
is deleted.

**3. Rename four `time::conn` Conns to the 8-char convention.**
`ticks → ticktime`, `rat_tick → wholtick`, `time → timetime`,
`grid → gridgrid`. Pair-side Conns duplicate the side name since
the rule is silent on pairs. `quantize_at` keeps its name as a Conn
*constructor* (parametric family) — the module doc spells out the
exemption. Helpers (`*_ceil` / `*_inner` / `*_floor`) and ~50 test
names rename in lockstep. All sites confined to `conn.rs`.

## Test plan

- [x] `cargo test --workspace` — 945 lib tests + 17 + 17 + 1 doctest pass
- [x] `cargo clippy --all-targets -- -D warnings` — clean
- [x] `scripts/check-floats.sh` — no new f32/f64 storage
- [x] `time_to_tick_never_panics` proptest exercises the full
      `(beats: u32, base: Grid)` domain
- [x] `from_ticks_some_at_horizon` / `from_ticks_none_above_horizon`
      / `from_ticks_none_at_u64_max` pin the new partial behavior
- [x] `tbase_divisibility_chain_strictly_ascending` pins
      `T256 < T128 < … < T1`
