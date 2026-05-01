#!/usr/bin/env bash
# check-layers.sh — CI gate for the partial-order import rule.
#
# Each top-level module under `crates/core/src/` declares its layer
# and its allowed upstream dependencies in a sentinel header comment
# on its module-root file:
#
#   //! layer: time
#   //! depends-on: conn
#
# This script walks every `.rs` file under each layer's directory
# (and the layer's module-root file itself), greps for
# `use crate::<top>` and `use agogo_core::<top>` references, and
# fails on any reference to a top-level module the current layer's
# `depends-on:` list does not authorise.
#
# Layers (Plan 2026-04-29-01):
#
#   control  → sink, channel, time, conn
#   sink     → channel, time, conn
#   channel  → time, conn
#   time     → conn
#   conn     → (leaf)
#   test     → (leaf)
#
# Self-references are always allowed (a file under `time/` may
# import `crate::time::*`). Cross-crate imports
# (`agogo_core::*`) are subject to the same rule when they appear
# inside `crates/core/src/` (we only enforce the rule on core
# itself; downstream crates can pull from any layer).
#
# Smoke test (run in a dirty worktree):
#
#   1. Add `use crate::control::sync::pll::Pll;` to
#      `crates/core/src/conn/fixed.rs`. Run this script. It must
#      fail with `crates/core/src/conn/fixed.rs:N — conn imports
#      control which is not in conn's depends-on list`.
#   2. Revert. Run again. It must pass.
#
# Known blind spots (column-0 import patterns the gate misses):
#
#   - `pub use crate::<layer>::...` — line starts with `pub`, not
#     `use`, so the grep below skips it. None exist in core today,
#     but a layer-violating re-export added later would slip past
#     this script. Code review is the backstop.
#   - `use crate::{conn::..., control::...};` (grouped imports
#     starting with `{`). The grep requires `[a-z]` after
#     `crate::`, so the `{` form is invisible. Not present today.
#
# Both gaps are inert today (verified: `grep -rn "^pub use crate::"
# crates/core/src/` returns nothing; no grouped column-0 `use
# crate::{` either). If either pattern grows in production code,
# extend the grep — see review-00048 for context.

set -euo pipefail

REPO_ROOT=$(git rev-parse --show-toplevel)
cd "$REPO_ROOT"

CORE_SRC="crates/core/src"

# All top-level layers known to the rule. Keep in sync with the
# six `pub mod <name>;` declarations in `crates/core/src/lib.rs`.
ALL_LAYERS=(channel conn control sink test time)

# Parse a layer's allowed deps from its module-root sentinel.
# Returns space-separated list (empty if leaf).
parse_deps() {
    local layer="$1"
    local root="${CORE_SRC}/${layer}.rs"
    if [[ ! -f "$root" ]]; then
        printf 'check-layers.sh: missing module-root file %s\n' "$root" >&2
        exit 2
    fi
    # Look for the `//! depends-on: ...` line. Strip leading `//! `,
    # the `depends-on:` prefix, and any leading whitespace; commas
    # become spaces.
    local line
    line=$(grep -m1 -E '^//! depends-on:' "$root" || true)
    if [[ -z "$line" ]]; then
        printf 'check-layers.sh: %s missing `//! depends-on:` sentinel\n' "$root" >&2
        exit 2
    fi
    # `//! depends-on: a, b, c` → `a b c` (or empty if leaf).
    line="${line#*depends-on:}"
    line="${line//,/ }"
    # Trim leading/trailing whitespace.
    line="$(echo "$line" | xargs)"
    printf '%s' "$line"
}

# Files that belong to a layer:
#   - The module-root file `<layer>.rs`
#   - Every `.rs` under `<layer>/`
files_for_layer() {
    local layer="$1"
    local root="${CORE_SRC}/${layer}.rs"
    if [[ -f "$root" ]]; then
        printf '%s\n' "$root"
    fi
    if [[ -d "${CORE_SRC}/${layer}" ]]; then
        find "${CORE_SRC}/${layer}" -type f -name '*.rs' -print
    fi
}

FAIL=0

for layer in "${ALL_LAYERS[@]}"; do
    deps=$(parse_deps "$layer")
    # Build the regex of authorised top-level imports:
    #   self + every dep.
    authorised=("$layer")
    if [[ -n "$deps" ]]; then
        # shellcheck disable=SC2206
        authorised+=($deps)
    fi

    while IFS= read -r file; do
        # Find every `use crate::<top>` or `use agogo_core::<top>`
        # reference at column 0 (production code). Test-only `use`
        # statements live inside `#[cfg(test)] mod tests { … }`
        # blocks and are indented; they're allowed to cross layers
        # because integration tests legitimately need to wire pieces
        # from different layers together. The column-0 heuristic
        # picks up all production imports without needing a stateful
        # parser to identify `mod tests` blocks.
        while IFS= read -r hit; do
            line_num="${hit%%:*}"
            line_body="${hit#*:}"
            # Extract the top-level module name. The grep below
            # ensures the line matches `use (crate|agogo_core)::<X>`;
            # capture <X> here.
            top=$(echo "$line_body" | sed -nE \
                's/.*\buse[[:space:]]+(crate|agogo_core)::([a-z][a-z0-9_]*).*/\2/p' \
                | head -1)
            if [[ -z "$top" ]]; then
                continue
            fi
            # Is `top` in the authorised list?
            ok=0
            for a in "${authorised[@]}"; do
                if [[ "$top" == "$a" ]]; then
                    ok=1
                    break
                fi
            done
            if (( ok )); then
                continue
            fi
            # Is `top` even a known top-level layer? If not, it's
            # a re-export from a sibling crate path or a non-layer
            # module — ignore.
            known=0
            for a in "${ALL_LAYERS[@]}"; do
                if [[ "$top" == "$a" ]]; then
                    known=1
                    break
                fi
            done
            if (( ! known )); then
                continue
            fi
            printf '%s:%s — %s imports %s which is not in %s'\''s depends-on list\n' \
                "$file" "$line_num" "$layer" "$top" "$layer" >&2
            printf '    %s\n' "$line_body" >&2
            FAIL=1
        done < <(grep -nE '^use[[:space:]]+(crate|agogo_core)::[a-z]' "$file" || true)
    done < <(files_for_layer "$layer")
done

if (( FAIL )); then
    cat >&2 <<'HINT'

check-layers.sh: FAIL.

The partial-order layering rule keeps inter-module imports
unidirectional. To resolve:

  - If the import is genuinely cross-cutting, lift the shared
    type up to a lower layer and re-export from both sides.
  - If the import is a stale leftover from a refactor, delete it.
  - If the layering needs to expand (rare), add the new edge to
    the offending layer's `//! depends-on:` sentinel comment AND
    document the rationale in AGENTS.md.

HINT
    exit 1
fi

printf 'check-layers.sh: OK — module imports respect the partial order.\n'
