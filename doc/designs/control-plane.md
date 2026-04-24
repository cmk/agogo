# control-plane — triage of Gemini chat, lock-free parameter bridge

**Source**: note lines 802–1026 (three-thread architecture,
`ChannelParams`, atomic ordering, parameter smoother) and 1850–1926
(base_advance recomputed once per buffer).

**Context**: agogo's standalone CLI + TUI needs a lock-free bridge
between the UI/keyboard thread and the RT audio callback from v0.1
onward — this is not invented by v0.3. What v0.3 adds is a *second*
writer on the same bridge: the `crates/stdio/` adapter that lets a
stdio-core dispatcher issue tool calls alongside (or instead of) the
local TUI. v0.3 sprint 02 is named "Lock-free control plane" and
calls out this file as the landing spot, but the design belongs to
agogo core, not to the stdio-core adapter.

## Adopt

- **Single-writer-single-reader shape.** Control plane has two
  surfaces: atomic scalars (`AtomicU64`-packed tempo, `AtomicI32`
  per-channel shift, etc.) read per-buffer by the RT thread and
  written by the control thread (TUI keyboard handler, or — in
  v0.3 — the stdio-core dispatcher adapter), and an `rtrb` SPSC
  queue for events that need to arrive in order (e.g., channel
  reconfigure, preset load). The distinction is "is it safe to miss
  an intermediate value" — if yes, atomic; if no, queue. One writer
  at a time: the TUI and the stdio-core adapter don't coexist in
  the same process; embedding via stdio-core replaces the TUI's
  keyboard thread with the dispatcher, leaving SWSR intact.
- **Read-once-per-buffer, not per-sample.** The RT callback snapshots
  every atomic parameter at the top of the buffer into a local
  struct, then uses that snapshot for every sample in the buffer.
  This avoids atomic-load overhead inside the sample loop and
  guarantees every sample in a buffer sees a consistent parameter
  set. Gemini gets this right in lines 940–962.
- **`Ordering::Relaxed` is correct here.** The RT side doesn't need
  release/acquire pairing with the writer because none of the
  values protect other memory. The worst case of a torn read is
  that the next buffer sees the new value instead of this one,
  which is exactly the latency budget the control plane is sold on.
- **Pack BPM as scaled integer.** agogo's `Tempo` already does this
  (`crates/core/src/fxp.rs`); the v0.3 bridge stores a plain
  `AtomicU64` carrying the same scaled integer. One tempo
  representation end-to-end, no per-boundary conversion.

## Defer

- **`triple_buffer` for larger-than-atomic state.** v0.4's snapshot
  push (agogo → observation) is the other direction and the obvious
  place to use `triple_buffer` or equivalent. Not needed in v0.3 if
  the control plane is scalar-dominant.
- **Parameter dezippering / smoothing.** Gemini's
  `ParameterSmoother` (lines 969–989) is correct for user-facing
  analog-feeling controls (shift ramp, LFO depth), but v0.3's goal
  is dispatch correctness, not polish. Defer to the first sprint
  that surfaces a parameter whose step change is audibly rough —
  probably shift-value changes via the TUI.
- **CPU pinning / thread affinity.** Gemini mentions `taskset` and
  thread affinity to stop OS migration. Real concern, but measure
  before tuning — Plan 02's PLL jitter spec is already being hit
  on stock scheduling. Park this in the v0.4 review notes and
  return to it only if we see spikes correlated with scheduler
  migration.

## Reject

- **`Arc<Mutex<_>>` anywhere in the data path.** Non-starter for
  RT audio; not considered. Flagged here because some of the chat's
  midstream examples relax into `Mutex`-like patterns, and I want
  that pattern to be explicitly forbidden in the control-plane
  design.
- **Priority inversion being a problem we need to defend against.**
  With atomics + SPSC there is nothing to invert on. Gemini raises
  this as a virtue of the pattern, which it is, but we don't need
  to *design for* it — it's a consequence of not having locks.
- **`tokio_util::CancellationToken` for shutdown.** Gemini's
  shutdown example threads this through the whole app. In
  standalone mode agogo's CLI already has a shutdown path via the
  TUI's `q` key / SIGINT handler (see `tui.md`); in stdio-core
  embedded mode the `crates/stdio/` adapter drops out when the
  dispatcher tears it down. Either way, lifecycle is the lifecycle
  doc's concern, not the control plane's.
