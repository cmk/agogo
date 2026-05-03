# PR #66 - Replace SampleTime helpers with explicit Conns

## Summary

This removes the `SampleTime` convenience trait and replaces its hidden
Q48.16 conversion helpers with explicit named sample connections.

The sample connection module now publishes transparent `SxxxQ016` isos
for the six supported sample-rate newtypes and composed left-sided
`SxxxI064` conns through upstream `Q016Q000` and `Q000I064`. Law battery
coverage was added for all twelve new conns, with spot checks pinning
whole-sample and negative fractional `S048I064` rounding.

The sync/control/host stack no longer carries `R: SampleTime` bounds.
Generic state containers remain rate-typed, but methods that need
conversion behavior are expanded for the six concrete `Sxxx` rates, so
future conversion policy has to be expressed as a named conn or explicit
raw-bit representation access.

Validation:

- `cargo fmt --all -- --check`
- `scripts/check-pii.sh`
- `scripts/check-floats.sh`
- `scripts/check-layers.sh`
- `scripts/check-connections.sh`
- `cargo test -p agogo-core --quiet`
- `cargo test --workspace --quiet`
- `cargo clippy --all-targets --quiet -- -D warnings`

## Local review (2026-05-03)

**Branch:** plan-2026-05-03-02
**Commits:** 3 (origin/main..plan-2026-05-03-02)
**Reviewer:** Codex (`codex review --base origin/main`)

---

The code and tests pass, but the newly introduced public sample-count connection API is awkwardly incomplete compared with the rest of the module and will fail for straightforward downstream use unless an implementation-detail trait is imported.

Review comment:

- [P2] Add inherent methods for SxxxI064 conns — crates/core/src/conn/sample.rs:331-341
  Callers using the new sample-count conns as advertised, e.g. `use agogo_core::conn::sample::S048I064; S048I064.inner(1)`, will not compile unless they also import `connections::conn::ViewL`. The existing rate and pico conns in this module expose inherent `ceil`/`inner` wrappers, so the new public `SxxxI064` family should do the same to keep the intended API usable and consistent.

## Local review (2026-05-03)

**Branch:** plan-2026-05-03-02
**Commits:** 4 (origin/main..plan-2026-05-03-02)
**Reviewer:** Codex (`codex review --base origin/main`)

---

The changes compile and the workspace tests pass. I did not find any discrete introduced defects that would break existing behavior or the documented migration to explicit sample-rate connections.

