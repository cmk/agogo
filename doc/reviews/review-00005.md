# PR #5 — Link read-only (structural slice)

## Summary

Lands the workspace + API scaffolding for Ableton Link integration in
`agogo` without committing to the numeric shape of
`phase_at_sample`. The fixed-point refactor happening on
`plan/2026-04-23-03` (the companion fxp branch) will settle the
`Phase` / rate-typed `Sample` types; this PR sets up every other
moving part so the follow-up sprint can drop the bridge in place.

### What's new

- **`crates/host-link/`** — new sibling crate wrapping `rusty_link`
  (pinned at `5b3f44e81b0aa30dae4b0650b2f5882048c0b842` via path dep to
  `ext/rusty_link`). `#![forbid(unsafe_code)]`; rusty_link is
  safe-fronted so we never reach for `unsafe` ourselves.
- **`PhaseSourceImpl` trait + `PhaseSource::Custom(...)` variant** in
  `agogo_core::sync::source`. The extension point for sibling-crate
  clock sources (Link first, DIN or MIDI-clock later). Documented
  RT-safety contract; delegate-through in the enum's
  `phase_at_sample` / `feed_samples`.
- **`LinkClock` lifecycle** — `new`, `enable`, `is_enabled`, `tempo`,
  `num_peers`. All reads go through the rusty_link RT-safe
  `capture_audio_session_state` path with a reused `SessionState`
  scratch buffer. Construction and `enable` are documented
  not-RT-safe (socket + thread setup on Link's C++ side).
- **`agogo link probe` CLI subcommand**, feature-gated behind
  `--features link`. Emits CSV `t_ms,peers,tempo_bpm` at a
  configurable period for a configurable duration. No phase column
  yet — added in the follow-up sprint.
- **Workspace plumbing** — `crates/host-link` added as a member,
  `rusty_link` + `agogo-host-link` in `[workspace.dependencies]`,
  `ext/` added to `workspace.exclude` so third-party path deps don't
  drag their upstream doc-tests into our CI.
- **`crates/host-link/README.md`** with setup instructions for cloning
  `rusty_link` at the pinned rev with submodules — `ext/` is
  gitignored, each worktree/CI runner clones it explicitly.

### Deliberately deferred

- **`LinkClock::phase_at_sample` body.** Ships as `unimplemented!()`
  with a matching `#[should_panic]` test. The Tick ↔ Host-Time bridge
  needs the fxp refactor's `Phase` / `Sample` types to settle first.
  Plan 07-b (a follow-up sprint) drops the body in place plus the
  numerical properties.
- **Tempo push-back, transport FSM, quantum snap** — Plan 08 scope.
- **CI for the `link` feature** — requires a CMake + C++ toolchain in
  the CI image plus resolving the gitignored-`ext/` clone step.
  Separate chore; default CI builds stay lean.

### Verification

- `cargo build --workspace` — clean (no `link`).
- `cargo build -p agogo-cli --features link` — clean, links rusty_link.
- `cargo test --workspace` — 186 core + 11 CLI + 6 host-link tests
  green, 1 pre-existing ignore.
- `cargo test --workspace --features link` — adds the `link_probe`
  smoke test (1 more, green).
- `cargo clippy --all-targets --features link -- -D warnings` — clean.
- E2E (manual, local-only): `cargo run -p agogo-cli --features link
  -- link probe --duration-ms 500 --period-ms 100` prints five rows
  of CSV with `peers=0` (no Live/LinkHut on LAN) and tempo stable at
  120 BPM.

### Deviations from the plan

Full list in `doc/plans/plan-2026-04-23-04.md` §Review. Three points:

1. Added `workspace.exclude = ["ext"]` — not in T0's file list, but
   necessary to keep rusty_link's upstream doc-tests out of
   `cargo test --workspace`.
2. `parse_positive_f64` was already in the CLI from Plan 03; reused
   rather than re-added.
3. T4 (tests) merged into T1 and T2 commits per repo convention
   (tests ship with their module).

### Out of scope / follow-ups

- **Plan 07-b (post-fxp phase bridge)** — replace
  `LinkClock::phase_at_sample`'s body with the `HostTimeAnchor` +
  sample → host-µs bridge; add monotonicity and anchor-respect
  property tests; add a `phase` column to the CLI probe.
- **Plan 08 (bidirectional)** — tempo push, minimal transport FSM
  via `rust-fsm 0.7`, quantum snap at the Channel layer, per-buffer
  host-time re-anchoring, two-peer integration tests.
- **Link CI** — separate chore once the CMake + C++ story is ready.
