# PR #4 — `channel/` + `SampleTickConn`

## Summary

Delivers the Plan 03 per-channel scheduler: pure-logic divider /
shuffle / shift / offset transforms over master tick streams, plus
the §7 `SampleTickConn` shim bridging Tick ↔ Sample in the presence
of `connections::Conn`'s fn-pointer constraint.

### What's new

- **`time::conn::SampleTickConn`** — runtime-parameterised
  `(ceil, inner, floor)` triple mirroring `Conn<Sample, Tick>`. Cannot
  be a genuine `Conn` because its conversion closes over `(sr, bpm)`.
  Laws verified by proptest (`sample_tick_round_trip`,
  `sample_tick_monotonic`, `sample_tick_ceil_ge_floor`).
- **`channel/mode.rs`** — `ChannelMode` enum with the full v0.1 spec
  surface (`MidiClock`, `Din`, `AnalogPulse`, `AnalogLfo`, `MidiCc`);
  only `MidiClock` is rendered in this sprint.
- **`channel/transform.rs`** — `Channel` struct, `ScheduledEvent`,
  and the pure divide → shuffle → Tick-to-Sample → shift → offset
  pipeline. Shift saturates at `[0, 300]` ms; negative shift remains
  deferred to v0.2.
- **`channel/scheduler.rs`** — `tick_stream(channel, stc,
  buffer_start, frames)` returns events whose `sample_index` lands in
  the half-open buffer window. Consecutive buffer calls covering a
  contiguous sample range reproduce the single-call result exactly
  (no dupes, no gaps).
- **CLI** — `agogo channel trace --bpm … --sr … --divider … --frames
  … --buffers …` emits CSV (`buffer_index,sample_index,tick`). The
  plan's build gate (120 BPM / 48 kHz / T4 / 4096-frame buffers →
  events at samples 0, 24 000, 48 000) is asserted as a unit test.
- **PR #1 drive-bys** — "uncuumed" typo in `sync/pll.rs` test comment
  and the `pll_phase_converges` verification-table footnote in
  `plan-2026-04-22-02.md`.

### Verification

All plan-specified properties green:

| Property | Module |
|----------|--------|
| `sample_tick_round_trip` | `time::conn` |
| `sample_tick_monotonic`  | `time::conn` |
| `tick_monotonicity`      | `channel::transform` |
| `divider_rate_preservation` | `channel::transform` |
| `shift_upper_clamp` / `shift_lower_clamp` | `channel::transform` |
| `shuffle_identity_on_even_steps` (replaces plan's `shuffle_zero_mean_per_beat` — see Deviations) | `channel::transform` |
| `scheduler_events_in_window` | `channel::scheduler` |
| `scheduler_block_equivalence` (combines `no_dupes` + `no_gaps`) | `channel::scheduler` |

No new `#[ignore]`s introduced by this sprint.

### Deviations from the plan

Full list in `doc/plans/plan-2026-04-23-01.md` §Review. Highlights:

1. `shuffle_zero_mean_per_beat` can't hold under Haskell's one-sided
   swing (documented in Plan 02); replaced with
   `shuffle_identity_on_even_steps`.
2. `divider_rate_preservation` uses `div_ceil` so dividers coarser
   than a beat (T1, T2) work.
3. `tick_stream` omits `&mut PhaseSource` — unused in v0.1; add back
   in Plan 04 if the MIDI-clock emitter needs live phase.
4. `MidiCc { cc, range }` uses `u8` (no `u7` newtype in core).
5. CLI stays on `clap` — plan's T5 text mis-stated a `bpaf` migration.

### Out of scope / follow-ups for Plan 04

- **MidiClock cadence vs `Channel.divider`.** MidiClock bytes fire
  at PPQN/24, but `divider` is a musical `TBase`. Reconcile before
  writing the emitter (either add `TBase::Tmidi24` or walk the
  master stream directly).
- Negative shift buffer budget (§10 open question) blocks the audio
  callback; spec before Plan 06.
