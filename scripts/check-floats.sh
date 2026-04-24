#!/usr/bin/env bash
# check-floats.sh — CI gate for CLAUDE.md's no-stored-float rule.
#
# `f32` / `f64` may appear only in the seven documented exception
# modules listed below. Each file is allowed because its contents
# fall into one of the five enumerated exception categories
# (CLAUDE.md §Repository conventions):
#
#   crates/core/src/sync/pll.rs     PI controller state + control law
#   crates/core/src/sync/detect.rs  parabolic-fit ABI-local locals
#   crates/core/src/sync/source.rs  PCM audio intake (`&[f32]`) + tests
#   crates/core/src/fxp.rs          argv-boundary + PI-exempt helpers
#   crates/core/src/arb.rs          test-fixture PCM generators
#   crates/host-link/src/link.rs    Link FFI (AblLink C++ ABI)
#   crates/cli/src/main.rs          argv parsers
#
# Any `f32` / `f64` in a non-allowlisted file is a build failure.
# To add a new allowlisted file, amend both this script and
# CLAUDE.md so the rule and the gate stay in sync.

set -euo pipefail

ALLOWED=(
  "crates/core/src/sync/pll.rs"
  "crates/core/src/sync/detect.rs"
  "crates/core/src/sync/source.rs"
  "crates/core/src/fxp.rs"
  "crates/core/src/arb.rs"
  "crates/host-link/src/link.rs"
  "crates/cli/src/main.rs"
)

REPO_ROOT=$(git rev-parse --show-toplevel)
cd "$REPO_ROOT"

FAIL=0
while IFS= read -r -d '' file; do
  rel_file="${file#./}"

  # Skip allowlisted files.
  allowed=0
  for a in "${ALLOWED[@]}"; do
    if [[ "$rel_file" == "$a" ]]; then
      allowed=1
      break
    fi
  done
  if (( allowed )); then
    continue
  fi

  # Scan every line for `f32` / `f64` identifiers. Skip any line that
  # looks like a comment (`///`, `//!`, `//`, `* `) — docs and inline
  # commentary about floats are fine, actual `f32` / `f64` types /
  # values are the real target.
  while IFS= read -r hit; do
    line_num="${hit%%:*}"
    line_body="${hit#*:}"

    # Strip leading whitespace.
    stripped="${line_body##*([[:space:]])}"
    case "$stripped" in
      "//"*|"/*"*|"*"*|"*/"*) continue ;;
    esac

    printf '%s:%s: unannotated f32/f64 (move to an allowlisted module or refactor to fxp)\n' \
      "$rel_file" "$line_num" >&2
    printf '    %s\n' "$line_body" >&2
    FAIL=1
  done < <(grep -nE '\bf32\b|\bf64\b' "$file" || true)
done < <(find crates -type f -name '*.rs' -path '*/src/*' -not -path '*/target/*' -print0)

if (( FAIL )); then
  cat >&2 <<'HINT'

check-floats.sh: FAIL. Fix by one of:

  - Store the value in a fixed-point type (Micro, Pico, Tempo, Phase)
    and compose an existing Conn to do the conversion at the boundary.
  - If the float genuinely belongs in one of the documented exception
    classes (PI, PCM ABI, ABI-local, argv, Link FFI), add the file to
    this script's ALLOWED list AND amend CLAUDE.md's exception list
    so the gate and the rule stay in sync.

HINT
  exit 1
fi

printf 'check-floats.sh: OK — all f32/f64 uses contained to allowlisted modules.\n'
