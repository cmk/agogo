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
# (and the layer's module-root file itself), greps for column-zero
# `use crate::<top>` / `use agogo_core::<top>` references — including
# `pub use` re-exports and `use crate::{...}` grouped imports — and
# fails on any reference to a top-level module the current layer's
# `depends-on:` list does not authorise. The `//! layer:` sentinel is
# also checked against the filename so a stale rename can't go
# unnoticed.
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
# import `crate::time::*`). Cross-crate imports (`agogo_core::*`)
# are subject to the same rule when they appear inside
# `crates/core/src/` (we only enforce the rule on core itself;
# downstream crates can pull from any layer).
#
# Smoke test (run in a dirty worktree):
#
#   1. Add `use crate::control::sync::pll::Pll;` to
#      `crates/core/src/conn/fixed.rs`. Run this script. It must
#      fail with `crates/core/src/conn/fixed.rs:N — conn imports
#      control which is not in conn's depends-on list`.
#   2. Revert. Run again. It must pass.
#
# Test-block imports (indented inside `#[cfg(test)] mod tests { … }`)
# are allowed to cross layers — integration tests legitimately need
# to wire pieces together — and are skipped by the column-zero
# heuristic.

set -euo pipefail

REPO_ROOT=$(git rev-parse --show-toplevel)
cd "$REPO_ROOT"

CORE_SRC="crates/core/src"

# All top-level layers known to the rule. Keep in sync with the
# six `pub mod <name>;` declarations in `crates/core/src/lib.rs`.
ALL_LAYERS=(channel conn control sink test time)

# Parse a layer's allowed deps from its module-root sentinel.
# Returns space-separated list (empty if leaf). Also validates
# that the `//! layer:` line names the same layer the file is
# meant to declare.
parse_deps() {
    local layer="$1"
    local root="${CORE_SRC}/${layer}.rs"
    if [[ ! -f "$root" ]]; then
        printf 'check-layers.sh: missing module-root file %s\n' "$root" >&2
        exit 2
    fi

    local declared
    declared=$(grep -m1 -E '^//! layer:' "$root" || true)
    if [[ -z "$declared" ]]; then
        printf 'check-layers.sh: %s missing `//! layer:` sentinel\n' "$root" >&2
        exit 2
    fi
    declared="${declared#*layer:}"
    declared="$(echo "$declared" | xargs)"
    if [[ "$declared" != "$layer" ]]; then
        printf 'check-layers.sh: %s declares layer `%s`, expected `%s`\n' \
            "$root" "$declared" "$layer" >&2
        exit 2
    fi

    local line
    line=$(grep -m1 -E '^//! depends-on:' "$root" || true)
    if [[ -z "$line" ]]; then
        printf 'check-layers.sh: %s missing `//! depends-on:` sentinel\n' "$root" >&2
        exit 2
    fi
    # `//! depends-on: a, b, c` → `a b c` (or empty if leaf).
    line="${line#*depends-on:}"
    line="${line//,/ }"
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

known_layer() {
    local candidate="$1"
    local layer
    for layer in "${ALL_LAYERS[@]}"; do
        [[ "$candidate" == "$layer" ]] && return 0
    done
    return 1
}

authorised_layer() {
    local candidate="$1"
    shift
    local layer
    for layer in "$@"; do
        [[ "$candidate" == "$layer" ]] && return 0
    done
    return 1
}

# Emit each top-level module name imported by a single column-zero
# use line. Handles two shapes (with either `crate` or
# `agogo_core` as the anchor):
#   use crate::<top>::...;
#   use crate::{<top>::..., <top>::..., ...};
# `pub use` (with optional visibility qualifier) is matched by the
# caller's grep — this function only inspects the body after `use`.
emit_import_tops() {
    local line_body="$1"
    local rest item

    if [[ "$line_body" =~ use[[:space:]]+(crate|agogo_core)::\{([^}]*)\} ]]; then
        rest="${BASH_REMATCH[2]}"
        rest="${rest//,/ }"
        for item in $rest; do
            item="${item%%::*}"
            item="${item%%;*}"
            item="${item%% as *}"
            [[ "$item" =~ ^[a-z][a-z0-9_]*$ ]] && printf '%s\n' "$item"
        done
        return
    fi

    if [[ "$line_body" =~ use[[:space:]]+(crate|agogo_core)::([a-z][a-z0-9_]*) ]]; then
        printf '%s\n' "${BASH_REMATCH[2]}"
    fi
}

FAIL=0

for layer in "${ALL_LAYERS[@]}"; do
    deps=$(parse_deps "$layer")
    authorised=("$layer")
    if [[ -n "$deps" ]]; then
        # shellcheck disable=SC2206
        authorised+=($deps)
    fi

    while IFS= read -r file; do
        while IFS= read -r hit; do
            line_num="${hit%%:*}"
            line_body="${hit#*:}"
            while IFS= read -r top; do
                [[ -z "$top" ]] && continue
                known_layer "$top" || continue
                authorised_layer "$top" "${authorised[@]}" && continue

                printf '%s:%s — %s imports %s which is not in %s'\''s depends-on list\n' \
                    "$file" "$line_num" "$layer" "$top" "$layer" >&2
                printf '    %s\n' "$line_body" >&2
                FAIL=1
            done < <(emit_import_tops "$line_body")
        done < <(grep -nE '^(pub([[:space:]]*\([^)]*\))?[[:space:]]+)?use[[:space:]]+(crate|agogo_core)::' "$file" || true)
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
