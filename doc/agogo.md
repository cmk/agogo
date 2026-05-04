# agogo — design brief

## 1. Context & scope

A Rust port/re-imagining of the E-RM Multiclock hardware (4-channel multi-format sync box). Takes a master clock input — audio-sync pulse train, MIDI clock, DIN/sync24, or an internal BPM generator — runs it through a PLL, and produces N configurable output streams: MIDI clock, DIN sync24, analog CV pulse/gate, analog LFO, or MIDI CC controller. Per-channel transforms: divider, shuffle, shift (±300 ms against master), offset calibration.

**Target**: software-only standalone binary, macOS-first but cross-platform from day one (Linux, Windows). "DAW-level precision" — defined concretely in §4.

**Library home**: new sibling crate. Companion to the in-development
`connections` library (Galois-connection primitives, N5 preorder).

**Not in v1**: VST/AU plugin wrapper, hardware firmware replacement, the device UI (LEDs/encoder/menus), pitch-bend/CC remote control, DIN physical wiring (library emits the bit pattern; user provides the interface). See §11.

## 2. Design Heuristics

## Clock Source And Follower Design

The clock subsystem should be explicit and central.

Core types should include:

- `ClockDomainId`: engine sample clock, audio device clock, Link clock, MTC, LTC, MIDI clock, DAW timeline, wall clock, and per-device clocks.
- `ClockObservation`: source-domain observation captured with host sample time and protocol metadata.
- `ClockEstimate`: phase, frequency ratio, jitter, confidence, and lock state.
- `TimelineMap`: conversion between samples, seconds, bars/beats, and ticks.
- `TransportSnapshot`: sample position, beat position, tempo, meter, loop, rolling state, and revision.

For following, the engine should support:

- lock acquisition and loss
- drift smoothing
- discontinuity detection
- holdover when external clock packets disappear
- manual source priority and automatic failover
- bounded slew limits so corrections do not produce audible jumps

For sourcing, the engine should support:

- sample-accurate internal tempo
- MIDI clock phase generation from sample position
- MTC/LTC generation from sample position and frame-rate policy
- Link beat-time publication from the transport map
- deterministic event ordering when many outputs share the same sample

Create a separate realtime contract for the engine boundary:

- `SampleTime`, `FrameCount`, `SampleRate`, `TempoMapRevision`
- `ClockDomainId`, `ClockObservation`, `ClockEstimate`
- `TransportState`, `LocateTarget`, `LoopRegion`
- `ScheduledEvent`, `EventDeadline`, `EventPriority`
- `DevicePortId`, `ProtocolEndpointId`, `HardwareLatency`
- bounded error and dropout reports

### Tick-master model

Time lives in layers, and only the bottom depends on runtime tempo/sample-rate:

```
Tick  (u32, 192 PPQN)           ← master time, tempo-independent
  ↕   Conn via quantize_at(tb)  ← grid quantization
Time  (beats × TBase)           ← musical duration
                                ← tempo+SR boundary
Samples (u64)                   ← output-rate rendering
```

Scheduling happens in Tick space. `Tick → Samples` conversion is parameterized by `(sample_rate, bpm)` and applied only at the output boundary. Consequence: a tempo change mid-buffer cannot retroactively shift already-scheduled events, because events are Tick-valued until the last moment.

The audio callback is the single real-time thread. It receives `AudioIo { input, output, buffer_start_sample, sample_rate, frames }`, advances the `PhaseSource`, schedules ticks per channel, and either writes samples (for CV/LFO outputs) or hands MIDI events with sample-indexed timestamps to a platform-native sink.

### Realtime Contract

- No network calls, model calls, JSON parsing, filesystem I/O, logging locks, heap allocation, mutex waits, subprocesses, or unbounded channels in the hard-time path.
- Cross from soft time into hard time only through preallocated bounded queues, lock-free rings, atomics, double-buffered snapshots, or equivalent bounded primitives.
- Express every scheduled action in an explicit time domain: sample frame, musical beat, bar/beat/tick, MIDI tick, wall clock, Link beat time, LTC/MTC timecode, or remote device clock.
- Treat sample accuracy as an internal scheduling property, not a universal promise to external hardware. DIN MIDI, USB MIDI, network OSC, and some rack hardware add transport and device jitter. The system should model that uncertainty instead of hiding it.

### Observability

Realtime observability should be designed around counters and bounded events:

- xrun count and last xrun sample
- callback duration histogram
- max scheduler occupancy
- queue fill levels and drop counts
- clock lock state, phase error, drift estimate, and jitter estimate
- per-protocol send lateness and device latency estimates
- external clock discontinuities and holdover intervals

Do not call general tracing or analytics APIs from the callback. Instead, writ compact records into preallocated rings and let a soft observer convert them t logs, telemetry, UI notifications, or reports.

## 3. Module layout

```
core/src
├── lib.rs
├── conn.rs
├── conn/
│   ├── arb.rs
│   ├── boundary.rs
│   ├── fixed.rs
│   ├── float.rs
│   ├── midi.rs
│   ├── phase.rs
│   ├── sample.rs
│   └── tempo.rs
├── time.rs
├── time/
│   ├── arb.rs
│   ├── conn.rs
│   ├── envelope.rs
│   ├── grid.rs
│   ├── swing.rs
│   ├── tbase.rs
│   └── tick.rs
├── channel.rs
├── channel/
│   ├── role.rs
│   ├── time.rs
│   ├── dsl.rs
│   ├── dsl/
│   ├── spec.rs
│   └── spec/
├── control.rs
├── control/
│   ├── event.rs
│   ├── sync.rs
│   └── sync/
│       ├── detect.rs
│       ├── pll.rs
│       ├── pulse.rs
│       └── source.rs
├── sink.rs
├── sink/
│   ├── audio.rs
│   └── midi.rs
└── test.rs
```

