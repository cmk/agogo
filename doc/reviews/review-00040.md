# PR #40 — Fix Display→parse drift on fractional-µs delay

## Summary

`Display for ChannelSpec` emits fractional milliseconds as
`delay={ms_int}.{frac:03}` (e.g. `Micro(116)` → `"delay=0.116"`). The
parser used to fall back to f64 for any input containing a `.`, and
`F064FD06.ceil` rounded the result up by one µs because `0.116 ×
10⁻³` doesn't represent exactly in f64. That made the
`spec_round_trip` proptest blind to the fractional-µs Display path —
the generator dodged the bug by producing only whole-ms values
(`0..=300 ms` integer-only).

This PR widens the proptest generator to expose the bug, then fixes
the parser:

- **`parse_decimal_ms`** (parser.rs): new string-decimal fast path
  that splits on `.` and parses both halves as integers. Sidesteps
  f64 entirely for the canonical `int.frac` form Display emits.
  f64 fallback retained for scientific notation (`1e-3`) and other
  non-canonical inputs.
- **`arb_delay`** (display.rs): replaces the `0..=300 ms` integer
  bound with a `prop_oneof!` that biases toward zero, sub-ms
  values, the `MAX_DELAY` clamp boundary, and the upper edge of
  the integer-fast-path domain (`i64::MAX / 1_000`). Single
  documented hazard remains — the `i64::MAX / 1_000` cap matches
  the parser's `checked_mul(1_000)` overflow check, beyond which
  no path could round-trip exactly.
- **`snap_to_quantum_micro`** generator widened from `i32`-narrowed
  to full `i64`.
- Three new spot tests pin the fix:
  - `display_round_trip_fractional_us_no_drift` — replays the
    failure mode for a handful of explicit fractional-µs values.
  - `parse_decimal_ms_exact` — boundary cases for the new helper
    (3-digit, shorter-pad, longer-truncate, decline-to-fall-through).
  - `parse_delay_decimal_no_f64_drift` — hits the user-facing
    `delay=0.116` input.

The auto-saved proptest regression seed for `Micro(116)` is checked
in at `proptest-regressions/channel/spec/display.txt`.

### Verification

| Check | Result |
|---|---|
| `cargo test --workspace --all-features` | 943 + 39 + 39 + 1 (was 940 lib; +3 new tests) |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | clean |
| `scripts/check-floats.sh` | OK (no allowlist change — the new `parse_decimal_ms` is integer-only) |

### Background

Surfaced as a deferred Copilot comment on PR #38 and pre-existed in
the codebase (the parser's old comment even called out the drift:
`micro_from_user_ms(51.0)` ceils to 51_001 µs). The original
proptest never reached the fractional-µs path because the generator
only emitted whole-ms values.

### History

This PR was originally opened as PR #39 from `plan/2026-04-28-07`,
but that branch slot had already been claimed by a parallel agent
for unrelated "three-mess cleanup" work. PR #39 has been closed and
the branch reset to its original state (just the plan commit) so
the other agent can resume. The fractional-ms fix moves to
`plan/2026-04-28-08` here.

The four Copilot inline comments on PR #39 are folded in:

