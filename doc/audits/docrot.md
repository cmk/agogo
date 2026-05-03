---
name: docrot
day: thu
paths: [crates/, doc/, README.md, AGENTS.md]
---
You are auditing agogo for documentation rot: prose that describes a
future state that is now present, references to renamed/deleted items,
dead intra-doc links, and comments that no longer match behavior.

Read first:
- AGENTS.md.
- doc/reviews/review-calibration.md.

Mechanical pass:

1. Stale future-tense prose.
   Search committed docs and Rust comments outside historical archives
   for "will land", "to be added", "will be", "TODO", "FIXME", and
   "deferred to". If the referenced future event already happened,
   flag as must-fix. If unclear, flag as follow-up.

2. Stale symbol/path references.
   In `crates/`, `README.md`, `AGENTS.md`, `doc/designs/`, and
   `doc/versions/`, verify referenced paths, functions, types,
   constants, and modules still exist. Flag broken references.

3. Broken rustdoc links.
   Run or reason from `cargo doc --workspace --no-deps` expectations.
   Any broken `[`Item`]` link in current source docs is must-fix.

4. README and crate README drift.
   Compare root `README.md` and crate-local READMEs with current module
   names and exported APIs. Flag clear mismatches.

5. Workflow/documentation rule drift.
   If a script enforces a rule differently than AGENTS.md describes,
   flag the mismatch. Examples: hook command lists, float allowlist,
   layer checks, connection-construction checks.

Judgment pass:

6. Comments that describe behavior the code no longer has.
   Sample changed public items and nearby comments. Flag clear
   divergences, especially comments on panic conditions, generator
   bounds, RT safety, and connection totality.

Output format:
- One section per category that has findings.
- Use exact severity labels `[must-fix]` and `[follow-up]`.
- Each finding: file:line, offending excerpt of at most 5 lines, rule
  violated, and the bug class it enables.
- If there are zero findings, output exactly:
  `no findings`

Anti-themes:
- `doc/plans/plan-*.md` and `doc/reviews/review-*.md` are historical
  records. Skip stale prose there unless the current branch's plan is
  being finalized.
- `doc/notes/` is personal, gitignored project context. Do not require
  it to be current.