## 4. Precision budget

| Path | Precision | Platform |
|---|---|---|
| CV / audio-rate output | 1 sample — 5.2 µs @ 192 kHz, 20.8 µs @ 48 kHz | All (via cpal output buffer) |
| MIDI scheduled, sub-µs | Sub-µs | CoreMIDI (mach time), JACK (frame index) |
| MIDI scheduled, ms-level | 2–5 ms dispatch jitter | WinMM |
| MIDI on USB bus | ~1 ms | Hardware ceiling, platform-independent |

Sample-accurate CV/audio output matches or beats the original hardware's ±20 µs spec. MIDI clock out is always USB-bus-limited to ~1 ms regardless of software. The precise case is CV out through a DC-coupled audio interface.

**Target config for max precision**: JACK on Linux or CoreAudio+CoreMIDI on macOS, 192 kHz, 64-sample buffer.

## 5. Platform abstraction

Two traits parameterized on "samples since stream start" — no platform `HostTime` type leaks into the core:

```rust
trait AudioHost {
    fn run(self, cfg: Config,
           cb: Box<dyn FnMut(&mut AudioIo) + Send>) -> Handle;
}

struct AudioIo<'a> {
    input: &'a [f32],
    output: &'a mut [f32],
    buffer_start_sample: u64,
    sample_rate: u32,
    frames: usize,
}

trait MidiSink: Send {
    fn send_at(&self, msg: &[u8], at_sample: u64);
}
```

Each backend converts `at_sample` → native timebase inside `send_at` (mach time for CoreMIDI, `QueryPerformanceCounter` for WinMM, frame index for JACK). The core logic only sees monotonic sample counts.

Feature-gated backends: `cpal-audio`, `coremidi`, `alsa-midi`, `jack`, `winmm`. cpal is the portable audio baseline; platform-native MIDI sinks are where precision differs.

## 6. Mapping to music-time 

| agogo feature | time module |
|---|---|
| Channel Divider (1/2/3/…/96 + triplets) | **TBase choice** (T4t/T8t/T16t/T32t/T64t correspond to 1/3/6/12/24) |
| Shuffle | **SwingConfig + effective_tick** |
| LFO Saw Up / Saw Down / S-curve | **opening / closing / s_curve** envelopes |
| Polyrhythm alignment display | **TBase lattice join (LCM)** — "channels realign at T8" |
| Preset compatibility check | **TBase lattice meet (GCD)** |

Two ops stay **outside** the grid lattice, at the Sample layer: **Shift (±300 ms)** and **Offset calibration**. Both are continuous affine translations applied after `Tick → Samples` — not expressible as grid operations.

## 7. On `Conn` and the fn-pointer constraint

The `Conn<A, B>` type in `connections/src/conn.rs` uses bare `fn` pointers, which cannot close over runtime state. The natural shape `fn conn_sample_tick(bpm) -> Conn<Sample, Tick>` is therefore not expressible today — a `Conn` value cannot depend on runtime tempo.

Pragmatic resolution: keep the static pieces static. `PPQN` is fixed at the `Tick` type, each supported sample rate has a rate-typed sample connection, and the musical scheduler computes `Tick + Tempo -> Sxxx` directly before using the static `SxxxI064` whole-sample connection. Decimal `FD06` / `FD12` conversions stay at SI-duration boundaries such as delay, offset, jitter, and FFI seams.

## 8. Libraries

- **`cpal`** — cross-platform audio baseline. Sample-accurate output buffers.
- **`midir`** — best-effort MIDI transport for the v0.1 baseline; v0.2+
  work distinguishes it from timestamped platform sinks behind `MidiSink`.
- **`rtrb`** — lock-free SPSC (control → RT).
- **`serde` + `ciborium`** — preset persistence.
- **`proptest`** — per connections-repo convention.
- **`connections`** — Galois connections for principled numerical casting

## 9. Open questions

- **Tempo glide**: should internal-master tempo changes apply instantly (hardware-faithful, occasional hiccup on big jumps) or through a one-pole glide filter (musically nicer)?
- **Shift buffer budget**: negative shift requires a ring buffer of future ticks. What's the maximum forward-look we budget — 300 ms to match the hardware, or more?
- **rtp-MIDI / network-MIDI**: worth a backend, or strictly local I/O?
- **Ableton Link**: v0.3 owns full follower/source behavior on top of the
  existing host-link scaffolding.
- **Preset SR-agnosticism**: a preset saved at 48 k — the Tick-master design should make it sample-rate-agnostic. Worth testing explicitly as a proptest invariant.
- **Transport FSM**: v0.3 owns the NEG/POS one-bar forerun semantics and
  source-switch phase behavior.
- **Output channel count**: hardware is 4. Should the software version be `N` generic, or fix the count?

---

*Companion conversation: this file captures the design synthesis of a multi-turn discussion from 2026-04-22. The agogo crate's own `doc/design.md` — once scaffolded — will be the polished spec; this is the seed.*
