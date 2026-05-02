# PR #55 — doc: Unify roadmap references

<!-- gh-id: 3175597096 -->
### Copilot on [`doc/designs/output.md:22`](https://github.com/cmk/agogo/pull/55#discussion_r3175597096) (2026-05-01 23:37 UTC)

`ChannelMode` appears to have been removed/replaced in the current codebase, and the referenced path `crates/core/src/channel/mode.rs` does not exist anymore (see `crates/core/src/channel/role.rs` which notes the `ChannelMode` replacement). Please update this section to reference the current `Channel`/role model (and adjust the follow-on sentence about merging enums accordingly). Also, the later bullet that describes agogo as having a “192/960 PPQN master” is stale given `crates/core/src/time/tick.rs` defines a single `PPQN = 960`; consider updating the wording to avoid implying 192 is still a supported master resolution.

<!-- gh-id: 4213807194 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-05-01 23:37 UTC](https://github.com/cmk/agogo/pull/55#pullrequestreview-4213807194))

## Pull request overview

This PR updates the project’s roadmap/version documentation and related in-code/doc references to consistently reflect the revised version ownership (v0.1 baseline kernel, v0.2 steel thread, v0.3 clock domains/followers, v0.4 heterogeneous output/latency, v0.5 persistence/calibration/hardening).

**Changes:**
- Rewrites `doc/versions/version-0.{1..5}.md` to the new thesis/scope/properties/acceptance structure and updated version allocation.
- Updates multiple design docs in `doc/designs/` (and `doc/agogo.md`) to match the new version mapping and terminology.
- Adjusts comments/README text across host/core/cli crates to remove stale “Plan X / v0.Y” references and align wording with the updated roadmap.

### Reviewed changes

Copilot reviewed 42 out of 42 changed files in this pull request and generated 1 comment.

<details>
<summary>Show a summary per file</summary>

| File | Description |
| ---- | ----------- |
| doc/versions/version-0.5.md | Replaces prior v0.5 plan with new v0.5 thesis/scope focused on persistence/calibration/hardening. |
| doc/versions/version-0.4.md | Replaces prior v0.4 plan with new v0.4 thesis/scope focused on heterogeneous output and latency. |
| doc/versions/version-0.3.md | Replaces prior v0.3 plan with new v0.3 thesis/scope focused on clock domains and followers. |
| doc/versions/version-0.2.md | Replaces prior v0.2 plan with new v0.2 steel-thread thesis/sprints/properties. |
| doc/versions/version-0.1.md | Replaces prior v0.1 plan with new v0.1 baseline kernel positioning and constraints. |
| doc/designs/tui.md | Updates version attribution for snapshot publication/seq semantics/log channeling. |
| doc/designs/transport.md | Updates context to place transport FSM ownership in v0.3 and expands phase-source list. |
| doc/designs/shift.md | Updates context to remove stale plan references and re-anchors dezippering ownership. |
| doc/designs/pid.md | Updates Link follower PID context from v0.5 to v0.3. |
| doc/designs/output.md | Updates context to move heterogeneous output to v0.4 and adjusts referenced verification properties. |
| doc/designs/mtc.md | Updates context to move MTC generation to v0.4 and adjusts deferred notes. |
| doc/designs/link.md | Updates Link roadmap context to v0.3 and aligns snapshot terminology. |
| doc/designs/dsl.md | Renames header to “post-v0.2”. |
| doc/designs/cv-pulse.md | Updates v0.4 property names and scope wording for CV pulse behavior. |
| doc/designs/control-plane.md | Updates control-plane context/phrasing to align with v0.2 steel-thread framing. |
| doc/agogo.md | Updates library and open-question bullets to reflect new version ownership/wording. |
| crates/host-midi/src/midir.rs | Updates comments to remove stale plan references and clarify test intent. |
| crates/host-midi/src/lib.rs | Updates crate docs to clarify midir best-effort role and v0.2+ timestamped-sink direction. |
| crates/host-midi/README.md | Updates README wording to align with v0.2+ timestamped-output roadmap. |
| crates/host-midi/Cargo.toml | Updates comments about detached crate and v0.2+ timing work. |
| crates/host-link/src/transport.rs | Updates module docs to reflect v0.3 (not v0.5) forerun/FSM extension framing. |
| crates/host-link/src/source.rs | Updates comments to reflect v0.3 ownership and removes stale plan references. |
| crates/host-link/src/session.rs | Updates comments to remove stale plan references. |
| crates/host-link/Cargo.toml | Updates comments about transport FSM scaffolding being extended in v0.3. |
| crates/host-cpal/src/lib.rs | Updates crate docs to remove stale plan references and align timestamped-output timing notes. |
| crates/host-cpal/src/cpal/control.rs | Updates comments around RT safety, message sizing, and default sizing example. |
| crates/host-cpal/src/cpal/callback.rs | Updates callback module docs/comments to remove stale plan references. |
| crates/host-cpal/src/cpal.rs | Updates comments/error message wording around mono-input limitation and fixtures. |
| crates/host-cpal/README.md | Updates README wording to remove stale plan references and clarify module responsibilities. |
| crates/host-cpal/Cargo.toml | Updates comments about detached crate rationale. |
| crates/core/src/sink/midi.rs | Updates module docs/comments to remove stale plan references and clarify transport layering. |
| crates/core/src/sink/audio.rs | Updates module docs/comments to remove stale plan references and align forward-compat notes. |
| crates/core/src/control/event.rs | Updates comments to remove stale plan references for allocation-free scheduling contract. |
| crates/core/src/control.rs | Updates `Machine` module docs/comments to remove stale plan references and align function names. |
| crates/core/src/channel/spec/types.rs | Updates comment roadmap wording for future routing-target tag additions. |
| crates/core/src/channel/spec/display.rs | Updates test comment wording to remove stale plan references. |
| crates/core/src/channel.rs | Updates comment about forward-compat routing roles/backends. |
| crates/cli/src/trace/midi.rs | Updates comment about longer MIDI messages arriving with CC output work. |
| crates/cli/src/run.rs | Updates module docs and test comments to remove stale plan references. |
| crates/cli/src/demo.rs | Updates module docs/comments and error message wording to remove stale plan references. |
| crates/cli/src/command.rs | Updates CLI command doc comment to remove plan references. |
| crates/cli/Cargo.toml | Updates feature comments to remove stale plan references and keep `run` description current. |
</details>






<!-- gh-id: 3175658664 -->
#### ↳ cmk ([2026-05-02 00:16 UTC](https://github.com/cmk/agogo/pull/55#discussion_r3175658664))

Fixed - `doc/designs/output.md` now describes the current `Channel` sum type plus target-specific role enums in `crates/core/src/channel/role.rs`, and the MIDI clock bullet now refers only to the current 960 PPQN master.
