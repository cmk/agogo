# tui — triage of Gemini chat, agogo's TUI (slotting into stdio-core)

**Source**: note lines 794–895 (three-thread architecture + crate
list), 1399–1520 (crossterm keybinding handler, fine/coarse/alt
modifier ladder), 2380–2484 (reactive observation pattern,
flash decay, sparklines), 2596–2687 (auto-scaling sparkline),
2689–2790 (CPU-load monitoring), 2798–2907 (`TuiState` struct and
update method), 2954–3052 (graceful shutdown, panic hook, MIDI
flush).

**Context**: agogo is a standalone CLI tool with a minimal-but-nice
TUI; that's the primary surface. agogo *also* serves as the DSP
foundation for stdio-core, which means the TUI's underlying state
and event shapes must be reusable when stdio-core embeds agogo as a
driver. The critical parts of the stack — FSM, snapshot, control
plane — live in agogo. stdio-core's TUI-FSM guidance
(`../stdio-core/doc/designs/tui-fsm.md`) gets us a free integration
seam if we follow it, but the TUI exists for the standalone case
first.

## Adopt

- **FSM with serializable state, effects-as-data, keybindings
  above the FSM.** The five rules from `tui-fsm.md` are the right
  shape regardless of stdio-core — serializable state keeps the
  TUI testable, keyboard events translated *above* the FSM into
  typed internal events makes rebinding config-not-code, effects
  returned (`Vec<Effect>`) not performed keeps the FSM pure.
  stdio-core integration is a bonus benefit (the form-patch
  protocol lands mechanically, non-keyboard inputs drive the same
  FSM). Use `rust-fsm` for the state-tag transitions; wrapper
  struct holds the data. Same discipline as the transport FSM in
  `transport.md`, different scope.
- **Observation pattern over atomic snapshots, not push.** The RT
  thread writes a compact snapshot (`AgogoSnapshot`) via a wait-
  free primitive — `triple_buffer` or equivalent. The TUI thread
  polls at 30 Hz. The snapshot lives in agogo core and exists for
  the standalone TUI first. v0.2's "publish to stdio-core" work then
  reuses the same type — the host adapter serializes it to
  `ObservationParams` and ships it through `ObservationDispatcher`
  — but the standalone read path isn't downstream of that: both
  consumers read the same wait-free slot.
- **Monotonic `seq` on the snapshot.** Primarily for the TUI's
  flash-decay logic (flash when `seq > last_seen_seq`). Happens
  to also satisfy stdio-core's observation contract (consumer
  detects drops), which is why v0.2's `snapshot_gap_detectable`
  property reads on it.
- **Fine/coarse/alt modifier ladder on nudge keys.** `←/→` = 1
  sample, `Shift+←/→` = 100 samples (~2 ms), `Alt+←/→` = 1 tick
  (50 samples @ 48 k/960). Matches hardware-clock muscle memory.
  Keybindings translate to typed events (`NudgeOffset(delta)`)
  above the FSM.
- **PID sync-error sparkline with auto-scaling.** Green below
  ±1 tick, yellow up to ±8, red beyond. Auto-scale grows
  instantly (catch spikes) but decays slowly (smooth zoom in).
  Round the y-axis to powers of 2 or named musical thresholds
  (1 tick, 8 ticks, ¼ note) so the displayed scale is stable
  across frames.
- **Audio-load gauge measured inside the cpal callback.**
  Measure `Instant::elapsed()` around the render work, divide by
  buffer duration, publish on the snapshot. Red above ~75 % in
  the TUI. The number rides on `AgogoSnapshot` so when stdio-core
  consumes the snapshot, agents / external monitors get it too —
  no separate instrumentation path.
- **Graceful shutdown + panic hook with terminal restore.**
  Panic hook runs `disable_raw_mode` and `LeaveAlternateScreen`
  before printing the backtrace; otherwise a panic leaves the
  user's terminal unusable. Register the hook *before* spawning
  the audio stream so an init failure still cleans up.
- **MIDI dispatcher flushes in-flight messages on shutdown.**
  When the shutdown flag is set, the dispatcher completes any
  partially-transmitted MTC quarter-frame sequence before
  exiting. Otherwise a chasing hardware recorder can be left with
  a torn timecode.

## Defer

- **Storing CPU usage from `sysinfo` alongside the audio-load
  number.** System-wide CPU is informative but the audio-load
  ratio is the one that actually predicts dropouts. Add global
  CPU later if we find it correlates with something the audio-
  load number misses.
- **`tui-logger` for a scrollable in-TUI log pane.** Nice-to-have
  once we have a reason to surface runtime events in the TUI.
  v0.2's stdio-core dispatch already channels structured log
  events out-of-band; don't duplicate.
- **GPU-terminal dev guidance (Alacritty/Kitty).** Gemini lines
  2474–2476. Environmental advice; not a design decision. Park
  in README notes, not in agogo's design docs.

## Reject

- **Rendering the TUI in the same thread that reads the audio
  callback.** Non-starter; cpal's callback is owned by the audio
  device's RT thread. Gemini doesn't actually propose this, but
  some of the snippet-level code is ambiguous enough that it's
  worth explicitly forbidding.
- **Atomics holding *every* TUI-visible value directly on a
  shared `SharedParams`.** The chat's `TuiState` ends up with
  duplicates of every engine atomic. Better: snapshot once per
  frame into a plain local struct (`TuiState` is RAM-local to the
  UI thread) and reference the shared bridge only for atomics
  that actually change at audio rates. Cuts atomic-load traffic
  on the draw path.
- **`ctrlc` crate for signal handling.** Works, and standalone
  agogo will use *something* for SIGINT — whether that's `ctrlc`
  or a `signal-hook`-style receiver plumbed through the TUI
  event loop. Keep whichever we pick confined to `bin/agogo.rs`
  so the library stays host-agnostic: when stdio-core embeds
  agogo, signal handling is the dispatcher's job and agogo's
  driver adapter exits through `on_unmount` instead.
