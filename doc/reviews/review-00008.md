# PR #8 — Link phase bridge (post-fxp)

## Summary

Completes the read-only half of the Ableton Link integration. PR #6
landed the scaffold + lifecycle-only `LinkClock`; PR #7 aligned its
trait signature to the post-fxp `Phase` type. This PR drops in the
real `phase_at_sample` body so the `PhaseSource::Custom(LinkClock)`
path is fully functional.

### What's new

- **`HostTimeAnchor { host_origin_micros: i64, sample_rate: u32 }`**
  on `LinkClock` carries the static sample-index → host-time mapping.
  `LinkClock::new(initial_bpm, anchor)` is the new constructor;
  `set_anchor` is the runtime setter Plan 09 will promote to an
  atomic-packed per-buffer path.
- **`phase_at_sample` bridge**: `host_micros = anchor.origin + n ×
  10⁶ / sample_rate` in `i128` (multi-day-safe), feeds
  `session.phase_at_time(host_micros, 1.0)`, returns via
  `fxp::f64_phase_to_phase` (handles `rem_euclid` + the "rounds to
  `2^32`" edge case). RT-safe.
- **Tests replacing the `#[should_panic]` guard**:
  `phase_at_sample_returns_valid_phase`,
  `phase_wraps_once_per_beat_at_120bpm_48k`,
  `phase_never_returns_exact_u32_max`,
  `set_anchor_shifts_the_sample_mapping`.
- **Proptest** `phase_delta_matches_tempo`: consecutive phase reads
  advance by the tempo-driven expected ULPs per stride (2²² ULP
  tolerance covers Link's session drift between captures).
- **CLI** `agogo link probe` gains `--sr <u32>` (default 48 000) and
  a `phase` column on every CSV row. Output header is now
  `t_ms,peers,tempo_bpm,phase`.

### Verification

- `cargo build --workspace` — clean (no `link` feature touched).
- `cargo test --workspace` — 204 core + 11 CLI tests green, 1
  pre-existing ignore.
- `cargo test -p agogo-cli --features link` — 12 tests green,
  including the updated `link_probe` smoke test.
- `cargo clippy --all-targets` / `cargo clippy … --features link`
  — both clean.
- E2E local: `cargo run -p agogo-cli --features link -- link probe
  --initial-bpm 120 --sr 48000 --duration-ms 500 --period-ms 100`
  prints five rows of CSV where the `phase` column advances by ~0.2
  per 100 ms at 120 BPM (one cycle per 500 ms beat), wrapping
  smoothly through 0/1.

### Deviations from the plan

See `doc/plans/plan-2026-04-23-05.md` §Review. Three points:

1. Anchor-shift test restructured to use one clock with two
   `set_anchor` calls — two independent `AblLink` instances have
   divergent session states, so cross-session comparisons fail.
2. Proptest named `phase_delta_matches_tempo` (more diagnostic than
   the plan's `phase_monotonic_across_random_anchors` heading).
3. Probe anchor captured via a throwaway `LinkClock` so
   `clock_micros()` can be read before the real clock is built.

### Out of scope / follow-ups

- **Plan 09 (bidirectional)** — tempo push, minimal transport FSM
  via `rust-fsm 0.7`, quantum snap at Channel layer, per-buffer
  atomic-packed host-time re-anchoring, two-peer integration tests
  behind `fixture_or_skip!("link_multicast")`.
- **Link CI** — adding CMake + C++ to the CI image so
  `--features link` runs there too. Separate chore.
- **Tidier `now_micros()`** — the probe's throwaway-clock pattern is
  a smell; a free function wrapping `abl_link_clock_micros` would
  be cleaner if rusty_link exposes the path.
