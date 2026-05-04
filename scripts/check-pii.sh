#!/usr/bin/env bash
# Fail if a staged diff or tracked tree contains PII or likely secrets.
#
# Default mode scans only lines the commit ADDS (drops unchanged
# context, deletions, and pre-existing content). `--tree` scans every
# tracked file for public-release checks. Both modes check for:
#   - Absolute user-home paths: /Users/<name>/ (macOS), /home/<name>/
#     (Linux) — catches any committer, including CI runner paths
#   - Private-key headers: -----BEGIN ... PRIVATE KEY-----
#   - Cloud/API token shapes: AWS AKIA, GitHub ghp_, Anthropic/OpenAI sk-
#
# Allow-list exceptions live in `.pii-allow` (one extended regex per
# line; blank lines and `#`-comments ignored). A hit is dropped if the
# offending line matches any allow-list pattern, so exceptions stay
# explicit and reviewable. `/home/runner/` paths in CI docs are a
# typical allow-list candidate.
#
# Default runtime is O(diff), not O(repo).
set -euo pipefail

mode=staged
case "${1:-}" in
  "" | "--staged")
    mode=staged
    ;;
  "--tree")
    mode=tree
    ;;
  "-h" | "--help")
    cat <<'USAGE'
usage: scripts/check-pii.sh [--staged|--tree]

  --staged  scan newly-added staged lines (default; used by hooks)
  --tree    scan all tracked files for public-release readiness
USAGE
    exit 0
    ;;
  *)
    echo "error: unknown option: $1" >&2
    echo "usage: scripts/check-pii.sh [--staged|--tree]" >&2
    exit 2
    ;;
esac

patterns=(
  '/Users/[a-zA-Z0-9._-]+/'
  '/home/[a-zA-Z0-9._-]+/'
  '-----BEGIN [A-Z ]*PRIVATE KEY-----'
  'AKIA[0-9A-Z]{16}'
  'ghp_[A-Za-z0-9]{36}'
  'sk-[A-Za-z0-9]{20,}'
)
alt=$(IFS='|'; echo "${patterns[*]}")

filter_allowed() {
  if [ ! -f .pii-allow ]; then
    cat
    return
  fi

  allow_patterns=$(grep -vE '^\s*(#|$)' .pii-allow || true)
  if [ -z "$allow_patterns" ]; then
    cat
    return
  fi

  grep -vE -f <(printf '%s\n' "$allow_patterns") || true
}

report=''

if [ "$mode" = "tree" ]; then
  # Read names via NUL terminators so paths with spaces/newlines survive.
  # The script and the allow-list itself are skipped so self-inclusion
  # of the patterns doesn't trip the check.
  while IFS= read -r -d '' f; do
    [ -z "$f" ] && continue
    matches=$(grep -nE "$alt" -- "$f" || true)
    [ -z "$matches" ] && continue

    matches=$(printf '%s\n' "$matches" | filter_allowed)
    [ -z "$matches" ] && continue

    report+="  $f:"$'\n'
    while IFS= read -r line; do
      report+="    $line"$'\n'
    done <<< "$matches"
  done < <(
    git ls-files -z -- \
      . ':(exclude)scripts/check-pii.sh' ':(exclude).pii-allow'
  )
else
  # ACMR = Added / Copied / Modified / Renamed; excludes pure deletions.
  while IFS= read -r -d '' f; do
    [ -z "$f" ] && continue
    added=$(git diff --cached -U0 --no-color -- "$f" \
      | grep -E '^\+[^+]' | sed 's/^+//' || true)
    [ -z "$added" ] && continue

    matches=$(printf '%s\n' "$added" | grep -E "$alt" || true)
    [ -z "$matches" ] && continue

    matches=$(printf '%s\n' "$matches" | filter_allowed)
    [ -z "$matches" ] && continue

    report+="  $f:"$'\n'
    while IFS= read -r line; do
      report+="    $line"$'\n'
    done <<< "$matches"
  done < <(
    git diff --cached --name-only -z --diff-filter=ACMR -- \
      . ':(exclude)scripts/check-pii.sh' ':(exclude).pii-allow'
  )
fi

if [ -z "$report" ]; then
  exit 0
fi

{
  if [ "$mode" = "tree" ]; then
    echo "error: tracked tree contains potential PII or secrets:"
  else
    echo "error: staged diff contains potential PII or secrets:"
  fi
  printf '%s' "$report"
  echo "  If these are false positives, add a regex to .pii-allow and rerun the check."
} >&2
exit 1
