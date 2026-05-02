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
