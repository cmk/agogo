#!/usr/bin/env python3
"""
audit_run.py — recurring audit harness for the agogo repo.

Orchestrates per-audit prompts under doc/audits/<name>.md, runs them
via `codex exec -` (Codex in non-interactive mode) on a per-day schedule,
and appends findings to doc/audits/log.md.

Each audit prompt is a markdown file with a YAML front-matter block:

    ---
    name: hygiene
    day: wed          # mon|tue|wed|thu|fri|sat|sun  (when to run)
    paths: [crates/, tests/]   # path filter for early-exit
    cadence: weekly   # weekly (default) | biweekly | monthly
    ---
    <prompt body>

Subcommands:
  list                 List configured audits.
  run <name>           Run one audit when relevant files changed; use
                       --force to skip the early-exit gate.
  cron-tick            Decide which audits should run today and run
                       them. Honors per-audit `day`, `cadence`, and
                       the early-exit gate (audit_report.sh). Intended
                       cron entry point — runs daily, no-ops most days.

Cron entry (in your crontab, fires daily at 9am):
  PATH=/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin
  0 9 * * *  cd /path/to/agogo && scripts/audit_run.py cron-tick

The early-exit gate uses scripts/audit_report.sh (small companion).
First-run audits the full path set; subsequent runs only audit if
something under the path filter has changed since the last `mark`.

Findings are appended to doc/audits/log.md with a YYYY-MM-DD
timestamp. The log file is committed; its history IS the audit
trail. If an audit reports `no findings`, nothing is appended.
"""

from __future__ import annotations

import argparse
import datetime as dt
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from shutil import which

REPO_ROOT = Path(__file__).resolve().parent.parent
AUDITS_DIR = REPO_ROOT / "doc" / "audits"
LOG_FILE = AUDITS_DIR / "log.md"
STATE_SCRIPT = REPO_ROOT / "scripts" / "audit_report.sh"

DAY_NAMES = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"]
CADENCES = {"weekly", "biweekly", "monthly"}
AUDIT_NAME_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]*$")


def strip_inline_comment(value: str) -> str:
    hash_at = value.find("#")
    if hash_at == -1:
        return value.strip()
    return value[:hash_at].rstrip()


@dataclass
class Audit:
    name: str
    day: str
    paths: list[str]
    cadence: str
    prompt_path: Path
    body: str

    def is_due_today(self, today: dt.date) -> bool:
        """Decide whether this audit should fire today.

        - day mismatch → no
        - weekly cadence + day match → yes
        - biweekly cadence + day match + ISO-week is even → yes
        - monthly cadence + day match + day-of-month ≤ 7 → yes (first
          occurrence of that weekday in the month)
        """
        if DAY_NAMES[today.weekday()] != self.day:
            return False
        if self.cadence == "weekly":
            return True
        if self.cadence == "biweekly":
            return today.isocalendar()[1] % 2 == 0
        if self.cadence == "monthly":
            return today.day <= 7
        raise ValueError(f"unknown cadence after validation: {self.cadence!r}")


def parse_audit(path: Path) -> Audit:
    text = path.read_text(encoding="utf-8")
    if not text.startswith("---\n"):
        raise ValueError(f"{path}: missing YAML front-matter")
    end = text.find("\n---\n", 4)
    if end == -1:
        raise ValueError(f"{path}: front-matter never closes")
    fm_text = text[4:end]
    body = text[end + 5 :].lstrip("\n")

    # Hand-rolled minimal YAML parse — we only support the four known
    # keys. Avoids a PyYAML dependency for a 4-key schema.
    fm: dict[str, str | list[str]] = {}
    for line in fm_text.splitlines():
        line = line.rstrip()
        if not line or line.startswith("#"):
            continue
        if ":" not in line:
            raise ValueError(f"{path}: front-matter line lacks ':' — {line!r}")
        key, _, val = line.partition(":")
        key = key.strip()
        val = strip_inline_comment(val.strip())
        if val.startswith("[") and val.endswith("]"):
            items = [v.strip() for v in val[1:-1].split(",") if v.strip()]
            fm[key] = items
        else:
            fm[key] = val

    name = fm.get("name") or path.stem
    day = fm.get("day", "mon")
    paths = fm.get("paths", ["crates/", "tests/"])
    cadence = fm.get("cadence", "weekly")

    if not isinstance(name, str) or not isinstance(day, str) or not isinstance(cadence, str):
        raise ValueError(f"{path}: name/day/cadence must be scalar strings")
    if not isinstance(paths, list):
        raise ValueError(f"{path}: paths must be a list")
    if not AUDIT_NAME_RE.fullmatch(name):
        raise ValueError(f"{path}: name={name!r} must be a safe basename")
    if day not in DAY_NAMES:
        raise ValueError(f"{path}: day={day!r} not one of {DAY_NAMES}")
    if cadence not in CADENCES:
        raise ValueError(f"{path}: cadence={cadence!r} not one of {sorted(CADENCES)}")

    return Audit(
        name=name,
        day=day,
        paths=paths,
        cadence=cadence,
        prompt_path=path,
        body=body,
    )


