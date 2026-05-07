#!/usr/bin/env bash
# check_boundary_panics.sh — gate user-input validation out of bridge/scheduler panics.

set -euo pipefail

repo_root=$(git rev-parse --show-toplevel)
cd "$repo_root"

python3 - <<'PY'
from __future__ import annotations

from pathlib import Path
import re
import sys

PATHS = [
    Path("crates/chan/src/channel/time.rs"),
    Path("crates/core/src/event.rs"),
    Path("crates/core/src/transport.rs"),
    Path("crates/core/src/bridge.rs"),
    Path("crates/core/src/driver.rs"),
    Path("crates/host-cpal/src/cpal/callback.rs"),
]

PATTERN = re.compile(
    r"(?<!debug_)assert!\s*\(|(?<!debug_)assert_eq!\s*\(|(?<!debug_)assert_ne!\s*\(|"
    r"\.expect\s*\(|\.unwrap\s*\(|panic!\s*\(|unreachable!\s*\(|"
    r"todo!\s*\(|unimplemented!\s*\("
)

ALLOW = "boundary-panic-ok:"


def production_lines(path: Path) -> list[tuple[int, str]]:
    lines = path.read_text().splitlines()
    out: list[tuple[int, str]] = []
    skip_rest = False
    pending_cfg_test = False
    for idx, line in enumerate(lines, start=1):
        stripped = line.strip()
        if skip_rest:
            continue
        if stripped.startswith("#[cfg(test)]"):
            pending_cfg_test = True
            continue
        if pending_cfg_test and stripped.startswith("mod tests"):
            skip_rest = True
            continue
        pending_cfg_test = False
        out.append((idx, line))
    return out


violations: list[tuple[Path, int, str]] = []

for path in PATHS:
    lines = production_lines(path)
    for i, (lineno, line) in enumerate(lines):
        if not PATTERN.search(line):
            continue
        previous = lines[i - 1][1] if i > 0 else ""
        if ALLOW in line or ALLOW in previous:
            continue
        violations.append((path, lineno, line.strip()))

if violations:
    print("check_boundary_panics.sh: bridge/scheduler panic-like calls found.", file=sys.stderr)
    print(
        "Move user-reachable validation to a parser/config boundary, or annotate true "
        "internal invariants with `// boundary-panic-ok: ...`.",
        file=sys.stderr,
    )
    for path, lineno, line in violations:
        print(f"{path}:{lineno}: {line}", file=sys.stderr)
    sys.exit(1)

print("check_boundary_panics.sh: OK — bridge/scheduler panic-like calls are annotated or absent.")
PY