- [#39 r3157666910](https://github.com/cmk/agogo/pull/39#discussion_r3157666910):
  doc-comment line break in `parse_decimal_ms` — fixed.
- [#39 r3157666944](https://github.com/cmk/agogo/pull/39#discussion_r3157666944):
  comment claimed f64 fallback parses `1_000` (it doesn't — Rust's
  `from_str` doesn't accept `_`); reworded.
- [#39 r3157666956](https://github.com/cmk/agogo/pull/39#discussion_r3157666956):
  unreachable `0 => 0` arm in `match frac_str.len()` (caller already
  rejects empty `frac_str`); removed.
- [#39 r3157666970](https://github.com/cmk/agogo/pull/39#discussion_r3157666970):
  **real bug** — `MAX_US = i64::MAX / 1_000` capped the µs domain
  1000× too low. The parser's overflow boundary is on `ms_int ×
  1_000`, so the µs range that round-trips is `0..=i64::MAX`. Bound
  removed; generator now sweeps the full domain (with `i64::MAX`
  pinned as a `Just(_)` arm).

<!-- gh-id: 3157666910 -->
### Copilot on [`crates/core/src/channel/spec/parser.rs:426`](https://github.com/cmk/agogo/pull/39#discussion_r3157666910) (2026-04-28 22:47 UTC)

The doc comment describing the Display format has an unintended line break: it currently reads `delay={ms_int}. {frac:03}` but `Display for ChannelSpec` emits `delay={ms_int}.{frac:03}` (no space/newline). This makes the comment inaccurate for readers trying to match the canonical form.
```suggestion
/// This is the inverse of `Display for ChannelSpec`'s
/// `delay={ms_int}.{frac:03}` form. f64 cannot represent decimal
/// fractions like `0.116` exactly — `0.116 × 10⁻³` round-trips through
/// `F064FD06.ceil` to `Micro(117)`, one µs above the source value.
/// Splitting on `.` and parsing both halves as integers sidesteps the
/// float entirely.
```

<!-- gh-id: 4192996392 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-04-28 22:47 UTC](https://github.com/cmk/agogo/pull/39#pullrequestreview-4192996392))

## Pull request overview

Fixes `ChannelSpec` Display→parse round-trip drift for fractional-millisecond delays by avoiding `f64` parsing for the canonical `int.frac` form and strengthening the test generators/cases to cover sub-ms values.

**Changes:**
- Add `parse_decimal_ms` in the spec parser to exactly parse `delay=int.frac` without `f64`, retaining `f64` for non-canonical inputs (e.g. scientific notation).
- Widen `spec_round_trip`’s proptest delay strategy (and `snap_to_quantum_micro` domain) to include sub-ms delays and boundary-biased cases.
- Add targeted regression/spot tests and check in a proptest regression seed.

### Reviewed changes

Copilot reviewed 5 out of 5 changed files in this pull request and generated 4 comments.

<details>
<summary>Show a summary per file</summary>

| File | Description |
| ---- | ----------- |
| doc/reviews/review-00039.md | Review record describing the drift bug, fix, and verification results. |
| doc/plans/plan-2026-04-28-07.md | New plan doc for a separate “three-mess cleanup” effort in `crates/core/`. |
| crates/core/src/channel/spec/parser.rs | Introduces exact decimal-ms parsing path and adds parser-focused tests. |
| crates/core/src/channel/spec/display.rs | Adds fractional-µs round-trip regression test and widens proptest delay generation. |
| crates/core/proptest-regressions/channel/spec/display.txt | Adds regression seed capturing the original failing case. |
</details>






<!-- gh-id: 3157666944 -->
### Copilot on [`crates/core/src/channel/spec/parser.rs:433`](https://github.com/cmk/agogo/pull/39#discussion_r3157666944) (2026-04-28 22:47 UTC)

`parse_decimal_ms` comment says the f64 fallback handles inputs like `1_000`, but Rust’s `f64::from_str` / `i64::from_str` don’t accept `_` separators in strings. Either drop the `_` example from the comment or add explicit underscore-stripping support before parsing so the comment matches behavior.
```suggestion
    // signed). Non-canonical forms are left to other parsing paths.
```

<!-- gh-id: 3157666956 -->
### Copilot on [`crates/core/src/channel/spec/parser.rs:444`](https://github.com/cmk/agogo/pull/39#discussion_r3157666956) (2026-04-28 22:47 UTC)

`frac_str` is checked for emptiness just above (`if ... frac_str.is_empty() { return None; }`), so the `match frac_str.len()` arm for `0 => 0` is unreachable. Removing the dead arm would simplify the logic and avoid suggesting that empty fractions are supported (they currently return `None`).
```suggestion

```

<!-- gh-id: 3157666970 -->
### Copilot on [`crates/core/src/channel/spec/display.rs:283`](https://github.com/cmk/agogo/pull/39#discussion_r3157666970) (2026-04-28 22:47 UTC)

`arb_delay`’s `MAX_US` is computed as `i64::MAX / 1_000`, but it’s used as a `Micro` (µs) upper bound. That’s 1000× smaller than the actual max representable `Micro` and doesn’t correspond to the parser’s `ms_int.checked_mul(1_000)` limit (which is in ms). Consider either (a) making the bound `i64::MAX` for µs, or (b) renaming to `MAX_MS` and mapping via `Micro(ms * 1_000)` when you want to target the integer-ms overflow boundary. The current name/comment and the generated domain don’t match.
