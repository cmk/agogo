# PR #49 — stdio-core integration control and observation

## Summary

See `doc/plans/plan-2026-04-30-01.md` and
`doc/plans/plan-2026-04-30-02.md` for the current sprint context.

## Local review (2026-04-30)

**Branch:** `plan-2026-04-30-01`  
**Reviewer:** Codex reviewer

---

The new stdio tool handler can panic on out-of-range user input
rather than returning an error, which can destabilize the adapter. The
rest of the scaffold and tests appear consistent with the stated plan.

### Findings

- **[P2] Return an error instead of panicking for large BPM** —
  `crates/stdio/src/driver.rs:170`

  When a stdio client calls `agogo.tempo.set` with an integer BPM
  above 4294, `parse_u32_field` accepts it but
  `Tempo::from_bpm_integer` panics on overflow. That turns malformed
  tool input into a process unwind instead of the same kind of tool
  error returned for other invalid fields, so validate the tempo range
  before constructing `Tempo`.
