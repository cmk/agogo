# Recurring audits

Self-hosted audit harness for the agogo repo. Five audits, one master
script, one early-exit gate. Cron-driven; per-audit prompts are
markdown files with small YAML front matter.

## Layout

```
doc/audits/
  README.md           - this file
  log.md              - append-only findings log, created on first finding
  proptest.md         - Mondays, weekly: generator/property relaxation
  docrot.md           - Thursdays, weekly: stale prose, dead refs
  coverage.md         - Tuesdays, biweekly: test coverage drift
  hygiene.md          - Wednesdays, monthly: cleanup sweep
  pii.md              - Fridays, monthly: secret/PII scan

scripts/
  audit.py            - orchestrator, calendar, Codex dispatch, log
  audit_state.sh      - early-exit gate, since-last/mark/last
```

## How a run works

1. Cron fires `scripts/audit.py cron-tick` daily.
2. The script reads each `*.md` under `doc/audits/`, parses front
   matter, and decides which audits are due today.
3. For each due audit, it asks `audit_state.sh since-last <name>
   <paths>` whether anything changed under the audit's path filter
   since the last successful run. If empty, it skips the audit.
4. If non-empty, it invokes `codex exec -` with the audit body plus a
   file-selection preamble.
5. If Codex exits nonzero, is missing from PATH, or returns empty
   output, the harness exits nonzero and does not move the audit pin.
6. If Codex outputs exactly `no findings`, nothing is appended.
   Otherwise the output is appended to `doc/audits/log.md`.
7. The audit's HEAD pin is marked only after the Codex invocation
   succeeds.

## Prompt format

Each audit lives in its own markdown file with front matter:

```markdown
---
name: proptest
day: mon              # mon|tue|wed|thu|fri|sat|sun
paths: [crates/, tests/]
cadence: weekly       # weekly (default) | biweekly | monthly
---
<prompt body - handed to codex exec via stdin with a file-list preamble>
```

Audit names must be safe basenames: ASCII letters/digits followed by
letters, digits, `.`, `_`, or `-`. The name is used as the
`.git/audit-state/<name>` pin filename.

Prompts must instruct Codex to output `no findings` on a clean run.
The harness uses that exact string to decide whether to append to the
audit log.

## Cron entry

Cron does not load your interactive shell. Set PATH explicitly so the
`codex` binary is available:

```cron
PATH=/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin
0 9 * * *  cd /path/to/agogo && scripts/audit.py cron-tick
```

On no-due days, the command exits silently. On due days where nothing
changed under the audit's paths, it exits silently. Any execution
failure exits nonzero so cron mail or the surrounding runner catches it.

## Manual / dev usage

```sh
# List configured audits and which are due today.
scripts/audit.py list

# Print the assembled prompt without invoking Codex or moving the pin.
scripts/audit.py run proptest --force --dry-run

# Run one audit unconditionally.
scripts/audit.py run proptest --force

# Inspect or set the early-exit pin.
scripts/audit_state.sh last proptest
scripts/audit_state.sh mark proptest
```

First run has no `.git/audit-state/<name>` pin, so it audits every
tracked file under the audit's pathspec. Bootstrap deliberately: inspect
`--dry-run`, then either run the first full audit or mark a baseline
after human approval.

## Audit Log

`doc/audits/log.md` is created on first finding and is intended to be
committed as audit history. Do not gitignore it. If a cron run appends
findings, fold the log update into the branch or cleanup commit that
addresses the findings unless the user asks for a standalone audit-log
commit.

## Adding a New Audit

1. Drop `doc/audits/<name>.md` with the front-matter schema above.
2. Body should include: a "Read first" section, a mechanical pass,
   optional judgment pass, output format, and anti-themes.
3. Test prompt assembly with:

```sh
scripts/audit.py run <name> --force --dry-run
```

## Cadence Accounting

| Audit | Day | Cadence | Avg fires/year |
| --- | --- | --- | --- |
| proptest | mon | weekly | 52 |
| docrot | thu | weekly | 52 |
| coverage | tue | biweekly | 26 |
| hygiene | wed | monthly | 12 |
| pii | fri | monthly | 12 |

154 calendar fires/year, with far fewer Codex invocations because the
early-exit gate skips no-change weeks.
