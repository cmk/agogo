# PR #84 - Port template workflow contract

## Summary

Ports the current `template-rust` workflow contract into `agogo`.

- Renames the workflow entrypoints to the current `pr_*`, `git_*`, and underscore script names, and updates AGENTS, workflow docs, Claude commands, hooks, CI, and PR templates to match.
- Keeps `agogo`-specific gates active: float discipline, layer checks, connection construction, boundary panic checks, and the audit harness.
- Adds Python workflow/audit tests and wires `python3 -m unittest` into CI.

Verification:

- `bash -n scripts/*.sh .githooks/pre-commit .githooks/pre-push`
- `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest`
- `scripts/pr_report.py path 1`
- `scripts/workflow_state.sh`
- `scripts/check_pii.sh`
- `scripts/check_layers.sh`
- `scripts/check_floats.sh`
- `scripts/check_connections.sh`
- `scripts/check_boundary_panics.sh`
- `cargo fmt -p agogo-chan -p agogo-core -p agogo-cli -- --check`
- `git diff --cached --check`

## Local review (2026-05-07)

**Branch:** plan/2026-05-07-01
**Commits:** 3 (origin/main..plan/2026-05-07-01)
**Reviewer:** Codex (`codex review --base origin/main`)

---

The patch regresses the PII helper's full-tree mode and leaves several live workflow/audit instructions pointing at deleted commands. These issues would break documented maintenance flows even though the new unit tests pass.

Full review comments:

- [P2] Preserve full-tree PII scanning — scripts/check_pii.sh:36-40
  When a release/audit run uses the existing `scripts/check_pii.sh --tree` mode after this rename, the argument is ignored and the script always scans only `git diff --cached`; with a clean index it exits 0 even if tracked files contain `/Users/...`, private-key headers, or token shapes. This regresses the full-repo PII check from the old script, so keep the `--staged|--tree` parser/tree scan or fail on unknown args instead of silently doing the staged scan.

- [P2] Update the post-push review command name — AGENTS.md:582-582
  After this patch renames `.claude/commands/pull-reviews.md` to `pr-report.md`, the Tier 2 workflow still tells agents to run `/pull-reviews <N>`; when GitHub review activity arrives, following AGENTS will invoke a missing slash command instead of mirroring comments. Update this occurrence and the later “remains available” mention to `/pr-report <N>` or `scripts/pr_report.py reviews <N>`.

- [P2] Retarget active audit docs to renamed scripts — scripts/audit_run.py:28-32
  Because this change removes `scripts/audit.py`, `scripts/audit_state.sh`, and `scripts/check-pii.sh`, the active audit docs/prompts are now stale: `doc/audits/README.md` still has cron/manual commands for the deleted audit scripts, and `doc/audits/pii.md` tells the auditor to read the deleted PII script. Anyone using the audit harness after this rename will hit missing files or give the auditor wrong context, so update those live docs/prompts to `audit_run.py`, `audit_report.sh`, and `check_pii.sh`.

- [P3] Make --check validate PR-number prediction — scripts/pr_review.sh:13-13
  `scripts/pr_review.sh --check` now uses `scripts/pr_report.py path 1`, which succeeds without contacting GitHub; in an unauthenticated or wrong-repo shell the health check passes, but the real transition later runs `scripts/pr_report.py path` with no PR number and fails in `pr_request.sh`. Use the no-arg path/pr-request check here so `--check` catches the same gh auth/repo prerequisite it is meant to preflight.