def load_audits() -> list[Audit]:
    if not AUDITS_DIR.exists():
        return []
    audits = []
    for path in sorted(AUDITS_DIR.glob("*.md")):
        if path.name == "log.md" or path.name.startswith("README"):
            continue
        audits.append(parse_audit(path))
    return audits


def changed_files_since_last(audit: Audit) -> list[str]:
    """Run audit_report.sh since-last and return the changed paths."""
    result = subprocess.run(
        [str(STATE_SCRIPT), "since-last", audit.name, *audit.paths],
        capture_output=True,
        text=True,
        cwd=REPO_ROOT,
    )
    if result.returncode != 0:
        raise RuntimeError(
            f"audit_report.sh failed for audit '{audit.name}' "
            f"(exit {result.returncode}):\n{result.stderr.strip()}"
        )
    return [line for line in result.stdout.splitlines() if line.strip()]


def tracked_files_for(audit: Audit) -> list[str]:
    """Return all tracked files under an audit's pathspec."""
    result = subprocess.run(
        ["git", "ls-files", "--", *audit.paths],
        capture_output=True,
        text=True,
        cwd=REPO_ROOT,
        check=True,
    )
    return result.stdout.splitlines()


def mark_audited(audit: Audit) -> None:
    subprocess.run(
        [str(STATE_SCRIPT), "mark", audit.name],
        cwd=REPO_ROOT,
        check=True,
    )


def invoke_codex(audit: Audit, changed: list[str], dry_run: bool = False) -> str:
    """Invoke `codex exec -` and return its stdout.

    The prompt is the audit body verbatim plus a file-selection
    preamble so the agent doesn't have to re-discover the scope. If
    `dry_run` is set, prints the prompt to stdout and returns empty
    without invoking Codex.
    """
    preamble = (
        f"# Files selected for audit ({len(changed)} files):\n"
        + "\n".join(f"- {p}" for p in changed[:200])
        + ("\n- ... (truncated)\n" if len(changed) > 200 else "\n")
        + "\n---\n\n"
    )
    full_prompt = preamble + audit.body

    if dry_run:
        print(f"--- dry run: prompt for '{audit.name}' ({len(full_prompt)} chars) ---")
        print(full_prompt)
        return ""

    codex = which("codex")
    if codex is None:
        raise RuntimeError(
            "audit_run.py: `codex` not on PATH. Cron must set PATH explicitly; "
            "see doc/audits/README.md."
        )

    result = subprocess.run(
        [
            codex,
            "exec",
            "--cd",
            str(REPO_ROOT),
            "--sandbox",
            "read-only",
            "--ask-for-approval",
            "never",
            "--ephemeral",
            "--color",
            "never",
            "-",
        ],
        input=full_prompt,
        capture_output=True,
        text=True,
        cwd=REPO_ROOT,
    )
    if result.returncode != 0:
        raise RuntimeError(
            f"codex exec failed for audit '{audit.name}' "
            f"(exit {result.returncode}).\nSTDOUT:\n{result.stdout.strip()}\n\n"
            f"STDERR:\n{result.stderr.strip()}"
        )
    if not result.stdout.strip():
        raise RuntimeError(f"codex exec produced empty output for audit '{audit.name}'")
    return result.stdout


