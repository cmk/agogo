# PR #51 — snapshot seqlock review

## Summary

See `doc/plans/plan-2026-04-30-02.md` for sprint context.

This review records the local concurrency finding against the snapshot
handoff and the fix applied to the active snapshot observation branch.

## Local review (2026-04-30)

**Branch:** `plan-2026-04-30-02`  
**Reviewer:** Codex reviewer

---

The new snapshot handoff can return inconsistent observations under the
intended concurrent RT-writer/async-reader usage on weakly ordered
platforms. Tests pass, but they do not exercise this memory-ordering
race.

### Findings

- **[P2] Use a real seqlock barrier around snapshot writes** —
  `crates/stdio/src/snapshot.rs:207`

  When the async reader races the audio callback, this `Release` store
  does not prevent the following payload stores from becoming visible
  before the odd epoch marker on weakly ordered targets. A reader can
  therefore see updated fields while both epoch loads still read the
  previous even value and return a torn snapshot. Since this slot is
  intended for RT-to-async cross-thread handoff, the epoch protocol
  needs stronger ordering/fences, or another documented single-writer
  seqlock implementation, before `begin_epoch == end_epoch` is trusted.
