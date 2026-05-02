# PR #56 — feat: Bump connections and name cast conversions

## Summary

This PR bumps `connections` from
`01a2bf92cbe3d3e7eda7ab91f5b4dfc9cc6f4242` to
`238a8aaf913db48a31c1d7409504fb9c2745d3cd` and migrates agogo's local
connection definitions to the current kind-tagged `ConnL` / `ConnR`
API.

It keeps existing `.ceil()`, `.inner()`, and `.floor()` call sites
working through local zero-sized marker values, converts MIDI U7/U4
casts to one-sided `ConnL` values, and replaces matching manual
saturating integer casts with named upstream integer Conns.

Validation:

- `cargo fmt --all -- --check`
- `scripts/check-pii.sh`
- `scripts/check-floats.sh`
- `scripts/check-layers.sh`
- `cargo test --workspace --quiet`
- `cargo clippy --all-targets --quiet -- -D warnings`
- `cargo test -p agogo-cli --no-default-features --no-run`

## Local review (2026-05-01, initial pass)

**Branch:** plan-2026-05-02-01
**Commits:** 3 (origin/main..plan-2026-05-02-01)
**Reviewer:** Codex (`codex review --base origin/main`)

---

No actionable correctness issues were found; the workspace tests, warning-denied clippy, and the no-default-features CLI check pass for the reviewed diff.

## Local review (2026-05-01, final pass after rustdoc drift fixes)

**Branch:** plan-2026-05-02-01
**Commits:** 5 (origin/main..plan-2026-05-02-01)
**Reviewer:** Codex (`codex review --base origin/main`)

---

The migrated connection markers and saturating-cast replacements preserve the existing behavior, and the workspace tests, clippy, no-default CLI check, and rustdoc warning check pass locally. I did not find any actionable correctness issues in the diff.

<!-- gh-id: 3176215147 -->
### Copilot on [`doc/reviews/review-00056.md:39`](https://github.com/cmk/agogo/pull/56#discussion_r3176215147) (2026-05-02 05:49 UTC)

This review record has two separate "Local review (2026-05-01)" sections for the same branch, but they report different commit counts (3 vs 5). That’s ambiguous for readers trying to determine which review state is authoritative; consider consolidating into a single section or clearly labeling one as superseded (e.g., “updated after additional commits”).

<!-- gh-id: 3176215152 -->
### Copilot on [`crates/core/src/time/conn.rs:26`](https://github.com/cmk/agogo/pull/56#discussion_r3176215152) (2026-05-02 05:49 UTC)

The module-level docs still describe these as five `Conn<A, B>` values and the table lists shapes like `Conn<Tick, Time>`, but the implementation now exposes zero-sized marker structs plus `RuntimeConn` for `quantize_at`. To avoid misleading downstream readers, update the docs/table wording to reflect the marker/ViewL-ViewR API (and that `quantize_at` no longer returns a `Conn<_, _>`).

<!-- gh-id: 4214536659 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-05-02 05:49 UTC](https://github.com/cmk/agogo/pull/56#pullrequestreview-4214536659))

## Pull request overview

This PR updates the workspace to a newer `connections` git revision and migrates agogo’s local “connection” definitions and cast sites onto the newer kind-tagged `ConnL` / `ConnR` API while keeping existing `.ceil()/.inner()/.floor()` call sites usable via ZST marker values.

**Changes:**
- Bump `connections` git rev and refresh `Cargo.lock`.
- Migrate local triple Conns to zero-sized marker structs implementing `ViewL`/`ViewR`, and update call sites/tests to use the new views.
- Replace several manual saturating/clamping integer casts with named upstream integer Conns.

### Reviewed changes

Copilot reviewed 17 out of 18 changed files in this pull request and generated 2 comments.

<details>
<summary>Show a summary per file</summary>

| File | Description |
| ---- | ----------- |
| doc/reviews/review-00056.md | Adds a review record for PR #56 (contains duplicated “Local review” sections). |
| doc/plans/plan-2026-05-02-01.md | Adds a plan describing the rev bump, migration steps, and verification gates. |
| crates/core/src/time/tick.rs | Updates rustdoc link to the renamed `TICKTIME` connection marker. |
| crates/core/src/time/swing.rs | Replaces manual clamp+cast with `I128U064` connection usage. |
| crates/core/src/time/envelope.rs | Uses `U128U008` for narrowing conversions in envelope primitives. |
| crates/core/src/time/conn.rs | Major migration to marker-based Conns + introduces `RuntimeConn` for `quantize_at`. |
| crates/core/src/control/sync/source.rs | Fixes rustdoc link to use `Self::phase_at_sample`. |
| crates/core/src/control/event.rs | Replaces manual clamp/casts with `I064U064` / `I128U064` connection usage. |
| crates/core/src/conn/sample.rs | Converts rate/pico connection constants into marker structs; updates tests for `ViewL`/`ViewR`. |
| crates/core/src/conn/midi.rs | Converts MIDI cast connections to one-sided `ConnL` values via `Conn::new_l`. |
| crates/core/src/conn/float.rs | Converts float boundary Conns to marker structs; updates tests to use view-specific laws. |
| crates/core/src/conn/fixed.rs | Converts fixed ladder Conns to marker structs; updates tests to use view-specific laws. |
| crates/core/src/conn/boundary.rs | Adjusts rustdoc links for `Phase`/`Tempo` to rely on local imports. |
| crates/core/src/channel/time.rs | Uses `I064U064` for delay/offset sample narrowing conversions. |
| crates/core/src/channel/spec.rs | Fixes rustdoc link to `crate::channel::Channel`. |
| crates/cli/src/parsers.rs | Improves CLI flag documentation formatting (backticks). |
| Cargo.toml | Bumps `connections` git revision. |
| Cargo.lock | Updates `connections` source hash to match the new revision. |

</details>

<!-- gh-id: 3176219607 -->
#### ↳ cmk ([2026-05-02 05:54 UTC](https://github.com/cmk/agogo/pull/56#discussion_r3176219607))

Fixed - the two local-review sections are now labeled as the initial pass and the final pass after rustdoc drift fixes, so the authoritative final review state is explicit.

<!-- gh-id: 3176219665 -->
#### ↳ cmk ([2026-05-02 05:54 UTC](https://github.com/cmk/agogo/pull/56#discussion_r3176219665))

Fixed - the module docs now describe the static connections as marker values with `ViewL`/`ViewR`, and `quantize_at` as returning `RuntimeConn<Tick, Time>` rather than a `Conn<_, _>`.
