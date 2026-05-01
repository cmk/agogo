# Cross-plan deferred-work summary

**Generated:** 2026-04-28 by harvesting `## Deferred` sections of every
plan in [`doc/plans/`](plans/) and cross-checking against the current
working tree.

**Purpose.** A thematic snapshot of work past sprints intentionally
punted but didn't close. Picking the next plan should not require
re-reading 36 plan files. Items below are grouped by theme and tagged
**open**, **partial**, **stale**, or **out-of-scope-intentional**.
Each entry back-links to the plan(s) that originated it.

**Refresh policy.** After landing any plan whose `## Deferred` section
adds new bullets, append them here under the right theme. Re-run the
"Resolved since first deferral" sweep at the end of each milestone
(v0.2, v0.3, …) to relocate items that have shipped.

> Out of this summary's scope: `## Review` drift notes (gardener-rule
> follow-ups), `#[ignore]`d proptest tracking, and design-doc roadmaps
> in `doc/agogo.md` / `doc/designs/*`. Each lives in its source plan.

---

## PI / PLL controllers + Link follower

### LpfPid — clocked-style PID controller wrapper
A tightly-typed wrapper around the existing `Pll` that the v0.5 Sprint 02
Link follower will consume. Proven in isolation but unwired.
- Sources: [plan-2026-04-28-03 §Deferred](plans/plan-2026-04-28-03.md),
  [plan-2026-04-28-04 §Deferred](plans/plan-2026-04-28-04.md),
  [plan-2026-04-28-05 §Deferred](plans/plan-2026-04-28-05.md),
  [plan-2026-04-28-06 §Deferred](plans/plan-2026-04-28-06.md)
- Status: **open** — no `crates/core/src/sync/lpf_pid.rs` yet.
- Blocked on: nothing; waiting for the Link-follower sprint.

### TransportState\<S\> typestate skeleton
Compile-time enforcement that "playing" / "stopped" transitions are
exhaustive at the type level, layered on top of the existing `rust-fsm`
declaration in `crates/host-link/src/transport.rs`.
- Sources: [plan-2026-04-28-04 §Deferred](plans/plan-2026-04-28-04.md),
  [plan-2026-04-28-05 §Deferred](plans/plan-2026-04-28-05.md),
  [plan-2026-04-28-06 §Deferred](plans/plan-2026-04-28-06.md)
- Status: **open**.
- Blocked on: ordering with NEG/POS forerun work.

### RelativeClock calibration helper
Building block for the MIDI-input adapter — converts external clock
ticks into the agogo internal timebase with calibrated drift.
- Sources: [plan-2026-04-28-04 §Deferred](plans/plan-2026-04-28-04.md),
  [plan-2026-04-28-05 §Deferred](plans/plan-2026-04-28-05.md),
  [plan-2026-04-28-06 §Deferred](plans/plan-2026-04-28-06.md)
- Status: **open**.

### MIDI-input adapter
The `midir` adapter that consumes `RelativeClock` and feeds the PLL.
Plan 28-03 noted `RelativeClock` is the prerequisite.
- Sources: [plan-2026-04-28-03 §Deferred](plans/plan-2026-04-28-03.md)
- Status: **open** — blocked on `RelativeClock`.

### Hard-sync phase teleport
Above ~⅛-note phase error: bypass the PID, teleport phase, zero
controller state, update `last_tick_triggered` to prevent pulse storm.
v0.5 transport scope per `pid.md:40-43` + `transport.md:24-33`.
- Sources: [plan-2026-04-25-03 §Deferred](plans/plan-2026-04-25-03.md)
- Status: **open** — v0.5.

### PID-smoothed Link follower wiring
`LpfPid` consumed by `LinkClock` so `--source=link` becomes a
PID-smoothed phase source rather than the raw bridge.
- Sources: [plan-2026-04-25-03 §Deferred](plans/plan-2026-04-25-03.md),
  [plan-2026-04-25-05 §Deferred](plans/plan-2026-04-25-05.md)
- Status: **open** — v0.5 Sprint 02. Depends on `LpfPid`.

