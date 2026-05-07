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
