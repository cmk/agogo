---
name: hygiene
day: wed
paths: [crates/, scripts/, AGENTS.md]
cadence: monthly
---
You are doing a mechanical hygiene sweep of agogo. These findings are
cleanup items unless they hide correctness or safety issues.

Read first:
- AGENTS.md, especially editing constraints and repository conventions.

Mechanical pass only:

1. Dead suppressions.
   Pattern: `#[allow(dead_code)]`, `#[allow(unused_imports)]`,
   `#[allow(unused_variables)]`, or clippy allows without a nearby
   reason. Check whether the suppression still has a purpose.

2. Leading-underscore parameters without a reason.
   Pattern: function parameters named `_foo` outside trait impls or
   ABI-required signatures. If the value is truly unused and not
   required by a signature, it should usually be removed.

3. Removed-code comments.
   Pattern: comments like "removed", "was", "old", "legacy path",
   or commented-out code. Flag unless it is explaining a deliberate
   compatibility boundary.

4. Bare TODO/FIXME.
   Flag `TODO` or `FIXME` without a plan, issue, PR, date, or owner.

5. Script/prose command drift.
   Check script names mentioned in AGENTS.md and docs still exist and
   have executable bits when intended.

6. Local helper drift.
   Flag small helper functions/macros that only rename one call or hide
   a single conversion, especially in connection/numeric code.

Output format:
- Bulleted list grouped by category.
- Use `[follow-up]` by default; use `[must-fix]` only when the hygiene
  issue can cause incorrect behavior or bypasses a safety rule.
- If there are zero findings, output exactly:
  `no findings`

Anti-themes:
- Do not flag intentionally historical prose in `doc/plans/` or
  `doc/reviews/`.
- Test helper names may be descriptive rather than minimal; do not flag
  style-only naming.
