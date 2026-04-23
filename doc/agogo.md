# agogo — design brief

## 1. Context & scope

A Rust port/re-imagining of the E-RM Multiclock hardware (4-channel multi-format sync box). Takes a master clock input — audio-sync pulse train, MIDI clock, DIN/sync24, or an internal BPM generator — runs it through a PLL, and produces N configurable output streams: MIDI clock, DIN sync24, analog CV pulse/gate, analog LFO, or MIDI CC controller. Per-channel transforms: divider, shuffle, shift (±300 ms against master), offset calibration.

**Target**: software-only standalone binary, macOS-first but cross-platform from day one (Linux, Windows). "DAW-level precision" — defined concretely in §4.

**Library home**: new sibling crate at `~/Music/Software/Anarkhiya/agogo/`. Companion to the in-development `connections` library (Galois-connection primitives, N5 preorder).

**Not in v1**: VST/AU plugin wrapper, hardware firmware replacement, the device UI (LEDs/encoder/menus), pitch-bend/CC remote control, DIN physical wiring (library emits the bit pattern; user provides the interface). See §11.

## 2. Architecture: the Tick-master model

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

## 3. Module layout

```
agogo/
├── Cargo.toml       # features: cpal-audio, coremidi, alsa-midi, winmm, jack
├── src/
│   ├── lib.rs
│   ├── time/        # port of Cirklon Time.hs (as a submodule, not a crate)
│   │   ├── tbase.rs     # enum TBase + divisibility lattice (LCM/GCD)
│   │   ├── tick.rs      # Tick(u32), Time { beats, base }
│   │   ├── swing.rs     # SwingConfig, effective_tick
│   │   ├── envelope.rs  # opening, closing, s_curve (LFO waveforms)
│   │   └── conn.rs      # quantize_at, ticks, rat_tick
│   ├── sync/
│   │   ├── source.rs    # enum PhaseSource { Internal, External(Pll) }
│   │   ├── detect.rs    # peak detector + sub-sample interp
│   │   └── pll.rs       # Type-II loop filter
│   ├── channel/
│   │   ├── mod.rs       # Channel { mode, divider, shuffle, shift, offset }
│   │   ├── mode.rs      # enum ChannelMode
│   │   ├── transform.rs # divider ∘ shuffle ∘ shift ∘ offset pipeline
│   │   └── lfo.rs       # renders time::envelope at sample rate
│   ├── out/
│   │   ├── midi.rs      # MidiSink trait + timestamped send
│   │   └── audio.rs     # render pulses/gates/LFO into output buffer
│   ├── machine.rs       # [Channel; N], preset I/O
│   ├── host/
│   │   ├── traits.rs    # AudioHost, MidiSink
│   │   ├── cpal.rs, coremidi.rs, alsa_midi.rs, jack.rs, winmm.rs
│   └── rt/
│       ├── callback.rs  # audio-thread hot loop (no alloc, no locks)
│       └── control.rs   # control-thread → SPSC → RT
└── bin/
    └── agogo.rs
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

## 6. Mapping to music-time (the Cirklon port)

`src/time/` is a Rust port of `Control.Cirklon.Type.Time` from `Software/Haskell/recologic/client/src/Control/Cirklon/Type/Time.hs`. Most agogo features land directly on its primitives:

| agogo feature | time module |
|---|---|
| Channel Divider (1/2/3/…/96 + triplets) | **TBase choice** (T4t/T8t/T16t/T32t/T64t correspond to 1/3/6/12/24) |
| Shuffle | **SwingConfig + effective_tick** |
| LFO Saw Up / Saw Down / S-curve | **opening / closing / s_curve** envelopes |
| Polyrhythm alignment display | **TBase lattice join (LCM)** — "channels realign at T8" |
| Preset compatibility check | **TBase lattice meet (GCD)** |

Two ops stay **outside** the grid lattice, at the Sample layer: **Shift (±300 ms)** and **Offset calibration**. Both are continuous affine translations applied after `Tick → Samples` — not expressible as grid operations.

## 7. On `Conn` and the fn-pointer constraint

The `Conn<A, B>` type in `connections/src/conn.rs` uses bare `fn` pointers, which cannot close over runtime state. The natural shape `fn conn_sample_tick(sr, bpm) -> Conn<Sample, Tick>` is therefore not expressible today — a `Conn` value cannot depend on runtime `(sr, bpm)`.

Pragmatic resolution: introduce a parallel `SampleTickConn { sr, bpm, ppqn }` struct with `floor/ceil/inner` methods mirroring `Conn`'s shape and laws. Tick↔Time connections (tempo-independent) use genuine `Conn` from the connections crate. If connections later ships a closure-capturing variant (`ConnBox` or similar), agogo migrates — the laws and test fixtures port unchanged.

## 8. Libraries

### 8a. Depend
- **`cpal`** — cross-platform audio baseline. Sample-accurate output buffers.
- **`midir`** — MIDI transport for v0; replace with platform sinks behind `MidiSink` for precision work.
- **`rtrb`** — lock-free SPSC (control → RT).
- **`serde` + `ciborium`** — preset persistence.
- **`proptest`** — per connections-repo convention.
- **`connections`** — path dep on `../connections/connections` for `Conn` + `Ple`.

### 8b. Emulate
- **`hertz`** (`ext/hertz`) — optional, for BPM/sample-rate conversion helpers.
- **`clocked`** (`ext/clocked`) — borrow `PidSettings` patterns for the PLL loop filter.
- **`embedded-time`** (`ext/embedded-time`)

## 9. Proposed first sprint

Plan 01 inside the agogo crate delivers the **pure-logic core**, no audio I/O:

- `time/` — full port of Cirklon Time.hs: TBase lattice, Time/Tick, `quantize_at`, SwingConfig, envelopes. Proptests for lattice laws (LCM/GCD absorption, Heyting) and Galois-connection adjointness.
- `channel/transform.rs` — pure divider/shuffle/shift/offset composition. Proptests: tick monotonicity, divider rate preservation, shuffle zero-mean over a beat, shift clamping.
- `sync/pll.rs` + `sync/detect.rs` — tested against synthetic pulse trains with injected Gaussian timing noise. Proptest: PLL BPM estimate converges within a known error bound.
- CLI binary: reads a WAV of audio-sync, runs the PLL offline, prints BPM+phase trace. No CoreAudio / MIDI yet.

Cargo.toml includes feature gates for future backends; none enabled. Zero platform code touched.

## 10. Open questions

- **Tempo glide**: should internal-master tempo changes apply instantly (hardware-faithful, occasional hiccup on big jumps) or through a one-pole glide filter (musically nicer)?
- **Shift buffer budget**: negative shift requires a ring buffer of future ticks. What's the maximum forward-look we budget — 300 ms to match the hardware, or more?
- **rtp-MIDI / network-MIDI**: worth a backend, or strictly local I/O?
- **Ableton Link**: wrap the C++ library as a `PhaseSource` variant, or defer entirely?
- **Preset SR-agnosticism**: a preset saved at 48 k — the Tick-master design should make it sample-rate-agnostic. Worth testing explicitly as a proptest invariant.
- **Transport FSM**: the NEG/POS "one-bar forerun" semantics from the manual need a concrete FSM spec before Sprint 2.
- **Output channel count**: hardware is 4. Should the software version be `N` generic, or fix the count?

## 11. Out of scope (v1)

- VST / AU plugin wrapper
- Hardware firmware replacement
- The device UI (status LEDs, encoder, menus) — this is a library + CLI first
- Remote control via pitch-bend / CC (deferred to a later sprint)
- DIN sync24 physical wiring — the library emits the bit pattern; the user provides the hardware interface

---

*Companion conversation: this file captures the design synthesis of a multi-turn discussion from 2026-04-22. The agogo crate's own `doc/design.md` — once scaffolded — will be the polished spec; this is the seed.*
