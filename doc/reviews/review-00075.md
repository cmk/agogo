## Summary

Implements Plan 2026-05-04-03.

First performs the mechanical public API cleanup requested before the render
work: adds the `agogo::chan` facade and moves downstream pure
channel/time/control/sink/conn imports to it; narrows `agogo::core` to runtime
orchestration exports; removes the public `conn::boundary` module by exposing
its helpers through `conn::float`; renames `conn::sample` to `conn::rate`; and
renames the sample-rate marker/connection family from `SXYZ` to `RXYZ`.

Then adds a deterministic hardware-free render path. `agogo-core` now exposes
`render_offline`, which drives the same `Playhead::on_buffer` path used by host
callbacks with silent input, scratch audio output, and an in-memory MIDI sink.
`agogo render` parses the same channel specs as `agogo run`, currently accepts
only `--source internal`, bounds duration by bars, and prints deterministic JSON
with MIDI records, offline timing metadata, dropped count, and audio-output
summary fields. The README now uses this as the primary hardware-free runtime
demo.

Verification run locally:

- `scripts/check-pii.sh`
- `scripts/check-floats.sh`
- `scripts/check-layers.sh`
- `scripts/check-connections.sh`
- `scripts/check-boundary-panics.sh`
- `cargo test -p agogo-core render --quiet`
- `cargo test -p agogo-cli --test render --quiet`
- `cargo run -p agogo-cli --bin agogo -- render --source internal --bpm 120 --sr 48000 --duration-bars 1 --buffer-frames 4096 --ch id=three,dev=midi,mode=clock,grid=t2t,out=diag --ch id=two,dev=midi,mode=clock,grid=t2,out=diag`
- `cargo check -p agogo-cli --features cpal --quiet`
- `cargo check --manifest-path crates/host-midi/Cargo.toml --quiet`
- `cargo check --manifest-path crates/host-link/Cargo.toml --quiet`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test --workspace`

## Local review (2026-05-03)

**Branch:** plan/2026-05-04-03
**Commits:** 4 (origin/main..plan/2026-05-04-03)
**Reviewer:** Codex (`codex review --base origin/main`)

---

The default workspace tests pass, but optional host backend builds are broken by unresolved `agogo::chan` paths in crates that do not depend on the facade module exposing that path. This makes feature builds such as `agogo-cli --features cpal` fail to compile.

Review comment:

- [P1] Keep detached host crates on a resolvable API path — crates/host-cpal/src/cpal/callback.rs:14-15
  When building any optional host backend, these `agogo::chan` imports do not resolve because the detached host crates still depend only on `agogo-core` and define a local `agogo::core` shim, not the facade crate's new `agogo::chan` module. For example, `cargo check -p agogo-cli --features cpal` fails here, and the same pattern also breaks `host-midi` and `host-link`; either add/update the local shim/dependencies or import through the existing `core_impl`/`agogo::core` surface.

Resolution:

- Added a local public `agogo::chan` shim to each detached host crate
  (`host-cpal`, `host-midi`, `host-link`) that re-exports the pure
  channel/conn/control/sink/test/time surface from `agogo-core`.
- Verified:
  - `cargo check -p agogo-cli --features cpal --quiet`
  - `cargo check --manifest-path crates/host-midi/Cargo.toml --quiet`
  - `cargo check --manifest-path crates/host-link/Cargo.toml --quiet`

<!-- gh-id: 3179851620 -->
### Copilot on [`crates/cli/src/command/render.rs:128`](https://github.com/cmk/agogo/pull/75#discussion_r3179851620) (2026-05-04 07:03 UTC)

`agogo render` currently lowers each parsed `ChannelSpec` straight into a runtime `Channel`, which drops the original `out=` routing target. That means specs like `out=hw-port` are accepted and rendered successfully even though offline mode never uses that output selection. The plan explicitly calls for rejecting real output targets when `out` semantics are not reused, so this should validate `out` before discarding it instead of silently ignoring the user's target.

Resolution:

- Added `agogo render` validation that accepts omitted diagnostic routing or
  `out=diag` / `out=diagnostic`, and rejects concrete output targets before
  lowering `ChannelSpec` into `Channel`.
- Added an integration test for `out=hw-port` rejection.
- Verified:
  - `cargo test -p agogo-cli --test render --quiet`
  - `scripts/check-pii.sh`
  - `cargo clippy --all-targets -- -D warnings`

<!-- gh-id: 4217969923 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-05-04 07:03 UTC](https://github.com/cmk/agogo/pull/75#pullrequestreview-4217969923))

## Pull request overview

This PR restructures the public API around a new `agogo::chan` facade, renames the sample-rate surface from `sample`/`Sxxx` to `rate`/`Rxxx`, and adds a hardware-free offline render path exposed through `agogo render`. It fits into the codebase by separating pure channel/time/control types from runtime orchestration while adding a deterministic CLI/runtime path for CI-friendly rendering and demos.

**Changes:**
- Adds `agogo::chan`, narrows `agogo::core` to orchestration exports, and updates downstream crates/imports to the new facade.
- Renames `conn::sample` to `conn::rate`, moves float-boundary helpers under `conn::float`, and updates docs/scripts/tests for the new naming.
- Introduces `render_offline` in `agogo-core` plus the `agogo render` CLI command and render-focused tests/docs.

### Reviewed changes

Copilot reviewed 53 out of 54 changed files in this pull request and generated 1 comment.

<details>
<summary>Show a summary per file</summary>

| File | Description |
| ---- | ----------- |
| scripts/check-floats.sh | Updates float allowlist paths/names for `float_boundary` and `rate`. |
| doc/reviews/review-00075.md | Adds local review record for this PR and prior host-crate shim fix. |
| doc/plans/plan-2026-05-04-03.md | Updates plan wording/examples to the new facade, rate names, and render command details. |
| doc/agogo.md | Refreshes module-layout docs from `core/src`/`sample` to `chan/src`/`rate`. |
| crates/host-midi/src/midir.rs | Switches MIDI sink imports to `agogo::chan`. |
| crates/host-midi/src/lib.rs | Adds local `agogo::chan` shim and updates crate docs. |
| crates/host-midi/README.md | Updates public API path in README text. |
| crates/host-link/tests/bidirectional.rs | Migrates test imports to `agogo::chan`. |
| crates/host-link/src/source.rs | Migrates Link source imports/docs to `agogo::chan`. |
| crates/host-link/src/session.rs | Migrates Link session imports/comments to `agogo::chan`. |
| crates/host-link/src/quantum.rs | Migrates fixed-point import to `agogo::chan`. |
| crates/host-link/src/link.rs | Repoints float/tempo/phase imports to `agogo::chan::conn::float` and `rate`. |
| crates/host-link/src/lib.rs | Adds local `agogo::chan` shim and updates top-level docs. |
| crates/host-cpal/src/lib.rs | Adds local `agogo::chan` shim and updates crate docs. |
| crates/host-cpal/src/cpal/control.rs | Migrates MIDI sink imports to `agogo::chan`. |
| crates/host-cpal/src/cpal/callback.rs | Renames callback rate types to `Rxxx` and moves pure imports to `agogo::chan`. |
| crates/host-cpal/src/cpal.rs | Migrates audio host imports to `agogo::chan`. |
| crates/host-cpal/README.md | Updates public API path in README text. |
| crates/core/src/transport.rs | Renames rate types and adds offline render config/report/errors plus render tests/properties. |
| crates/core/src/runtime.rs | Updates runtime’s fixed rate from `S048` to `R048`. |
| crates/core/src/lib.rs | Re-exports offline render API from `agogo-core`. |
| crates/core/src/bridge.rs | Renames bridge test rate type to `R048`. |
| crates/cli/test/render.rs | Adds integration coverage for the new `render` subcommand. |
| crates/cli/src/parse.rs | Repoints CLI parsers to `agogo::chan::conn::float` and `rate`. |
| crates/cli/src/command/time.rs | Migrates time-command imports to `agogo::chan`. |
| crates/cli/src/command/sync.rs | Migrates sync command to `agogo::chan` and `R048`. |
| crates/cli/src/command/run.rs | Migrates run command imports/names from `core`/`sample` to `chan`/`rate`. |
| crates/cli/src/command/render.rs | Adds the new offline render CLI command and JSON formatter. |
| crates/cli/src/command/midi.rs | Migrates MIDI command imports to `agogo::chan`. |
| crates/cli/src/command/link/probe.rs | Migrates Link probe imports and helpers to `agogo::chan`. |
| crates/cli/src/command/link/commands.rs | Migrates Link command tempo formatting import to `agogo::chan::conn::float`. |
| crates/cli/src/command/link.rs | Updates Link CLI types/defaults to `agogo::chan` paths. |
| crates/cli/src/command/demo.rs | Migrates demo command imports/types to `agogo::chan` and `R048`. |
| crates/cli/src/command/channel.rs | Migrates channel command imports to `agogo::chan`. |
| crates/cli/src/command.rs | Registers the new `render` subcommand and migrates shared type imports. |
| crates/cli/Cargo.toml | Adds `serde_json` and a new `render` integration test target. |
| crates/chan/src/time/conn.rs | Renames sample-rate imports/types from `Sxxx` to `Rxxx`. |
| crates/chan/src/test.rs | Updates shared test-surface docs to `agogo::chan::test`. |
| crates/chan/src/control/source.rs | Renames phase-source rate types/imports to `Rxxx`. |
| crates/chan/src/control/pulse.rs | Moves float helpers to `conn::float` and renames pulse rates to `Rxxx`. |
| crates/chan/src/control/pll.rs | Repoints float-boundary helpers to `conn::float` and renames PLL rate types. |
| crates/chan/src/control/detect.rs | Renames detector rate impls/imports to `Rxxx` and updates float helper path. |
| crates/chan/src/conn/rate.rs | Renames the former sample module/types/conns/docs from `Sxxx` to `Rxxx`. |
| crates/chan/src/conn/float_boundary.rs | Renames docs/imports from `sample` to `rate` and from `boundary` surface to `float`. |
| crates/chan/src/conn/float.rs | Re-exports `float_boundary` through `conn::float`. |
| crates/chan/src/conn/arb.rs | Updates arb docs to `Rxxx`/`conn::rate`. |
| crates/chan/src/conn.rs | Replaces public `boundary`/`sample` modules with `float`/`rate` docs and exports. |
| crates/chan/src/channel/time.rs | Repoints `pico_to_samples` import to `conn::float`. |
| crates/chan/src/channel/dsl.rs | Updates doc examples to `agogo::chan` paths. |
| crates/agogo/src/lib.rs | Adds the public `chan` facade and narrows `core` re-exports to orchestration APIs. |
| crates/agogo/Cargo.toml | Adds direct optional dependency on `agogo-chan` and wires it into the `core` feature. |
| README.md | Promotes `agogo render` as the primary hardware-free demo. |
| Cargo.lock | Records new dependency edges for `agogo-chan` and `serde_json`. |
| AGENTS.md | Updates repository guidance for `float_boundary`, `rate`, and `Rxxx` naming. |
</details>

<!-- gh-id: 3179945850 -->
#### ↳ cmk ([2026-05-04 07:23 UTC](https://github.com/cmk/agogo/pull/75#discussion_r3179945850))

Fixed in dc1392b. agogo render now validates diagnostic routing before lowering ChannelSpec into Channel: omitted out= plus out=diag/out=diagnostic are accepted, and concrete targets like out=hw-port are rejected with a boundary-level error. Added render_rejects_real_output_targets to cover the regression.
