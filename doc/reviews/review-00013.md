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
  time entities. Panics on unsupported rate — and that panic is
  reachable from any caller that doesn't validate first, since
  `SampleTickConn::new` only asserts `sr > 0` (the audio-rate
  allowlist is a distinct invariant). The CLI's
  `channel_trace::trace` now rejects non-audio rates up front
  with a clear error message; tests and internal callers are the
  remaining "programmer must validate" sites.

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

<!-- gh-id: 4169427407 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-04-24 09:31 UTC](https://github.com/cmk/agogo/pull/13#pullrequestreview-4169427407))

## Pull request overview

This PR removes the runtime `PicoSampleConn` Pico↔sample bridge (and its large local proptest suite) and replaces it with a small runtime `sr` dispatch function that delegates to upstream `connections` compile-time `F12Sxx` conversions.

**Changes:**
- Add `agogo_core::fxp::pico_to_samples(Pico, sr) -> Option<i64>` that match-dispatches on `sr` to `F12S44..F12S192`.
- Delete `PicoSampleConn` and its associated proptests from `crates/core/src/time/conn.rs`, replacing them with a small agreement spot-check.
- Route `channel::transform::micro_to_samples` (and scheduler usage) through the new `pico_to_samples` helper and update docs accordingly.

### Reviewed changes

Copilot reviewed 5 out of 5 changed files in this pull request and generated 3 comments.

<details>
<summary>Show a summary per file</summary>

| File | Description |
| ---- | ----------- |
| doc/reviews/review-00013.md | Adds PR review record describing the refactor and test plan. |
| crates/core/src/time/conn.rs | Removes `PicoSampleConn` + large test suite; adds smaller agreement/guard tests using `pico_to_samples`. |
| crates/core/src/fxp.rs | Re-exports `F12Sxx` and adds `pico_to_samples` runtime dispatch helper. |
| crates/core/src/channel/time.rs | Updates `micro_to_samples` to use `pico_to_samples` and documents the new behavior. |
| crates/core/src/control/event.rs | Updates comments to reflect the new conversion path via `pico_to_samples`. |
</details>






<!-- gh-id: 3136768107 -->
### Copilot on [`crates/core/src/channel/time.rs:66`](https://github.com/cmk/agogo/pull/13#discussion_r3136768107) (2026-04-24 09:31 UTC)

`micro_to_samples` claims the unsupported-`sr` panic is unreachable because `SampleTickConn::new` rejects unsupported rates, but `SampleTickConn::new` only asserts `sr > 0` (it doesn't enforce the 6-rate allowlist). As a result, this panic is reachable from e.g. CLI `--sr` inputs and represents a user-visible behavior change vs the old `PicoSampleConn` path (which worked for any positive `sr`). Consider either validating `sr` against the allowlist at the boundary / in `SampleTickConn::new`, or making `micro_to_samples` return `Option/Result` and propagating an error instead of panicking; update the doc comment accordingly.

<!-- gh-id: 3136768132 -->
### Copilot on [`doc/reviews/review-00013.md:36`](https://github.com/cmk/agogo/pull/13#discussion_r3136768132) (2026-04-24 09:31 UTC)

The summary says `micro_to_samples` panics on unsupported rates but that this is unreachable because `SampleTickConn::new` already rejects unsupported rates. In the current code, `SampleTickConn::new` only asserts `sr > 0`, so unsupported positive rates (e.g. 22_050) can still flow through and trigger the panic. Please adjust this write-up (or ensure the allowlist invariant is actually enforced upstream) so the doc matches the code’s real behavior.
```suggestion
  time entities, and still panics on unsupported rates; at
  present that panic remains reachable for unsupported positive
  `sr` values because `SampleTickConn::new` only asserts `sr > 0`.
```

<!-- gh-id: 3136768157 -->
### Copilot on [`crates/core/src/fxp.rs:231`](https://github.com/cmk/agogo/pull/13#discussion_r3136768157) (2026-04-24 09:31 UTC)

`pico_to_samples` is a hardcoded `match` dispatch; upstream proptests cover each `F12Sxx`, but they won’t catch a local mapping mistake (e.g., accidentally dispatching 96k → `F12S88`). Add a small unit test that iterates the supported rates and checks a simple identity like `pico_to_samples(Pico(1_000_000_000_000), sr) == Some(sr as i64)` (and maybe a negative case) to lock in the dispatch table.
```suggestion

#[cfg(test)]
mod tests {
    use super::{pico_to_samples, Pico};

    #[test]
    fn pico_to_samples_maps_one_second_to_sample_rate_for_supported_rates() {
        let one_second = Pico(1_000_000_000_000);
        let supported_rates = [44_100, 48_000, 88_200, 96_000, 176_400, 192_000];

        for sr in supported_rates {
            assert_eq!(pico_to_samples(one_second, sr), Some(sr as i64));
        }
    }

    #[test]
    fn pico_to_samples_returns_none_for_unsupported_rate() {
        assert_eq!(pico_to_samples(Pico(1_000_000_000_000), 44_099), None);
    }
}
```

<!-- gh-id: 3136786622 -->
#### ↳ cmk ([2026-04-24 09:34 UTC](https://github.com/cmk/agogo/pull/13#discussion_r3136786622))

You're right — my "unreachable panic" claim was wrong. `SampleTickConn::new` only asserts `sr > 0`, so `--sr 22050` would flow through and blow up deep inside the transform.

Fixed at the CLI boundary: `channel_trace::trace` now rejects non-audio rates up front with an explicit error message listing the six supported ones. The panic in `micro_to_samples` stays (programmer-error signal for anyone who forgets to validate), but with a tightened doc comment that says the audio-rate allowlist is a separate invariant from `SampleTickConn::new`'s `sr > 0` check.

<!-- gh-id: 3136786830 -->
#### ↳ cmk ([2026-04-24 09:34 UTC](https://github.com/cmk/agogo/pull/13#discussion_r3136786830))

Same fix as the sibling thread — the review-doc claim was inherited from my wrong assumption about `SampleTickConn::new`. Rewrote the paragraph to say the panic is reachable from non-CLI callers that skip validation, and to note that `channel_trace::trace` is now the CLI's validation point for the audio-rate allowlist.

<!-- gh-id: 3136787042 -->
#### ↳ cmk ([2026-04-24 09:34 UTC](https://github.com/cmk/agogo/pull/13#discussion_r3136787042))

Added — `pico_to_samples_one_second_maps_to_sr` iterates all six supported rates and asserts `pico_to_samples(1 s, sr) == Some(sr as i64)`. A cross-wired arm (e.g. 96k → F12S88) would return `88_200` instead of `96_000` and fail. Paired with a negative test `pico_to_samples_rejects_unsupported_and_zero` that covers unsupported positive rates (22_050, 44_099) and the zero case.
