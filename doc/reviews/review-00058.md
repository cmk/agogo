# PR #58 — doc: Mark facade feature gates in rustdoc

## Summary

Marks the `agogo` facade crate's feature-gated modules with docs.rs
`doc_cfg` metadata, so generated documentation can show which feature
enables each public namespace:

- `agogo::core`
- `agogo::host`
- `agogo::host::cpal`
- `agogo::host::link`
- `agogo::host::midi`

Configures the facade package's docs.rs metadata to build the pure-Rust
`core,host` facade documentation with `--cfg docsrs`, avoiding optional
backend features that need system-specific tooling. The backend
namespaces remain annotated for local or dedicated docs builds that
explicitly enable those features. The change does not add new runtime
API or alter existing feature forwarding; `host` still implies `core`,
and `link` still enables `link_impl/rusty-link`.

The external prelude suggestion is intentionally deferred until the
facade's stable convenience import set is clearer from downstream usage.

## Test plan

- `cargo fmt --all -- --check`
- `scripts/check-pii.sh`
- `scripts/check-floats.sh`
- `scripts/check-layers.sh`
- `cargo test -p agogo --quiet`
- `cargo test -p agogo --no-default-features --quiet`
- `cargo test -p agogo --no-default-features --features core --quiet`
- `cargo test -p agogo --no-default-features --features host --quiet`
- `cargo test -p agogo --features cpal,link,midi --quiet`
- `cargo clippy --all-targets --quiet -- -D warnings`
- `RUSTDOCFLAGS="--cfg docsrs" cargo +nightly doc -p agogo --no-default-features --features core,host --no-deps`

## Local review (2026-05-02)

**Branch:** plan-2026-05-02-03
**Commits:** 3 (origin/main..plan-2026-05-02-03)
**Reviewer:** Codex (`codex review --base origin/main`)

---

The changes are limited to docs.rs/Rustdoc feature metadata and corresponding review/plan documentation. The local docsrs check passed at review time; the GitHub review round below narrows the actual docs.rs metadata to the pure `core,host` feature set and adds CI coverage for that path.

<!-- gh-id: 3176325946 -->
### Copilot on [`doc/plans/plan-2026-05-02-03.md:27`](https://github.com/cmk/agogo/pull/58#discussion_r3176325946) (2026-05-02 07:34 UTC)

This says the facade package can be configured to build docs with all facade features on docs.rs, but the current feature graph includes backends that the repo explicitly keeps out of default CI because they need extra host tooling (`libasound2-dev` for cpal/midi and a C++/CMake toolchain for link). Unless the docs build is constrained to the pure-Rust feature set, this plan overstates what an actual docs.rs build can do today.

<!-- gh-id: 3176325967 -->
### Copilot on [`doc/reviews/review-00058.md:45`](https://github.com/cmk/agogo/pull/58#discussion_r3176325967) (2026-05-02 07:34 UTC)

This conclusion says the new cfg handling is consistent with docs.rs builds, but the PR also switches docs.rs to `all-features`, which would pull in the detached cpal/midi/link backends that the repo explicitly documents as needing extra system tooling. As written, the review record reads as if the config is ready for an actual docs.rs build when the current feature set is still tied to environment-specific dependencies.


<!-- gh-id: 3176325978 -->
### Copilot on [`crates/agogo/Cargo.toml:10`](https://github.com/cmk/agogo/pull/58#discussion_r3176325978) (2026-05-02 07:34 UTC)

Enabling `all-features` here pulls `cpal`, `midi`, and `link` into the docs build, but those backends are explicitly kept out of the default CI path because they need extra host tooling (`libasound2-dev` for cpal/midi and a C++/CMake toolchain for link). A real docs.rs build cannot install those system packages first, so this configuration is likely to make the published documentation build fail rather than just showing the new feature badges.


<!-- gh-id: 3176325997 -->
### Copilot on [`crates/agogo/src/lib.rs:2`](https://github.com/cmk/agogo/pull/58#discussion_r3176325997) (2026-05-02 07:34 UTC)

This new `docsrs`-only code path is only verified in the PR's local test plan. The repository CI does not run any `cargo doc` / `--cfg docsrs` job, so a future typo in these `doc_cfg` annotations or the matching Cargo metadata would silently regress the feature badges that this change is meant to add.

<!-- gh-id: 4214675588 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-05-02 07:34 UTC](https://github.com/cmk/agogo/pull/58#pullrequestreview-4214675588))

## Pull request overview

This PR aims to improve the `agogo` facade crate's generated Rustdoc by surfacing feature gates on public facade modules and by recording the change in the repo's plan/review docs. It fits into the codebase as documentation/build-metadata work for the top-level facade crate rather than a runtime API change.

**Changes:**
- Add `docsrs`-gated `doc_cfg` annotations to the facade crate's feature-gated public modules.
- Add docs.rs package metadata and a `docsrs` `check-cfg` allowance in `crates/agogo/Cargo.toml`.
- Add/update planning and review records describing the rustdoc badge work and local verification steps.

### Reviewed changes

Copilot reviewed 4 out of 4 changed files in this pull request and generated 4 comments.

| File | Description |
| ---- | ----------- |
| `doc/reviews/review-00058.md` | Adds the PR review record summarizing the rustdoc/doc_cfg change and local review conclusion. |
| `doc/plans/plan-2026-05-02-03.md` | Adds the implementation plan, verification steps, and deferred prelude note for the facade docs work. |
| `crates/agogo/src/lib.rs` | Adds `docsrs`-gated `doc_cfg` annotations for `core`, `host`, and optional backend namespaces. |
| `crates/agogo/Cargo.toml` | Adds docs.rs build metadata and registers `cfg(docsrs)` for the unexpected-cfg lint. |

<!-- gh-id: 3176337286 -->
#### ↳ cmk ([2026-05-02 07:38 UTC](https://github.com/cmk/agogo/pull/58#discussion_r3176337286))

Fixed: replaced docs.rs all-features metadata with no-default-features plus features = ["core", "host"], so docs.rs does not pull cpal, midi, or link backend dependencies.

<!-- gh-id: 3176337864 -->
#### ↳ cmk ([2026-05-02 07:39 UTC](https://github.com/cmk/agogo/pull/58#discussion_r3176337864))

Fixed: updated the plan to describe docs.rs as the pure core,host facade docs path and to leave backend doc_cfg coverage to explicit backend docs builds.

<!-- gh-id: 3176338831 -->
#### ↳ cmk ([2026-05-02 07:39 UTC](https://github.com/cmk/agogo/pull/58#discussion_r3176338831))

Fixed: revised the review artifact summary and local-review note so it no longer implies all-features is the docs.rs path; the round now records the core,host constraint.

<!-- gh-id: 3176339199 -->
#### ↳ cmk ([2026-05-02 07:39 UTC](https://github.com/cmk/agogo/pull/58#discussion_r3176339199))

Fixed: added a CI facade-docs job that runs nightly rustdoc with --cfg docsrs on the same core,host feature set used by docs.rs metadata.
