# link — triage of Gemini chat, Ableton Link integration

**Source**: note lines 1028–1222 (Link session state, timeline
mapping, CPAL `playback` timestamp, start/stop), 1750–1848 (quantum
calculation, wrapped-error PID math), 1519–1631 (Link as a
`PhaseSource` inside the transport FSM).

**Context**: v0.3 owns Link follower/source behavior. The existing
`agogo-host-link` scaffolding wraps `rusty_link`; v0.3 turns that
session surface into a clock-domain implementation that can follow
or publish Link while preserving agogo's integer tick stream. This
doc is the Link-specific half of that work; the PID/FSM sides live
in `pid.md` and `transport.md`.

## Adopt

- **Treat Link as the "ideal" reference and drag agogo's phase
  with a PID, never hard-assign.** Link's `beat_at_time` is
  continuous-float; agogo's phase is `Tick(u32)`. Assigning would
  produce a jump on every callback because of rounding. Running a
  PID over the error keeps agogo's tick stream internally smooth
  while staying externally phase-locked. This is the core v0.3 Link
  follower design.
- **Query Link with cpal's `playback` timestamp, not `callback`.**
  `info.timestamp().playback` is the instant the first sample of
  this buffer will hit the DAC. Asking Link where the beat will be
  *then* rather than *now* folds output latency into the sync
  calculation — the impulse exits the jack at the musically-correct
  moment. Corresponds to agogo.md §4's "precision crown jewel"
  promise.
- **Call `capture_app_session_state()` once per buffer, not per
  sample.** The call is heavy enough that per-sample use would
  blow the RT budget. One call sets the trajectory for the buffer;
  the sample loop runs the PID-nudged increment from there. Also
  matches the control-plane rule of "snapshot at the top of the
  buffer, reuse within."
- **Map time-signature → Link quantum.** `quantum = numerator *
  (4 / denominator)`. 4/4 → 4.0, 3/4 → 3.0, 7/8 → 3.5, 5/4 → 5.0.
  Stored on the `SharedParams` so the TUI / tool-call side can
  mutate it without racing the RT thread.
- **Wrapped-error PID.** The Link timeline wraps at the quantum; a
  naive `target - current` can see a near-quantum-sized error that
  is really a near-zero error in the other direction. Gemini's
  `calculate_wrapped_error` (lines 1819–1831) is the right shape —
  subtract `quantum_ticks` if the error exceeds half-quantum. Put
  it in `sync/link.rs` next to the PID wrapper.
- **Link 3.0 start/stop sync.** If Link goes `is_playing = true`
  and agogo was stopped, reset phase to `target_phase` and zero
  the PID integrator. Matches the transport FSM's
  `Stopped → Starting → Running` path in `transport.md`.

## Defer

- **Publishing "jump events" to the TUI as a sticky flag.** Gemini
  lines 1388–1394. Cleaner approach: route through the `agogo-state`
  snapshot — transport state + last-sync-state become
  fields on `AgogoSnapshot`, and the sticky-flag behavior lives in
  the TUI-side consumer (or in stdio-core's renderer for
  dispatched TUI flows).
- **Start-up warm-up state.** Gemini lines 1629–1631: lower PID
  gain for the first quarter note while network jitter settles.
  Defer until we observe an actual lock-in artifact; the existing
  PLL doesn't need this.

## Reject

- **Mapping agogo's `Tick` to `f64` mid-buffer for Link comparison.**
  Keep the conversion one-way: Link's `f64` beat → agogo's
  fixed-point target → PID error → nudged integer advance.
  Round-tripping agogo ticks back through `f64` for anything other
  than display re-introduces the float drift agogo exists to avoid.
- **Deriving agogo's base advance from `session_state.tempo()`
  every callback.** Tempting but subtly wrong: in internal-master
  mode (v0.1, no Link) the base advance comes from the user's
  BPM setting; in Link mode it comes from the Link session. The
  base-advance source lives on the `PhaseSource` enum variant,
  not on a single code path.
- **C++ wrapper integration effort.** `rusty_link` is a maintained
  Rust binding — use it rather than rolling a new FFI layer. If
  `rusty_link` is unfit for our purposes, file a sprint to improve
  it, not to duplicate it.
