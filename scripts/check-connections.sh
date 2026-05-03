#!/usr/bin/env bash
# check-connections.sh — gate for AGENTS.md connection-construction discipline.
#
# Agogo production code must not construct connections directly with
# `Conn::new_l` / `Conn::new_r` or agogo-local wrappers when an upstream
# connections macro can declare the surface. This check is intentionally
# grep-based: it blocks the recurring failure mode where an unlawful
# connection is hand-built and its tests avoid the failing domain.

set -euo pipefail

repo_root=$(git rev-parse --show-toplevel)
cd "$repo_root"

tmp=$(mktemp)
trap 'rm -f "$tmp"' EXIT

rg -n \
  'Conn::new_[lr]\(|RuntimeConn::new\(|macro_rules! def_conn_marker|def_conn_marker!\(' \
  crates \
  >"$tmp" || true

if [[ ! -s "$tmp" ]]; then
  echo "check-connections.sh: OK — connection construction is macro-backed."
  exit 0
fi

# Temporary migration allowlist for Plan 2026-05-02-09. These sites are
# known unresolved work in the active sprint:
# - `conn/midi.rs` needs an upstream one-sided declaration macro.
# - `time/conn.rs` still owns the local `def_conn_marker!` shim for
#   static markers that have not yet been migrated to upstream macros.
#   `quantize_at` is intentionally demoted from `Conn` to total rounder.
#
# The allowlist exists so this gate can land before those design
# decisions are finished. It must shrink as the sprint progresses.
violations=$(awk '
  /crates\/core\/src\/conn\/midi\.rs:/ { next }
  /crates\/core\/src\/time\/conn\.rs:/ { next }
  { print }
' "$tmp")

if [[ -n "$violations" ]]; then
  cat >&2 <<'MSG'
check-connections.sh: direct connection construction found.

Use upstream `connections` macros (`triple!`, `iso!`, `compose!`,
`compose_l!`, `compose_r!`) or stop and design a lawful type/API.
Do not hide connection-domain failures with hand-built constructors.

Offending sites:
MSG
  printf '%s\n' "$violations" >&2
  exit 1
fi

echo "check-connections.sh: OK — only active Plan 2026-05-02-09 allowlisted sites remain."
