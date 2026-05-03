---
name: proptest
day: mon
paths: [crates/, tests/, doc/plans/, AGENTS.md]
---
You are auditing the agogo repo at the path you are launched in. Your
job is to detect property tests and generators that have been relaxed,
bounded, filtered, or otherwise shaped to hide the failure domain of
the code under test.

Read first:
- AGENTS.md, especially "Property-based testing is mandatory" and the
  connection-construction rule.
- doc/reviews/review-calibration.md.
- doc/plans/plan-2026-05-02-09.md.

Mechanical pass first. If any mechanical check finds a must-fix,
report it and stop.

1. Bounded generators hiding a connection failure domain.
   Pattern: a strategy feeding a connection law test that uses a
   bounded range, `prop_assume!`, `prop_filter`, or a custom "fits"
   predicate to avoid panic, overflow, representability failure, or
   saturation. Known target class: `TICKTIME` using `arb_tick` below
   the `from_ticks` horizon while the public type is `Tick(u64)`.
   Severity: must-fix.

2. Panic-backed connection adjoints.
   Pattern: `expect`, `unwrap`, `panic!`, `unreachable!`, or a
   documented precondition inside a `ceil`, `inner`, or `floor`
   function, or inside a method that claims to mirror the
   `ceil/inner/floor` connection shape. If the public type does not
   encode the precondition, this is an unlawful connection.
   Severity: must-fix.

3. Direct local connection construction.
   Pattern: production code calling `Conn::new_l`, `Conn::new_r`,
   `RuntimeConn::new`, or a local macro that wraps those constructors
   instead of using upstream `connections` declaration/composition
   macros. Flag unless a narrow allowlist comment explains why the
   site is not a public agogo connection.
   Severity: must-fix.

4. Properties listed in a plan but missing in code.
   For the most recent 5 plan files, read the Verification table.
   Every listed property must exist in code or be explicitly deferred
   in that plan's Review section. Missing properties are must-fix.

5. `#[ignore]`d proptests without a re-enablement note.
   Pattern: `#[ignore]` near `proptest!`, `*_galois_*`, `*_law_*`,
   `*_property_*`, or tests in a property module, without a nearby
   reason and re-enable plan.
   Severity: must-fix.

6. Weak proptest configuration.
   Pattern: `cases: N` where N < 32, `max_global_rejects`, or
   `max_local_rejects` tuned to hide rejects, unless immediately
   justified and paired with boundary spot checks.
   Severity: follow-up.

Judgment pass only if the mechanical pass is clean:

7. Boundary-biased strategies whose weights make boundaries
   effectively unreachable. For every `prop_oneof!`, estimate boundary
   probability and flag values below 5% unless justified.

8. Generator comments that describe "realistic" domains for a law that
   is claimed over a wider Rust type. Realism is not a substitute for
   type-level domain encoding.

Output format:
- One section per category that has findings.
- Use exact severity labels `[must-fix]` and `[follow-up]`.
- Each finding: file:line, offending excerpt of at most 5 lines, rule
  violated, and the bug class it enables.
- If there are zero findings across all checks, output exactly:
  `no findings`

Anti-themes:
- A bounded strategy is acceptable only when the property is explicitly
  about that narrowed type/domain and a separate full-domain or boundary
  test covers the excluded region.
- Historical plan/review files may contain stale examples; use them as
  evidence only when the current code still repeats the pattern.
