# PR #39 — Fix Display→parse drift on fractional-µs delay

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
in at `proptest-regressions/machine/spec/display.txt`.

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
