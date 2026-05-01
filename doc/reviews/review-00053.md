# PR #53 — fix: Port workflow FSM follow-ups

<!-- gh-id: 3172629681 -->
### Copilot on [`scripts/safe_merge.sh:78`](https://github.com/cmk/agogo/pull/53#discussion_r3172629681) (2026-05-01 08:40 UTC)

`safe_merge.sh` forwards all args to `gh pr merge`, but the guard’s `gh pr view` call ignores repo-selection flags like `-R/--repo`. If a user runs `scripts/safe_merge.sh 17 -R other/repo ...`, the guard will resolve headRefName for PR 17 in the *current* repo, while the merge targets PR 17 in `other/repo`, so the safety check can apply to the wrong branch/PR. Fix by parsing `-R/--repo` (and its value) and including it in `head_ref_cmd` (or explicitly rejecting `-R/--repo` usage with a clear error).
```suggestion
declare -a repo_args
repo_args=()
expect_repo_value=false
for arg in "$@"; do
  if [ "$expect_repo_value" = true ]; then
    repo_args+=("$arg")
    expect_repo_value=false
    continue
  fi

  case "$arg" in
    -R|--repo)
      repo_args+=("$arg")
      expect_repo_value=true
      ;;
    --repo=*)
      repo_args+=("$arg")
      ;;
  esac
done

if [ ${#pr_selector[@]} -gt 0 ]; then
  head_ref_cmd=(gh pr view "${pr_selector[@]}" "${repo_args[@]}" --json headRefName --jq .headRefName)
else
  head_ref_cmd=(gh pr view "${repo_args[@]}" --json headRefName --jq .headRefName)
```

<!-- gh-id: 3172629696 -->
### Copilot on [`scripts/safe_merge.sh:84`](https://github.com/cmk/agogo/pull/53#discussion_r3172629696) (2026-05-01 08:40 UTC)

The failure hint here assumes an auth issue (“authenticated to gh”), but the same path occurs when the `gh` CLI is not installed / not on PATH. Consider adding an explicit `command -v gh` preflight (similar to other scripts) and tailoring the error message so it’s accurate in both cases.

<!-- gh-id: 3172629705 -->
### Copilot on [`scripts/safe_merge.sh:39`](https://github.com/cmk/agogo/pull/53#discussion_r3172629705) (2026-05-01 08:40 UTC)

This script’s no-arg behavior and help exit code have changed (now runs against the current branch’s open PR by default, and `-h/--help` exits 0). There are repo docs/review records that still describe the old “no args => usage exit 64” behavior; those should be updated so the documented workflow matches what the script does.

<!-- gh-id: 4210585959 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-05-01 08:40 UTC](https://github.com/cmk/agogo/pull/53#pullrequestreview-4210585959))

## Pull request overview

This PR refines the repo’s workflow state-machine tooling and automation docs by making the shell scripts more robust in environments without `gh`, tightening merge-guard behavior, and adjusting local review and `/watch-pr` instructions to match current CLI constraints.

**Changes:**
- Make `workflow_state.sh` tolerate missing `gh` and improve output robustness (avoid heredoc/subshell status formatting).
- Update `safe_merge.sh` argument handling/help output and improve refusal messaging quoting.
- Adjust `local_review.sh` to use a `codex review --base` invocation compatible with current Codex CLI behavior; update `/watch-pr` commit instructions.

### Reviewed changes

Copilot reviewed 4 out of 4 changed files in this pull request and generated 4 comments.

| File | Description |
| ---- | ----------- |
| scripts/workflow_state.sh | Avoids calling `gh` when unavailable; improves review-file fallback and output formatting. |
| scripts/safe_merge.sh | Refines help/usage, selector/flag ordering enforcement, and safer command echoing when refusing to merge. |
| scripts/local_review.sh | Switches to a Codex CLI invocation that works with `--base` without a custom prompt. |
| .claude/commands/watch-pr.md | Updates the round-commit procedure and adds counters to drive commit behavior. |





<!-- gh-id: 3172629715 -->
### Copilot on [`.claude/commands/watch-pr.md:109`](https://github.com/cmk/agogo/pull/53#discussion_r3172629715) (2026-05-01 08:40 UTC)

These counters are later used with `-eq` integer comparisons in the commit-step snippet. To keep the instructions copy/paste-safe, it would help to explicitly say they must be initialized to `0` and incremented as integers (otherwise an unset/empty value will cause `[ ... -eq ... ]` to error).
```suggestion
auto_fix_count=0
reply_count=0

# Increment as integers while classifying threads:
# auto-fix thread:
auto_fix_count=$((auto_fix_count + 1))
reply_count=$((reply_count + 1))

# push-back or defer thread:
reply_count=$((reply_count + 1))
```

<!-- gh-id: 3172649186 -->
#### ↳ cmk ([2026-05-01 08:49 UTC](https://github.com/cmk/agogo/pull/53#discussion_r3172649186))

Fixed - safe_merge.sh now extracts -R/--repo / --repo=... arguments and includes them in the guarded gh pr view, so the guard and forwarded merge target use the same repo.

<!-- gh-id: 3172649490 -->
#### ↳ cmk ([2026-05-01 08:49 UTC](https://github.com/cmk/agogo/pull/53#discussion_r3172649490))

Left historical review records unchanged - they describe PR #41 at the time and are audit artifacts. Active workflow docs and script help now describe the current no-arg/default-current-PR behavior.

<!-- gh-id: 3172649509 -->
#### ↳ cmk ([2026-05-01 08:49 UTC](https://github.com/cmk/agogo/pull/53#discussion_r3172649509))

Fixed - added an explicit gh CLI preflight with a missing-CLI error before any gh pr view call.

<!-- gh-id: 3172649655 -->
#### ↳ cmk ([2026-05-01 08:49 UTC](https://github.com/cmk/agogo/pull/53#discussion_r3172649655))

Fixed - /watch-pr now initializes auto_fix_count=0 and reply_count=0 and shows integer increments during triage before the commit step uses -eq.
