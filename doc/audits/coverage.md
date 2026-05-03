---
name: coverage
day: tue
paths: [crates/, tests/, doc/plans/, AGENTS.md]
cadence: biweekly
---
You are auditing whether new public surface in agogo has matching test
coverage. The repo's TDD discipline in AGENTS.md requires properties
for parsers, encoders, transformers, and connection-shaped conversions.

Read first:
- AGENTS.md, especially Repository conventions and TDD workflow.
- doc/reviews/review-calibration.md.
- doc/plans/plan-2026-05-02-09.md.

Mechanical pass:

1. Public connection-shaped items without law coverage.
   For every public marker, constant, struct, or function that exposes
   `ceil`, `inner`, `floor`, `ViewL`, `ViewR`, `ConnL`, `ConnR`, or a
   `Conn`-like API, confirm there is a law/property battery covering
   the declared domain. Missing coverage is must-fix.

2. Public parsers, encoders, or transformers without proptests.
   Search newly changed public functions in `crates/`. If a function
   parses, serializes, converts, rounds, schedules, maps units, or
   transforms numeric/data domains, confirm a property test covers it.
   Missing coverage is must-fix.

3. Public modules with no tests.
   Find changed modules that introduce public surface but have no
   `#[cfg(test)]` unit tests and no integration coverage. Flag as
   must-fix unless the module is pure re-export glue.

4. Feature-gated public items without feature-gated tests.
   A public item behind a Cargo feature should have a test that runs
   with that feature or a documented reason it cannot be tested
   locally. Missing coverage is follow-up unless the item is a parser,
   transformer, or connection.

5. Plan Verification drift.
   For the most recent 5 plans, every Verification table property must
   exist in code or appear in that plan's Review section as deferred
   with a concrete re-enablement path.

Judgment pass:

6. Happy-path-only tests for failure-prone APIs.
   Public APIs that reject, saturate, clamp, round, or cross thread/RT
   boundaries need negative or boundary coverage, not only success
   cases.

Output format:
- One section per category that has findings.
- Use exact severity labels `[must-fix]` and `[follow-up]`.
- Each finding: file:line, offending excerpt of at most 5 lines, rule
  violated, and the bug class it enables.
- If there are zero findings, output exactly:
  `no findings`

Anti-themes:
- Historical `doc/plans/` and `doc/reviews/` files preserve past
  decisions. Do not require old plans to match current code unless the
  current branch is implementing that plan.
- Pure re-export modules do not need their own tests when the exported
  item has coverage at its owner.