### `f64_bpm_to_tempo` / `f64_beats_to_quantum` re-base on lawful Conns
The argv-boundary helpers in `boundary.rs` and
`host-link/src/quantum.rs` still hand-roll the conversion.
- Sources: [plan-2026-04-26-04 §Deferred](plans/plan-2026-04-26-04.md),
  [plan-2026-04-28-01 §Deferred](plans/plan-2026-04-28-01.md)
- Status: **partial** — `Phase` is genuinely a wrapping quotient
  (not a monotone Conn), so `f64_phase_to_phase` is the named exception
  per CLAUDE.md. `Tempo` and `Quantum` are still candidates for
  composing through existing F-ladder rungs via `compose!`.

### Forerun transport (NEG/POS FSM extension)
`PreRoll` + a forerun-aware stop-pending state on the transport FSM.
The current `{Stopped, Playing}` shim was explicitly designed to be
extended (see `crates/host-link/src/transport.rs:1-7`).
- Sources: [plan-2026-04-23-01 §Deferred](plans/plan-2026-04-23-01.md),
  [plan-2026-04-25-03 §Deferred](plans/plan-2026-04-25-03.md),
  [plan-2026-04-25-05 §Deferred](plans/plan-2026-04-25-05.md)
- Status: **open** — v0.5 Sprint 01.

---

## Real-time audio plumbing

### Atomic parameter bridge (live BPM knob)
Control-thread → RT scalar updates for BPM, shift, offset. Plan 13
shipped the rtrb SPSC drain half (RT → control) but not the
control → RT half. Plan 14's BPM is locked at startup.
- Sources: [plan-2026-04-25-03 §Deferred](plans/plan-2026-04-25-03.md),
  [plan-2026-04-25-05 §Deferred](plans/plan-2026-04-25-05.md)
- Status: **open** — v0.3 per `control-plane.md:17-43`.

### Audio-load gauge in callback
Measure `Instant::elapsed()` around `on_buffer`'s render, divide by
buffer duration, publish via a second `AtomicU32` alongside
`dropped_count()`.
- Sources: [plan-2026-04-25-03 §Deferred](plans/plan-2026-04-25-03.md),
  [plan-2026-04-25-05 §Deferred](plans/plan-2026-04-25-05.md)
- Status: **open** — v0.4 per `tui.md:59-64`.

