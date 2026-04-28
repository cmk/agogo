#!/usr/bin/env bash
# safe_merge.sh — guard `gh pr merge` against the `replies_amended`
# trap.
#
# `doc/workflow.md`'s state machine has no edge from `replies_amended`
# to `merged`. The only path is `replies_amended → push → gh_review →
# merged`. But `gh pr merge` is a GitHub-side operation; it doesn't
# know about local state. Merging while a fix commit sits unpushed on
# the local branch silently drops it on the floor — the merge takes
# the remote head, and the local commit stays orphaned in the reflog.
#
# This script is the local-side enforcement: it refuses to invoke
# `gh pr merge` if the current branch is ahead of its remote tracking
# ref, regardless of why. Re-run after `git push`.
#
# Usage:
#   scripts/safe_merge.sh <gh-pr-merge-args...>
#
# Examples:
#   scripts/safe_merge.sh 17                      # interactive
#   scripts/safe_merge.sh 17 --rebase --delete-branch
#
# All arguments are forwarded verbatim to `gh pr merge` after the
# guard passes.
set -euo pipefail

if [ $# -lt 1 ]; then
  cat >&2 <<'USAGE'
usage: safe_merge.sh <gh-pr-merge-args...>

Refuses to run if the current branch is ahead of its remote tracking
ref. All arguments are forwarded to `gh pr merge` once the guard
passes.
USAGE
  exit 64
fi

branch=$(git branch --show-current)
if [ -z "$branch" ]; then
  echo "safe_merge.sh: detached HEAD; nothing to compare against." >&2
  exit 1
fi

# Refresh the remote tracking ref so the comparison isn't stale. A
# silent failure here (offline, auth) is fine — the next check will
# still compare against whatever's local, and the user will see if
# something's wrong.
git fetch --quiet origin "$branch" || true

upstream="origin/$branch"
if ! git rev-parse --verify --quiet "$upstream" >/dev/null; then
  echo "safe_merge.sh: no remote tracking ref '$upstream'." >&2
  echo "  push the branch first, then re-run." >&2
  exit 1
fi

ahead=$(git log "$upstream..HEAD" --oneline)
if [ -n "$ahead" ]; then
  cat >&2 <<EOF
safe_merge.sh: REFUSING TO MERGE — local branch is ahead of $upstream.

Unpushed commits would be silently dropped by the merge:

$ahead

Per doc/workflow.md, the merge transition starts from gh_review (push
complete), not replies_amended. Push first, then re-run:

    git push
    $0 $*

EOF
  exit 1
fi

exec gh pr merge "$@"
