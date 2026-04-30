# PR #48 — Plan 2026-04-29-01: Reorganize crates/core into five top-level modules

## Summary

Collapse the ad-hoc top-level layout (`boundary`, `channel`,
`dsl`, `host`, `machine`, `midi`, `out`, `sync`, `testing`,
`time`) in `crates/core/src/` into five intent-named layers —
`conn`, `time`, `channel`, `control`, `sink` — plus a `test`
rename of `testing`. The previous seams weren't load-bearing
(`dsl` and `machine/spec` were both channel configuration;
`host` and `out` were both sinks; `sync` and `machine` were both
control plane; `boundary` / `midi` / parts of `time` were all
conn-shaped value types).

The PR also adds a layering rule enforced by a new
`scripts/check-layers.sh` so the structure stays clean. Each
top-level module-root file declares its allowed deps in a
sentinel header comment; the script parses these and fails on
any back-edge in production code (column-0 imports). Test-block
imports inside `#[cfg(test)] mod tests { … }` are allowed to
cross layers because integration tests legitimately need to wire
pieces together.

### What moved (eight atomic commits)

- **T1** — `testing.rs` → `test.rs` (smallest rename, warmup).
- **T2** — `boundary` / `midi` / `time::{decimal, float, sample,
  tempo}` / `sync::phase` → `conn/`. `decimal.rs` renames to
  `fixed.rs`. Phase and Tempo move with the rest of the
  conn-shaped value types so `conn` is a layering leaf.
- **T3** — `sync::sample_tick::SampleTickConn` merged into
  `time::conn` (where the other `Tick`-flavored Galois conns
  live). The "no tempo coupling on time/" prose convention was
  relaxed because Tempo itself is now in conn and the layering
  rule pins the partial order more strictly than prose did.
- **T4** — Per-type `arb.rs` files consolidated into
  `time/arb.rs` and `conn/arb.rs`. CLAUDE.md's
  "Strategies are colocated with the type" rule relaxed to
  "one arb file per top-level module".
- **T5** — `dsl/`, `dsl.rs`, `machine/spec/`, `machine/spec.rs`
  all move under `channel/`. `channel/transform.rs` renames to
  `channel/time.rs`.
- **T6** — `machine.rs` → `control.rs`, `sync.rs` →
  `control/sync.rs`, `sync/` → `control/sync/`,
  `pulse_train.rs` → `pulse.rs`, `host.rs` → `sink/audio.rs`,
  `out/midi.rs` → `sink/midi.rs`, `channel/scheduler.rs` →
  `control/event.rs`. `out.rs` deleted.
- **T7** — Layering enforcement: sentinel headers on each
  module-root, `scripts/check-layers.sh`, wired into
  `.githooks/pre-commit` and `.github/workflows/ci.yml`,
  smoke-tested with an injected back-edge.
- **T8** — Sweep `doc/plans/plan-2026-04-28-*.md`,
  `doc/reviews/`, CLAUDE.md, scripts/ for path references that
  the rename invalidated.

### Why the layering rule

`crates/core/src/lib.rs` shrinks from 11 `pub mod` declarations
to 6. The "can-A-import-B?" question gets a mechanical answer:
just check the declared `depends-on:` list.

```
control  → sink, channel, time, conn
sink     → channel, time, conn
channel  → time, conn
time     → conn
conn     → (leaf)
test     → (leaf)
```

### Verification

- `cargo build --workspace` clean after each of T1..T8 (each
  commit individually green per the no-red-suite rule).
- `cargo test --workspace` — 950 unit + 17 integration + 1 doc
  test, all passing, no `#[ignore]`.
- `cargo clippy --all-targets -- -D warnings` clean.
- `scripts/check-floats.sh` passes (ALLOWED list updated to new
  paths in T2, T5, T6).
- `scripts/check-layers.sh` passes; smoke-tested by injecting
  `use crate::control::sync::pll::Pll;` into
  `crates/core/src/conn/fixed.rs` (a back-edge), confirming the
  script fails; reverting confirms it passes.
- `cargo doc --workspace --no-deps` clean.

### Notes for reviewers

- This is a rename-only PR. No behavioral changes, no API
  consolidation, no new conn types, no `Conn::then` work.
- No compatibility shims for old paths
  (`agogo_core::testing`, `agogo_core::boundary`,
  `agogo_core::time::tempo`, `agogo_core::sync::phase`, etc.)
  — the compiler is the migration tool; downstream call sites
  get updated atomically inside each task's commit.
- T8's path sweep is mechanical; plan-2026-04-28-03's
  narrative description of the `sample_tick → sync` transition
  is intentionally preserved (that plan moved the file *to*
  sync; Plan 2026-04-29-01 T3 reversed it; both statements are
  historically accurate).
- Future-plan refs (plan-2026-04-28-10's `out/audio.rs`
  renderer, plan-2026-04-28-11's `sync/lpf_pid.rs` PID wrapper)
  were rewritten to their new-layout equivalents
  (`sink/audio.rs`, `control/sync/lpf_pid.rs`); the renderer's
  path now collides with `host.rs → sink/audio.rs` and will
  need editorial revision when implementation lands. Flagged
  in the plan's Review section.