### Output buffer writes (CV pulse, analog LFO)
Plan 13 stubs `io.output` empty (CV out is v0.4's `out/audio`). Rule
when v0.4 fills it: write every sample, don't preclear — cpal buffers
have undefined contents.
- Sources: [plan-2026-04-25-03 §Deferred](plans/plan-2026-04-25-03.md)
- Status: **open** — v0.4 per `cv-pulse.md:47-52`.

### Telemetry / observation push
Beyond `dropped_count()`, structured per-callback metrics published
to the TUI / external observers.
- Sources: [plan-2026-04-25-03 §Deferred](plans/plan-2026-04-25-03.md),
  [plan-2026-04-25-05 §Deferred](plans/plan-2026-04-25-05.md)
- Status: **open** — v0.4.

### Graceful audio-device-change handling
cpal surfaces device disconnects as `StreamError`; Plan 13 logs and
exits, v0.5 reconnects.
- Sources: [plan-2026-04-25-03 §Deferred](plans/plan-2026-04-25-03.md),
  [plan-2026-04-25-05 §Deferred](plans/plan-2026-04-25-05.md)
- Status: **open** — v0.5.

### WAV-file CLI input path
`agogo run --wav file.wav` for offline rendering / CI smoke tests.
Belongs with the cpal backend once it lands.
- Sources: [plan-2026-04-22-02 §Deferred](plans/plan-2026-04-22-02.md)
- Status: **open** — host-cpal exists; this is the offline analog.

---

## Platform-native backends

All deferred to post-v0.5 unless a precision need surfaces earlier.
Each lives in a new sibling crate on the `host-midi` / `host-link`
pattern (per Plan 13's expansion plan).

### MIDI sinks
- **CoreMIDI** — sub-µs scheduling via `MIDITimeStamp` (macOS)
- **JACK MIDI** — frame-indexed scheduling (Linux pro-audio)
- **ALSA MIDI** — Linux baseline
- **WinMM** — Windows baseline

Sources: [plan-2026-04-25-02 §Deferred](plans/plan-2026-04-25-02.md),
[plan-2026-04-25-03 §Deferred](plans/plan-2026-04-25-03.md),
[plan-2026-04-25-05 §Deferred](plans/plan-2026-04-25-05.md). Status: **open** — post-v0.5.

### Audio backends
- **JACK audio** — `host-jack` crate
- **CoreAudio direct** — `host-coreaudio` crate (cpal works on macOS,
  but direct CoreAudio gives sample-accurate device sync)
- **ASIO** — `host-asio` crate (Windows pro-audio)

Sources: [plan-2026-04-25-03 §Deferred](plans/plan-2026-04-25-03.md). Status: **open** — post-v0.5.

---

## v0.4 audio output

### `Channel::Audio` variant + `AudioRole::Click(AudioClickConfig)`
The natural sprint slot to introduce both. Until then, `Channel` stays
3-way (Midi / Din / Cv) and `dev=audio` errors with `AudioDeferred`.
- Sources: [plan-2026-04-26-02 §Deferred](plans/plan-2026-04-26-02.md),
  [plan-2026-04-25-05 §Deferred](plans/plan-2026-04-25-05.md)
- Status: **open** — v0.4.

### `host-cpal` output typing (Audit P6)
Same 1–2-line FFI containment treatment that v0.4's audio output side
will need for the cpal write-path types.
- Sources: [plan-2026-04-26-04 §Deferred](plans/plan-2026-04-26-04.md)
- Status: **open** — opportunistic, v0.4.

### `samples_to_micro_at_rate` helper (`link.rs:262`)
Rate-aware samples-to-microseconds conversion used by the audio output
planning. Currently inline `× 10⁶ / Hz`.
- Sources: [plan-2026-04-27-04 §Deferred](plans/plan-2026-04-27-04.md)
- Status: **open** — v0.4.

### Non-clock channel-mode renderers
- **DIN sync24** — v0.2
- **CV pulse + analog LFO** — v0.4 (via `out/audio.rs`)
- **MIDI CC** — post-v0.5 (wants a real `u7` newtype which agogo-core
  deliberately doesn't carry)

Sources: [plan-2026-04-23-01 §Deferred](plans/plan-2026-04-23-01.md),
[plan-2026-04-25-02 §Deferred](plans/plan-2026-04-25-02.md). Status: **open**.

### Negative shift (forward-look ring buffer)
Plan 03 clamps shift to `0..=+300 ms`; v0.2 adds the negative-shift
ring buffer. Tracked under `agogo.md §10` open question on shift
buffer budget.
- Sources: [plan-2026-04-23-01 §Deferred](plans/plan-2026-04-23-01.md)
- Status: **open** — v0.2.

---

## CLI surface & ergonomics

### Multi-port MIDI dispatch
Plan 14 supports one `MidirSink` shared across all `dev=midi` channels.
True per-channel routing wants a `MidiSinkRouter` keyed by port name.
- Sources: [plan-2026-04-25-05 §Deferred](plans/plan-2026-04-25-05.md)
- Status: **open** — v0.2.

### Preset I/O (`--config presets/foo.toml`)
The docker-style `--ch` syntax is the v0.1 surface; preset files are
the post-v0.5 sugar over it.
- Sources: [plan-2026-04-25-03 §Deferred](plans/plan-2026-04-25-03.md),
  [plan-2026-04-25-05 §Deferred](plans/plan-2026-04-25-05.md)
- Status: **open** — post-v0.5.

### Optional `dev=` key
Defaulting `dev=midi` and omitting from Display would shrink the
user-facing surface. CLI behaviour change; tabled until Audit P4.
- Sources: [plan-2026-04-26-03 §Deferred](plans/plan-2026-04-26-03.md)
- Status: **open** — depends on second routing-target parser.

### Sample-rate ladder extension (`S22`, `S32`, `S24`, `S352`, `S384`)
Not needed by v0.1; widen if a downstream rate surfaces.
- Sources: [plan-2026-04-23-03 §Deferred](plans/plan-2026-04-23-03.md)
- Status: **open** — speculative.

### `--help` output polish
`bpaf`'s default rendering is fine for v0.1; revisit when more
subcommands accrete.
- Sources: [plan-2026-04-23-02 §Deferred](plans/plan-2026-04-23-02.md)
- Status: **open** — speculative.

---

## Type discipline carryovers

### `BoundedTick` newtype
Encode the partial-`from_ticks` precondition at the type level,
eliminating the `expect()` from the conversion.
- Sources: [plan-2026-04-28-07 §Deferred](plans/plan-2026-04-28-07.md)
- Status: **open**.

### Audit P4 — sum-typed `ChannelSpec`
Mirror the `Channel` sum type at the spec layer (`enum { Midi, Din, Cv }`).
- Sources: [plan-2026-04-26-02 §Deferred](plans/plan-2026-04-26-02.md),
  [plan-2026-04-26-03 §Deferred](plans/plan-2026-04-26-03.md)
- Status: **open** — natural moment is when `dev=din` or `dev=cv`
  lands as a parser key.

### `F64TMP` / `F64PHS` / `F64Q15` agogo-local Conns
Originally specified as `Conn<FloatExt<f64>, Extended<T>>` for Tempo /
Phase / Q15 targets. Need a u32-backed variant of upstream's
`float_conn!` macro (Tempo / Phase / Q15 are u32 / u32 / u16, not i64).
- Sources: [plan-2026-04-24-01 §Deferred](plans/plan-2026-04-24-01.md),
  [plan-2026-04-24-02 §Deferred](plans/plan-2026-04-24-02.md)
- Status: **likely stale** — `Phase` is a wrapping quotient (not a
  monotone Conn), and `f64_bpm_to_tempo` / `f64_phase_to_phase` are
  the named argv-boundary exceptions per CLAUDE.md. Revisit only if
  bidirectional Galois laws are ever needed downstream.

### `MidiSink::send_at(&[u8])` typed-message migration (Audit E)
Renderer signatures narrowed in Plan 21, but the sink trait stayed
byte-oriented. Plan 23 explicitly tagged this "low marginal value".
- Sources: [plan-2026-04-26-02 §Deferred](plans/plan-2026-04-26-02.md),
  [plan-2026-04-26-04 §Deferred](plans/plan-2026-04-26-04.md)
- Status: **open** — low priority.

### `compose!` / `ceiling1` body cleanups
The rev bump unlocked `compose!` and `ceiling1` upstream. A pass
through agogo to use them where the ad-hoc inline arithmetic still
sits.
- Sources: [plan-2026-04-28-04 §Deferred](plans/plan-2026-04-28-04.md),
  [plan-2026-04-28-05 §Deferred](plans/plan-2026-04-28-05.md),
  [plan-2026-04-28-06 §Deferred](plans/plan-2026-04-28-06.md)
- Status: **open** — ready, no upstream block.

### host-link 4-layer wrapping cleanup
`LinkClock` → `LinkSession` → `LinkPhaseSource` plus `transport.rs`
is functional but accidentally accreted. Untangling deserves its own
plan.
- Sources: [plan-2026-04-28-03 §Deferred](plans/plan-2026-04-28-03.md),
  [plan-2026-04-28-04 §Deferred](plans/plan-2026-04-28-04.md),
  [plan-2026-04-28-05 §Deferred](plans/plan-2026-04-28-05.md),
  [plan-2026-04-28-06 §Deferred](plans/plan-2026-04-28-06.md)
- Status: **open**.

---

## Test infrastructure

### Re-enable `swing_is_bar_periodic`
`#[ignore]`'d on main with documented re-enablement plan; seed
`b9e83f4f` reproduces a real `effective_tick` bug for T1+negative-amount.
- Sources: [plan-2026-04-25-01](plans/plan-2026-04-25-01.md) review
  trail (also memory `project_swing_bar_periodic_flaky.md`)
- Status: **open** — confirmed at `crates/core/src/time/swing.rs:467`.

### `detect` multi-rate proptest re-enable
Tracked in plan-2026-04-23-03 §Review; a rate-dispatch macro will
restore coverage across the SR ladder.
- Sources: [plan-2026-04-24-01 §Deferred](plans/plan-2026-04-24-01.md)
- Status: **open**.

### `link::probe::tests` LAN-peer brittleness
Pre-existing fragility deferred from PR #37; the test depends on LAN
peer presence and is brittle in CI / sandboxed runs.
- Sources: [plan-2026-04-28-06 §Deferred](plans/plan-2026-04-28-06.md)
- Status: **open**.

### `trybuild` compile-fail tests for Channel narrowing
Confirm `render_midi_channel(&cv_channel)` doesn't compile. The type
system already enforces it; trybuild adds a regression net.
- Sources: [plan-2026-04-26-02 §Deferred](plans/plan-2026-04-26-02.md)
- Status: **open** — non-blocking.

### `agogo-testkit` crate
A re-export hub for per-type `arb` submodules, when external consumers
appear.
- Sources: [plan-2026-04-28-08 §Deferred](plans/plan-2026-04-28-08.md)
- Status: **open** — speculative, no consumer yet.

### Per-type `prop.rs` files for law predicates
For when agogo authors its own algebras (DSL grammar laws, machine-spec
round-trip) rather than consuming upstream `connections::prop::conn`.
- Sources: [plan-2026-04-28-08 §Deferred](plans/plan-2026-04-28-08.md)
- Status: **open** — speculative.

### Link phase bridge tolerance doc fix
Doc-only: review-00008.md flagged a 2²² ULP vs plan's 1 ULP discrepancy
in the prose. Fold into the next plan that touches the file.
- Sources: [plan-2026-04-24-01 §Deferred](plans/plan-2026-04-24-01.md)
- Status: **open** — trivial doc edit.

---

## Tooling

### `check-conns.sh` enforcement script
A grep-based gate flagging open-coded unit shifts (`* 1.0e-3`,
`* 1_000_000.0`) outside the named-Conn allowlist. Suggested by
plan-2026-04-28-02 once Conn-naming stabilizes.
- Sources: [plan-2026-04-28-02 §Deferred](plans/plan-2026-04-28-02.md)
- Status: **open**.

---

## Latency & timing (v0.5 Sprint 03)

### Per-format latency compensation table
Platform-specific µs-scale tuning so each backend's emit time aligns
with the master `at_sample` timebase. midir's ~1 ms ceiling stays;
CoreMIDI / JACK can do sub-µs.
- Sources: [plan-2026-04-25-02 §Deferred](plans/plan-2026-04-25-02.md),
  [plan-2026-04-25-03 §Deferred](plans/plan-2026-04-25-03.md),
  [plan-2026-04-25-05 §Deferred](plans/plan-2026-04-25-05.md)
- Status: **open** — v0.5 Sprint 03.

### Song Position Pointer (`0xF2`)
Transport navigation mid-song. Revisit with the forerun FSM.
- Sources: [plan-2026-04-25-02 §Deferred](plans/plan-2026-04-25-02.md)
- Status: **open** — v0.5.

---

## Polyrhythm UX

### Polyrhythm alignment display
TBase lattice join (LCM) — surfacing "channels realign at T8" via
the CLI / TUI. Infrastructure is in `time/`; UX is post-v0.1.
- Sources: [plan-2026-04-23-01 §Deferred](plans/plan-2026-04-23-01.md)
- Status: **open**.

---

## Out of scope (intentional, not "deferred")

- **SysEx** — explicitly out per `agogo.md §11`.
- **`bits_q48_16_to_seconds` open-coded body** — already named
  PI-exempt per CLAUDE.md exception 1; the unit shift is rate-dependent
  (per-Hz), not an SI ladder rung. Leaves as-is.
- **Inline `err_bits / 65_536.0`** at `detect.rs:269, :301` — annotated
  binary-fixed scale, lifted to a helper only if more sites appear.
- **`crates/core/src/boundary.rs` further splitting** — splitting by
  argv / PI / rate categories would multiply the
  `scripts/check-floats.sh` allowlist for no navigation gain.
- **`out/midi.rs`, `time/grid.rs`, `time/sample.rs` further splitting**
  — large but cohesive; reconsider when one passes 1000 lines or
  develops a second concern.

---

## Resolved since first deferral

These items appeared in older plans' `## Deferred` sections but the
working tree confirms they shipped. Listed for archaeological purposes
when a future plan grep brings up a stale promise.

- **`Conn::then` upstream blocker** → abandoned in favor of
  `connections::compose!` macro, which agogo now uses directly
  (memory: `project_conn_then_upstream_blocking.md`).
- **`Ple` trait migration** — vendored locally then removed entirely
  (Plan 24 + Plan 28-07 T2; only historical comments remain).
- **`ExtendedFloat::Finite` → `Extend` rename** — Plan 24 swept all
  six call sites.
- **Channel sum-type with `Midi { common, role }` / `Din` / `Cv`
  variants** — Plan 21 (audit P3); see `crates/core/src/channel/transform.rs:41`.
- **`ChannelSpec.dev` field deletion** — Plan 22 (audit P4 first half).
- **`ChannelSpec.delay_ms: f64 → FD06` and `RunArgs.bpm: f64 → Tempo`** —
  Plan 27 (Q3 / audit K + L); `delay_ms` now crosses via the `F064FD06`
  Conn per `crates/core/src/machine/spec.rs:27`.
- **`fxp.rs` deletion / `boundary.rs` migration** — Plan 28-03 T5.
- **`Quantum` + `f64_beats_to_quantum` move to `host-link`** — Plan
  28-03 T4; lives at `crates/host-link/src/quantum.rs`.
- **`machine/spec.rs` 1299-line split** — Plan 28-06; spec/{display,
  parser, types, validate}.rs.
- **`cli/main.rs` 1727-line extraction** — Plan 28-05; now grouped
  under `cli/src/trace/`, `cli/src/time/`, and `cli/src/link/`
  by Plan 2026-04-30-03.
- **`cargo build -p agogo-cli --no-default-features` build break** —
  Plan 2026-04-30-03 fixed the core-dependent CLI module gates and
  feature dependencies; the no-default-features compile check passes.
- **Per-type `arb.rs` colocation** — Plan 28-08; every type module
  under `crates/core/src/time/` now owns its `<module>/arb.rs`.
- **`SampleTickConn` shim** — wired in Plan 03 (then renamed and
  ultimately deleted in Plan 28-04 once `compose!` removed the need).
- **Transport FSM minimal `{Stopped, Playing}` shim** — Plan 09;
  `crates/host-link/src/transport.rs:33-46`. NEG/POS extension is the
  open follow-on listed above.
- **`snap_intent` accessor on `ChannelSpec`** — Plan 20 shipped the
  accessor; Plan 28-09 T1 wired it into the orchestrator via
  `agogo_host_link::apply_snap_offsets` (the walk lives on the
  host-link side, not in cli, to avoid widening cli/Link coupling).
- **`Quantum::from_bars(N)` typed fallbacks** — Plan 28-02 confirmed
  `bpaf` accepts the `pub const fn` directly; the `fallback_with`
  workaround was never needed.
- **bpaf parser swap (clap → bpaf)** — Plan 04.
- **`agogo-cli` binary alias** — Plan 28-09 T4; `crates/cli/Cargo.toml`
  now declares one `[[bin]]` entry. External scripts have migrated.
- **`#[bpaf(version)]` flag** — Plan 28-09 T5; `agogo --version`
  prints the workspace version.
- **`channel.rs` re-export hub audit (`CvRole` / `DinRole`)** —
  Plan 28-09 T6; both re-exports marked `#[doc(hidden)]` (forward-
  compat scaffolding for v0.4 / v0.2 backends with no v0.1 renderer).
- **Tempo → f64 sweep at `link.rs:68, :160`** — confirmed already
  done in Plan 28-09's exploration: both call sites now use
  `tempo_to_f64_bpm()`. Listed here for the archaeological grep.
- **Collapse PLL Tempo `abs_diff` sites** — confirmed already done
  via `Tempo::abs_diff()` (`crates/core/src/time/tempo.rs:33`); the
  five PLL call sites + `sync/source.rs:226` use it. Plan 28-09
  exploration finding N3 closure.
