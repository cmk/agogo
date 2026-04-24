# transport — triage of Gemini chat, PlayState FSM & hard sync

**Source**: note lines 1302–1398 (hard-sync threshold, double-pulse
problem, anti-windup on jump) and 1519–1631 (`PlayState` enum, the
Stopped/Starting/Running/Realigning transitions, warm-up state).

**Context**: v0.5 Sprint 01 is the Transport FSM — the concrete
replacement for agogo.md §10's open question about NEG/POS one-bar
forerun semantics. This doc triages Gemini's Link-centric FSM sketch
into agogo's shape: transport lives above the `PhaseSource` enum and
its job is to sequence Play/Stop/Locate with forerun alignment,
independent of whether the phase source is Internal, External
(audio-sync PLL), or Link.

## Adopt

- **FSM states: `Stopped | Starting | Running | Realigning`.**
  The four are sufficient and map cleanly onto hardware Multiclock
  behavior. `Starting` is the single-buffer window where
  phase is being assigned (forerun, Link start, or user `Play`).
  `Realigning` is the single-buffer window where the PID is
  bypassed during a hard sync.
- **Hard-sync threshold ≈ 1/8 note.** Above this, PID-nudging
  takes too long to catch up and you get audibly drifted ticks.
  Teleport phase and zero controller state in the same move. The
  exact value (ticks at 960 PPQN: `960 / 8 = 120` ticks) is a
  constant, not configuration.
- **`last_tick_triggered` update on teleport.** Without this, a
  forward jump would make the sample loop think thousands of ticks
  elapsed in one sample and emit a pulse storm. Gemini lines 1344–
  1354 name the bug and the fix. Same rule applies to backward
  jumps — align the "previous" state so the loop doesn't
  re-trigger.
- **Bar-boundary-aligned time-sig changes.** Queue a change from
  the control plane; apply at the first tick where `phase %
  ticks_per_bar == 0`. Prevents downstream sequencers from losing
  "The One" mid-bar. Gemini lines 1716–1724. Matches the hardware
  convention.
- **NEG/POS one-bar forerun.** The Multiclock's forerun semantics
  belong here, even though the chat only glances at them. On
  `Play`, compute the first downbeat and *negatively* offset the
  start time by one bar so downstream gear has a full bar of
  clock pulses before the "real" downbeat. The v0.5 verification
  table already names `transport_forerun_lands_on_bar` as a
  required property.

## Defer

- **`WarmUp` state with reduced PID gain for the first quarter
  note after `Starting → Running`.** Gemini lines 1629–1631.
  Worth having if start-up lock-in is audibly rough; we've not
  observed it yet. Add a fifth FSM state only when the need is
  demonstrated — don't pre-wire it.
- **MMC (MIDI Machine Control) transport integration.** Send
  MMC Stop/Start/Locate to hardware that chases. Belongs alongside
  MTC (see `mtc.md`), not in the core FSM. Park as a post-0.5
  enhancement.

## Reject

- **Single-FSM design where Link state and transport state are
  fused.** Gemini threads Link's `is_playing()` directly into the
  FSM as the primary trigger. That works for Link-mode but not for
  Internal or `External(Pll)` phase sources where there is no
  "Link is playing" signal. The right factoring: transport FSM is
  driven by a `TransportEvent` enum (`Play`, `Stop`, `Locate`,
  `PhaseSourceStart`, `PhaseSourceStop`, …) and each phase source
  adapter translates its native events into that enum.
- **Treating the four states as mutually exclusive with manual
  transition methods.** Use `rust-fsm` (already discussed in the
  stdio-core TUI-FSM design) so transitions are declarative,
  guarded, and typed. States hold no data; data lives on the
  wrapper. Same shape the stdio-core doc prescribes.
