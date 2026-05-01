# PR #54 — fix: Guard safe merge tracking branches

<!-- gh-id: 3172779774 -->
### Copilot on [`scripts/safe_merge.sh:131`](https://github.com/cmk/agogo/pull/54#discussion_r3172779774) (2026-05-01 09:43 UTC)

For consistency with the other arrays in this script (`repo_args`, `pr_selector`), consider declaring `local_refs` as an array with `declare -a local_refs` before initializing it. This makes the intent explicit and avoids accidental scalar/array confusion later.


<!-- gh-id: 4210741760 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-05-01 09:43 UTC](https://github.com/cmk/agogo/pull/54#pullrequestreview-4210741760))

## Pull request overview

Adds additional local-branch resolution logic to `scripts/safe_merge.sh` so the merge guard also detects unpushed commits that exist on a differently named local branch which tracks the PR head’s remote ref (e.g., `origin/<head_ref>`).

**Changes:**
- Collects candidate local branches: the branch named like the PR head, plus any other local branches whose upstream matches `origin/<head_ref>`.
- Checks each candidate branch for commits ahead of the remote PR head and refuses `gh pr merge` if any are found.


<details>
<summary>Comments suppressed due to low confidence (1)</summary>

**scripts/safe_merge.sh:166**
* The refusal instructions always suggest `git push origin <head_ref>`, but when the ahead commits are on a differently named local branch (`local_branch != head_ref`), that command may fail (no local `<head_ref>` exists) and/or not push the commits that triggered the refusal. Consider tailoring the suggested push command based on `local_branch` (e.g., push `local_branch` to `origin/<head_ref>` when names differ).
```
safe_merge.sh: REFUSING TO MERGE — local branch '$local_branch' is ahead of $upstream.

Unpushed commits would be silently dropped by the merge:

$ahead

Per doc/workflow.md, the merge transition starts from gh_review (push
complete), not round_unpushed. Push first, then re-run:

EOF
  printf '    git push origin %q\n' "$head_ref" >&2
  printf '    %q' "$0" >&2
```
</details>



<!-- gh-id: 3172821564 -->
#### ↳ cmk ([2026-05-01 10:02 UTC](https://github.com/cmk/agogo/pull/54#discussion_r3172821564))

Fixed - local_refs is now explicitly declared with declare -a before initialization, matching the other arrays in the script.
