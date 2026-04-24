# PR #13 — Delete PicoSampleConn; replace with `fxp::pico_to_samples`

## Summary

Follow-up to PR #11's boundary sweep. `PicoSampleConn` was a
runtime-`sr`-parameterised Conn-lookalike introduced in PR #9
because we wanted a lawful Pico ↔ Sample bridge that worked at
arbitrary runtime rates. It carried its own ~300-line proptest
battery (adjoint laws, monotonicity, saturation, gcd-reduction,
etc.) duplicating what the upstream `connections` crate already
proves for each `F12Sxx` compile-time constant.

Since the set of audio sample rates is small and compile-time
known, a match-dispatch on `sr` to the upstream `F12Sxx` works as
well and reuses the upstream proptests directly. This PR:

- Adds `agogo_core::fxp::pico_to_samples(p: Pico, sr: u32) ->
  Option<i64>` — 10 lines, match-dispatches on `sr` to
  `F12S44..F12S192`, rounds to the nearest whole sample, returns
  `None` for unsupported rates.
- Deletes `PicoSampleConn` from `crates/core/src/time/conn.rs`
  (struct, `new`, `sr`, `ceil`, `inner`, `floor`, `gcd_i128`
  helper) — ~90 lines.
- Deletes the `PicoSampleConn` proptest suite (15 tests:
  4 spot checks, 7 adjoint/monotonicity proptests, 3 saturation /
  gcd / panic tests, plus the triangle agreement check) — ~310
  lines.
- Replaces with a single spot check
  `sample_tick_and_pico_to_samples_agree_at_120bpm_48k` + a
  `pico_to_samples_rejects_unsupported_rate` test — ~25 lines.
- Re-routes `channel::transform::micro_to_samples` and the
  `scheduler.rs` callers through the new free fn. `micro_to_samples`
  now composes `F12F06.inner` + `pico_to_samples`, two compile-
  time entities, and panics on unsupported rate (unreachable in
  practice because `SampleTickConn::new` already rejects those
  rates upstream of every caller).

Net: **~375 lines removed**, no behaviour change at the user-
visible layer. The adjoint laws are still verified — just by
upstream's test suite instead of downstream duplication.

### Design context

This is the structural payoff we discussed: once the runtime
Conn-lookalike's purpose collapses to "dispatch on a small
enumerable set of compile-time known cases," a match statement on
the upstream constants is the honest form. The Conn-lookalike
machinery (gcd reduction, saturation clamp, i128 intermediates,
bespoke proptests) was solving a problem that didn't need to
exist once we enumerated the rates.

Upstream `compose!` macro still pending as the next bit of work —
that's separate and lands against `connections`, not agogo.

## Test plan

- [x] `cargo build --workspace` — clean.
- [x] `cargo test --workspace` — 219 passed (208 core + 11 cli, + 2
  ignored fixture-gated); zero failures; 0.27 s.
- [x] `cargo test --manifest-path crates/host-link/Cargo.toml
  --features rusty-link` — 10 passed.
- [x] `cargo clippy --all-targets -- -D warnings` — clean.
- [x] `scripts/check-floats.sh` — OK.
- [x] `agogo channel trace --bpm 120 --sr 48000 --divider t4
  --frames 4096 --buffers 16` spot check: samples `0, 24_000,
  48_000` at ticks `0, 192, 384`, bit-exact with pre-refactor.
