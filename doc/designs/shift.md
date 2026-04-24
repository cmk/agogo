# shift — triage of Gemini chat, negative-offset lookahead

**Source**: note lines 563–685 (the "Circular Delay Line with
Read-Ahead" / `CompensatedChannel`).

**Context**: agogo.md §2 and §6 call out Shift (±300 ms against
master) as one of the two transforms that live at the Sample layer
rather than the grid layer. v0.1 Plan 03 ships `channel/transform.rs`
with divider/shuffle/shift/offset; the shift *math* is trivial
(`at_sample + k`), but the **negative** case — playing a pulse
earlier than the master — needs a lookahead budget and is the
substance of this design.

## Adopt

- **Global system-latency budget + per-channel signed shift.** The
  engine delays *all* channels by a constant `SYSTEM_LATENCY`
  (enough to absorb the largest negative shift the user can
  request), then each channel's effective delay is
  `SYSTEM_LATENCY - shift_samples`. A channel with `shift = 0`
  plays at `SYSTEM_LATENCY`; positive shift plays later, negative
  plays earlier. This is the only physically realizable shape and
  matches the original hardware's "pre-roll" requirement.
- **±300 ms lookahead matches the hardware.** No reason to exceed
  it in v0.1; 300 ms at 48 k is 14_400 samples which is a trivial
  ring-buffer footprint. Budget `SYSTEM_LATENCY = 14_400` samples
  (`≥ 300 ms` at 48 k, `≥ 150 ms` at 96 k) and call it a constant
  until a user complains. Flag as an agogo.md §10 open question
  resolved.
- **Work in Tick space, not Sample space, until the output
  boundary.** Gemini's delay line holds `f32` samples; ours should
  hold `Tick` events tagged with their intended `Sample` target.
  This keeps shift composable with the rest of the Tick-master
  pipeline — a tempo change in the shift window doesn't shear the
  buffered events because they're still Ticks.
- **Report the lookahead as the engine's "plugin delay
  compensation" value.** Any downstream consumer (stdio-core,
  future DAW embedding) that needs to sync non-agogo audio against
  agogo's output reads this number and compensates.

## Defer

- **Smooth shift-value interpolation ("don't jump from 0 to 500
  samples mid-buffer").** Gemini's `ParameterSmoother` in
  lines 969–989. Correct concern, but for v0.1 Plan 03 (pure-logic
  transform) we just accept step changes. Dezippering lives on top
  of `SharedParams` and is a v0.3 concern; cross-reference from
  `control-plane.md`.
- **Crossfading the delay-line read pointer during large shift
  jumps.** Same concern as above, one level higher — if the user
  drags the shift knob from +200 ms to -200 ms in one frame, the
  read pointer jumps 400 ms' worth. Acceptable at the CLI/atomic
  level; matters only once the TUI surface makes it easy to do
  by accident.

## Reject

- **Storing the rendered `f32` audio in the ring buffer.** Our
  shift operates on scheduled `Tick` events, not rendered audio.
  Storing rendered samples would commit us to re-rendering LFO
  output after shift, which is backwards — shift the event, then
  render.
- **Reporting `SYSTEM_LATENCY` to the OS as "Plugin Delay
  Compensation".** agogo is not a VST/AU plugin (per agogo.md §11);
  there is no host to report PDC to. The value is a
  library-visible constant and a CLI-visible number in the status
  output.
