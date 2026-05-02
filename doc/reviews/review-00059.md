# PR #59 — Host rustdoc on GitHub Pages + check-layers.sh port

## Summary

Two-commit branch.

**`feat: Publish workspace rustdoc to GitHub Pages`** adds
`.github/workflows/docs.yml`. On every push to `main` the workflow
runs `cargo doc --workspace --no-deps --lib` with
`RUSTDOCFLAGS="-D warnings"`, writes a top-level `index.html`
redirect to the facade crate (`agogo/index.html`), uploads
`target/doc/` as a Pages artifact, and deploys via the standard
`actions/configure-pages@v5` + `actions/upload-pages-artifact@v3`
+ `actions/deploy-pages@v4` triple. Pull requests run the build
step only as a smoke check — no deploy from a fork. README gains
a Docs badge and a one-line link to <https://cmk.github.io/agogo/>.

`--lib` skips bin targets to dodge the cargo issue #6313 doc-output
collision between the `agogo` facade lib and `agogo-cli`'s
`[[bin]] name = "agogo"`. The cli is binary-only (no `lib.rs`) so
nothing meaningful is dropped. ALSA dev headers are pre-installed
even though the four current workspace members don't need them —
saves a re-edit if cpal/midi join the workspace later.

**One-time manual step before the first deploy succeeds**: in the
repo's GitHub Settings → Pages, set the source to "GitHub Actions".
The workflow fails with `not enabled` until that's done.

**`debt: Port check-layers.sh blind-spot fixes from stdio-core`**
upgrades the layering gate to close the two patterns its own
docstring explicitly flagged: `pub use crate::<top>::...`
re-exports and `use crate::{a, b}` grouped imports. The port also
validates that each `//! layer:` sentinel matches its filename so
a stale rename can't slip through. Pre-port audit confirms no file
under `crates/core/src/` uses either of the formerly-blind
patterns, so the gate's behaviour on the current tree is
unchanged — only future cost moves.

Preserved from agogo's version: the six `ALL_LAYERS`, the dual
`(crate|agogo_core)` regex (stdio-core's variant dropped to
`crate::` only), the smoke-test recipe, and the partial-order
HEREDOC. AGENTS.md updated so the prose acknowledges the new
coverage.

Smoke-tested locally: clean tree → `OK`; injected single-import
violation → fail; grouped-import violation → fail (both
top-level modules listed); `pub use` violation → fail; restore →
`OK`.

## Local review (2026-05-02)

**Branch:** plan-2026-05-02-04
**Commits:** 4 (origin/main..plan-2026-05-02-04)
**Reviewer:** Claude (sonnet, independent)

---

### Commit Hygiene
Four commits: `plan:`, `feat:`, `debt:`, `doc:`. Split is correct,
subjects under 72 chars, conventional-commit prefixes from the
accepted list, no merge commits.

### Code Quality

**`docs.yml` action versions and permission scoping** — current
versions, top-level `contents: read`, elevated `pages: write` +
`id-token: write` scoped to the `deploy` job only. Minimal and
correct.

**`docs.yml` PR path correctly skips deploy** — both
`upload-pages-artifact` (step `if`) and the `deploy` job (job
`if`) require `github.ref == 'refs/heads/main' && github.event_name
== 'push'`. PRs build but do not deploy. No fork-PR exfil path.

**`docs.yml` redirect HTML** — uses relative URLs
(`agogo/index.html`); the `<<'EOF'` (not `<<-'EOF'`) leaves leading
whitespace in the file. Cosmetic only — browsers render fine.

**`docs.yml` missing `configure-pages@v5` step** — Plan T1 and the
review-file Summary both describe a `configure-pages@v5` +
`upload-pages-artifact@v3` + `deploy-pages@v4` triple, but the
initial workflow only used the latter two. Adding `configure-pages`
matches the documented pattern and provides the
`base_url`/enablement-detection some `deploy-pages` flows expect.
**Auto-applied** as part of this round (see fix commit).

**`check-layers.sh` regex correctness** — the new grep
`'^(pub([[:space:]]*\([^)]*\))?[[:space:]]+)?use[[:space:]]+(crate|agogo_core)::'`
correctly matches `pub`, `pub(crate)`, `pub(super)`,
`pub(in path::to::mod)`, and plain `use`. `emit_import_tops`
handles both single and grouped forms; nested groups
(`use crate::{a::{x, y}, b}`) survive word-splitting because the
`^[a-z][a-z0-9_]*$` filter strips brace-bearing tokens. Leaf-node
`//! depends-on:` (empty after the colon) is handled correctly.
Sentinel-name validation trips on a mismatched layer/filename pair.

**`emit_import_tops` docstring inaccuracy** — the function header
listed three shapes including `use {crate,agogo_core}::{...}`,
which isn't valid Rust syntax (the function actually handles two
shapes: simple and brace-grouped, with either `crate` or
`agogo_core` as anchor). **Auto-applied** correction in the fix
commit so the docstring matches the implementation.

**AGENTS.md prose update** — accurately describes new behaviour
(column-0 imports including `pub use` re-exports and grouped
forms; sentinel-vs-filename validation). No overstatement.

**README** — badge URL and link target both point to the correct
repo paths.

### Test Coverage
Plan's three Spot Check smoke patterns (single-import,
grouped-import, `pub use`) were exercised locally; results
captured at lines 45–48 of the Summary. N/A for property tests
(docs/CI/scripts change, no library code).

### Plan Conformance
T1, T2, T3 all implemented. Build gates met. Nothing in the diff
falls outside plan scope.

### Risks
- Deploy-from-PR: no path. PR-triggered runs skip both upload and
  deploy.
- Layers script silent weakening: new grep is a strict superset of
  the old one. No coverage class dropped.
- One-time GitHub Pages settings step: documented in plan Review
  and review-file Summary.

### Recommendations

**Must fix before push:**

1. *(auto-applied)* Add `actions/configure-pages@v5` to `docs.yml`
   so the implementation matches the documented three-action
   pattern. Without it, the PR description is factually wrong about
   what was shipped, and the `gh pr create --body-file` mechanism
   would propagate that into the GitHub PR body.
2. *(auto-applied)* Strike the third shape from
   `emit_import_tops`'s docstring — it described invalid Rust
   syntax that the function does not actually parse.

**Follow-up (future work):**

- The `<<'EOF'` heredoc in `docs.yml` writes leading whitespace
  into the generated `index.html`. Cosmetic only; switch to
  `<<-'EOF'` + tab-indent if desired in a later pass.
