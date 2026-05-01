# control-plane — triage of Gemini chat, lock-free parameter bridge

**Source**: note lines 802–1026 (three-thread architecture,
`ChannelParams`, atomic ordering, parameter smoother) and 1850–1926
(base_advance recomputed once per buffer).

**Context**: agogo's standalone CLI + TUI needs a lock-free bridge
between the UI/keyboard thread and the RT audio callback from v0.1
onward. The v0.2 hard-time steel thread makes that bridge explicit:
soft agent/tool commands enter through the host adapter, become typed
RT command envelopes, and either reach the callback by a declared
deadline or are rejected before admission. The design belongs to
agogo core, not to the host adapter.

## Adopt

- **Single-writer-single-reader shape.** Control plane has two
  surfaces: atomic scalars (`AtomicU64`-packed tempo, `AtomicI32`
  per-channel shift, etc.) read per-buffer by the RT thread and
  written by the control thread (TUI keyboard handler, or the
  stdio-core dispatcher adapter), and an `rtrb` SPSC
  queue for events that need to arrive in order (e.g., channel
  reconfigure, preset load). The distinction is "is it safe to miss
  an intermediate value" — if yes, atomic; if no, queue. One writer
  at a time: the TUI and the host adapter don't coexist in
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
  (`crates/core/src/conn/tempo.rs`); the v0.2 bridge stores a plain
  `AtomicU64` carrying the same scaled integer. One tempo
  representation end-to-end, no per-boundary conversion.

## Defer

- **`triple_buffer` for larger-than-atomic state.** v0.2's snapshot
  slot and observation publisher are the other direction and the
  obvious place to use `triple_buffer` or equivalent. Not needed in
  the command bridge while the control plane is scalar-dominant.
- **Parameter dezippering / smoothing.** Gemini's
  `ParameterSmoother` (lines 969–989) is correct for user-facing
  analog-feeling controls (shift ramp, LFO depth), but v0.2's goal
  is admission and deadline correctness, not polish. Defer to the
  first sprint that surfaces a parameter whose step change is audibly
  rough — probably shift-value changes via the TUI.
- **CPU pinning / thread affinity.** Gemini mentions `taskset` and
  thread affinity to stop OS migration. Real concern, but measure
  before tuning — the PLL jitter spec is already being hit on stock
  scheduling. Park this with v0.4 timing diagnostics and return to it
  only if measured output jitter correlates with scheduler migration.

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
  embedded mode the `crates/host/` adapter drops out when the
  dispatcher tears it down. Either way, lifecycle is the lifecycle
  doc's concern, not the control plane's.
