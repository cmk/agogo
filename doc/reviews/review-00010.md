# PR #10 — Plan 11 sprint opener: rev bump + rename migration

## Summary

Sprint-opener PR for Plan 11 (post-fxp enforcement: boundary sweep +
grep gate + rules — the continuation of Plan 10 / PR #9). Lands the
plan doc and the prerequisite rev bump; the T2–T8 migrations ship in
subsequent PRs on top of this base.

### What ships

- **Plan 11 doc** at `doc/plans/plan-2026-04-23-08.md`. Defines T0
  (rev bump, this PR), T2 (Channel state → `Micro`), T3/T4 (CLI argv
  f64 via Conns + `ProbeRow` drops floats), T5 (`LinkClock` exposes
  `Tempo`), T7 (`scripts/check-floats.sh` grep gate + CI wire), T8
  (CLAUDE.md glossary + rule additions + review-calibration Patterns
  9/10/11). Dependency graph, audit findings copied over from
  plan-07, verification criteria.
- **Rev bump** `connections` `883b4ea` → `ccc4d85`. Picks up three
  upstream refactors landed while PR #9 was open:
  1. `Reorganize src/ — tiers under conn/, fold float_ext into float`:
     `connections::fixed::*` → `connections::conn::fixed::*`;
     `connections::sample::*` → `connections::conn::sample::*`; the
     `float_ext` module collapsed into `conn::float`.
  2. `Rename FloatExt → ExtendedFloat crate-wide`.
  3. `Collapse order.rs into lattice.rs` — `Ple` moved from
     `connections::order::Ple` to `connections::lattice::Ple`.

  Plus Plan 06 upstream added a `property.rs` shared-strategy harness
  and expanded the proptest battery (idempotent / closure properties
  across the 21 fixed-ladder and 15 sample-pair conns). No code-level
  effect on agogo, but the "generator domain = input type's full
  domain" house style is now formalised crate-wide upstream.

  Migration across four agogo files + CLI is mechanical renaming —
  23 lines changed, zero behavioural diff.

### Why a separate PR for the rev bump

Keeps the dependency change cleanly reviewable: rename-only diff, no
semantic code moves, easy to CI-verify without confounders. T2
(`Channel` → `Micro`) will land on top of this base with isolated
review of the actual structural change.

## Test plan

- [x] `cargo build --workspace` — clean.
- [x] `cargo test --workspace` — 234 passed (223 core + 11 cli, + 2
  ignored fixture-gated); zero failures; 0.29 s.
- [x] `cargo clippy --all-targets -- -D warnings` — clean.
- [x] No behavioural diff — only import paths and one type-name
  change (`FloatExt` → `ExtendedFloat`).
