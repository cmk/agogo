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

if [[ -s "$tmp" ]]; then
  cat >&2 <<'MSG'
check-connections.sh: direct connection construction found.

Use upstream `connections` macros (`triple!`, `iso!`, `compose!`,
`compose_l!`, `compose_r!`) or stop and design a lawful type/API.
Do not hide connection-domain failures with hand-built constructors.

Offending sites:
MSG
  cat "$tmp" >&2
  exit 1
fi

echo "check-connections.sh: OK — connection construction is macro-backed."
