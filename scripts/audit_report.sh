#!/usr/bin/env bash
#
# audit_report.sh — early-exit helper for recurring audit agents
#
# Persists the last-audited HEAD SHA in .git/audit-state/<name> (one
# file per audit name, so multiple audits can pin independently). The
# pin lives inside .git/ so it's untracked by default and isn't shared
# across clones — each developer / runner pins its own audit cadence.
#
# Usage:
#   audit_report.sh since-last <name> [path1 path2 ...]
#       Print files changed under the given paths since the last
#       audit. Empty output means "no relevant changes — skip this
#       audit run." If no pin exists yet, prints all files matching
#       the path filter under HEAD (first run audits everything).
#
#   audit_report.sh mark <name>
#       Update the pin for <name> to current HEAD. Run this after a
#       successful audit so the next run early-exits unless something
#       relevant changes.
#
#   audit_report.sh last <name>
#       Print the pinned SHA (or empty if no pin).
#
# Exit codes:
#   0 — success (output may be empty for since-last with no changes)
#   2 — usage error or unknown subcommand
#
# Example agent loop:
#   files=$(scripts/audit_report.sh since-last hygiene crates/ scripts/)
#   [ -z "$files" ] && { echo "no changes; skipping"; exit 0; }
#   # ... run the audit on $files ...
#   scripts/audit_report.sh mark hygiene

set -euo pipefail

usage() {
    sed -n '3,30p' "$0" >&2
    exit 2
}

validate_name() {
    local name=$1
    if [[ ! "$name" =~ ^[A-Za-z0-9][A-Za-z0-9._-]*$ ]]; then
        echo "audit_report.sh: audit name must be a safe basename: $name" >&2
        exit 2
    fi
}

if [ $# -lt 1 ]; then usage; fi

cmd=$1
shift

# Resolve the audit-state dir relative to the repo root, not pwd.
git_dir=$(git rev-parse --git-dir 2>/dev/null) || {
    echo "audit_report.sh: not inside a git work tree" >&2
    exit 2
}
state_dir="$git_dir/audit-state"

case "$cmd" in
    last)
        if [ $# -ne 1 ]; then usage; fi
        name=$1
        validate_name "$name"
        pin_file="$state_dir/$name"
        [ -f "$pin_file" ] && cat "$pin_file" || true
        ;;

    mark)
        if [ $# -ne 1 ]; then usage; fi
        name=$1
        validate_name "$name"
        mkdir -p "$state_dir"
        pin_file="$state_dir/$name"
        head_sha=$(git rev-parse HEAD)
        echo "$head_sha" > "$pin_file"
        echo "audit_report: pinned $name -> $head_sha" >&2
        ;;

    since-last)
        if [ $# -lt 1 ]; then usage; fi
        name=$1
        validate_name "$name"
        shift
        # Remaining args are pathspecs. If empty, default to the
        # whole repo.
        pin_file="$state_dir/$name"

        if [ -f "$pin_file" ]; then
            last_sha=$(cat "$pin_file")
            # Validate the pin still points at a reachable commit
            # (e.g., after a force-push the pin can dangle). On
            # invalid pin, fall back to "audit everything" rather
            # than failing — safer to over-audit than to skip.
            if git cat-file -e "$last_sha" 2>/dev/null; then
                git diff --name-only "$last_sha" HEAD -- "$@"
            else
                echo "audit_report: pinned SHA $last_sha unreachable; auditing all" >&2
                git ls-files -- "$@"
            fi
        else
            # First run: emit everything under the pathspec so the
            # audit covers the full surface once.
            git ls-files -- "$@"
        fi
        ;;

    *)
        usage
        ;;
esac