def append_to_log(audit: Audit, output: str, today: dt.date) -> bool:
    """Append findings to doc/audits/log.md unless output is 'no findings'.

    Returns True if anything was appended.
    """
    stripped = output.strip()
    if not stripped or stripped.lower() == "no findings":
        return False

    AUDITS_DIR.mkdir(parents=True, exist_ok=True)
    header_exists = LOG_FILE.exists()
    with LOG_FILE.open("a", encoding="utf-8") as f:
        if not header_exists:
            f.write("# Audit log\n\n")
            f.write(
                "Findings from `scripts/audit_run.py` runs. Each entry: "
                "audit name, date, then the agent's report verbatim.\n\n"
            )
        f.write(f"## {audit.name} — {today.isoformat()}\n\n")
        f.write(stripped)
        f.write("\n\n")
    return True


def cmd_list(_args: argparse.Namespace) -> int:
    audits = load_audits()
    if not audits:
        print("No audits configured. Add markdown files under doc/audits/.")
        return 0
    today = dt.date.today()
    print(f"{'name':<12} {'day':<5} {'cadence':<10} {'due today?':<10} paths")
    print("-" * 60)
    for a in audits:
        due = "yes" if a.is_due_today(today) else "no"
        print(f"{a.name:<12} {a.day:<5} {a.cadence:<10} {due:<10} {','.join(a.paths)}")
    return 0


def cmd_run(args: argparse.Namespace) -> int:
    audits = {a.name: a for a in load_audits()}
    if args.name not in audits:
        print(f"audit_run.py: no audit named {args.name!r}", file=sys.stderr)
        return 2
    audit = audits[args.name]
    changed = changed_files_since_last(audit)
    if args.force:
        # Force mode: audit the full pathspec
        changed = tracked_files_for(audit)
    elif not changed:
        print(f"audit_run.py: '{audit.name}' — no changed files since last run; skip")
        return 0
    print(f"audit_run.py: running '{audit.name}' on {len(changed)} changed files")
    output = invoke_codex(audit, changed, dry_run=args.dry_run)
    if args.dry_run:
        return 0  # don't append to log or move the pin in dry-run mode
    appended = append_to_log(audit, output, dt.date.today())
    mark_audited(audit)
    if appended:
        print(f"audit_run.py: '{audit.name}' findings appended to {LOG_FILE.relative_to(REPO_ROOT)}")
    else:
        print(f"audit_run.py: '{audit.name}' — no findings")
    return 0


def cmd_cron_tick(_args: argparse.Namespace) -> int:
    audits = load_audits()
    today = dt.date.today()
    due = [a for a in audits if a.is_due_today(today)]
    if not due:
        # Silent on no-op days — cron mail stays quiet
        return 0
    failures = []
    for audit in due:
        try:
            changed = changed_files_since_last(audit)
            if not changed:
                continue  # early-exit; nothing changed under audit's paths
            print(f"audit_run.py: cron-tick running '{audit.name}'")
            output = invoke_codex(audit, changed)
            append_to_log(audit, output, today)
            mark_audited(audit)
        except Exception as exc:
            failures.append(f"{audit.name}: {exc}")
            print(f"audit_run.py: cron-tick failed for '{audit.name}': {exc}", file=sys.stderr)
    if failures:
        raise RuntimeError("cron-tick audit failures:\n" + "\n".join(failures))
    return 0


def main() -> int:
    p = argparse.ArgumentParser(prog="audit_run.py", description=__doc__.strip().split("\n")[0])
    sub = p.add_subparsers(dest="cmd", required=True)

    sub.add_parser("list", help="list configured audits and which are due today")

    p_run = sub.add_parser("run", help="run one audit if relevant files changed")
    p_run.add_argument("name", help="audit name (filename stem under doc/audits/)")
    p_run.add_argument(
        "--force", action="store_true",
        help="audit even if nothing changed since last mark (audits the full pathspec)",
    )
    p_run.add_argument(
        "--dry-run", action="store_true",
        help="print the assembled prompt instead of invoking Codex; don't move the pin",
    )

    sub.add_parser(
        "cron-tick",
        help="decide which audits run today and run them; intended cron entry point",
    )

    args = p.parse_args()
    return {
        "list": cmd_list,
        "run": cmd_run,
        "cron-tick": cmd_cron_tick,
    }[args.cmd](args)


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception as exc:
        print(f"audit_run.py: error: {exc}", file=sys.stderr)
        sys.exit(1)
