from __future__ import annotations

import argparse
import contextlib
import datetime as dt
import io
import importlib.util
import subprocess
import sys
import unittest
from pathlib import Path
from unittest import mock


REPO_ROOT = Path(__file__).resolve().parents[1]
AUDIT_RUN_PATH = REPO_ROOT / "scripts" / "audit_run.py"

spec = importlib.util.spec_from_file_location("audit_run", AUDIT_RUN_PATH)
assert spec is not None
audit_run = importlib.util.module_from_spec(spec)
assert spec.loader is not None
sys.modules[spec.name] = audit_run
spec.loader.exec_module(audit_run)


def audit(name: str) -> audit_run.Audit:
    today = dt.date.today()
    return audit_run.Audit(
        name=name,
        day=audit_run.DAY_NAMES[today.weekday()],
        paths=["crates/"],
        cadence="weekly",
        prompt_path=REPO_ROOT / "doc" / "audits" / f"{name}.md",
        body="audit body",
    )


class AuditRunTests(unittest.TestCase):
    def test_cmd_run_force_propagates_git_ls_files_failure_without_moving_pin(self) -> None:
        broken = audit("broken")
        error = subprocess.CalledProcessError(
            returncode=128,
            cmd=["git", "ls-files", "--", "crates/"],
            stderr="fatal: bad pathspec",
        )

        with (
            mock.patch.object(audit_run, "load_audits", return_value=[broken]),
            mock.patch.object(audit_run, "changed_files_since_last", return_value=[]),
            mock.patch.object(audit_run, "tracked_files_for", side_effect=error),
            mock.patch.object(audit_run, "invoke_codex") as invoke_codex,
            mock.patch.object(audit_run, "mark_audited") as mark_audited,
        ):
            args = argparse.Namespace(name="broken", force=True, dry_run=False)

            with self.assertRaises(subprocess.CalledProcessError):
                audit_run.cmd_run(args)

        invoke_codex.assert_not_called()
        mark_audited.assert_not_called()

    def test_tracked_files_for_checks_git_ls_files_result(self) -> None:
        configured = audit("hygiene")
        completed = subprocess.CompletedProcess(
            args=["git", "ls-files", "--", "crates/"],
            returncode=0,
            stdout="crates/core/src/lib.rs\n",
            stderr="",
        )

        with mock.patch.object(audit_run.subprocess, "run", return_value=completed) as run:
            self.assertEqual(audit_run.tracked_files_for(configured), ["crates/core/src/lib.rs"])

        run.assert_called_once_with(
            ["git", "ls-files", "--", "crates/"],
            capture_output=True,
            text=True,
            cwd=audit_run.REPO_ROOT,
            check=True,
        )

    def test_cron_tick_continues_due_audits_then_reports_failures(self) -> None:
        first = audit("first")
        second = audit("second")

        def changed_files(audit: audit_run.Audit) -> list[str]:
            if audit.name == "first":
                raise RuntimeError("codex failed")
            return ["crates/core/src/lib.rs"]

        with (
            mock.patch.object(audit_run, "load_audits", return_value=[first, second]),
            mock.patch.object(audit_run, "changed_files_since_last", side_effect=changed_files),
            mock.patch.object(audit_run, "invoke_codex", return_value="no findings") as invoke_codex,
            mock.patch.object(audit_run, "append_to_log", return_value=False) as append_to_log,
            mock.patch.object(audit_run, "mark_audited") as mark_audited,
            contextlib.redirect_stdout(io.StringIO()),
            contextlib.redirect_stderr(io.StringIO()),
        ):
            with self.assertRaisesRegex(RuntimeError, "first: codex failed"):
                audit_run.cmd_cron_tick(argparse.Namespace())

        invoke_codex.assert_called_once_with(second, ["crates/core/src/lib.rs"])
        append_to_log.assert_called_once()
        mark_audited.assert_called_once_with(second)


if __name__ == "__main__":
    unittest.main()
