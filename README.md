[![CI](https://github.com/cmk/agogo/actions/workflows/ci.yml/badge.svg)](https://github.com/cmk/agogo/actions/workflows/ci.yml)
[![Docs](https://github.com/cmk/agogo/actions/workflows/docs.yml/badge.svg)](https://cmk.github.io/agogo/)

# agogo

agogo is a Rust studio timing and sync engine for building precise musical
clock, click, and control pipelines. It is inspired by multi-format clock
hardware: one timeline should be able to drive MIDI clock, MIDI notes, audio
clicks, CV pulses, Link transport, and future native timestamped backends
without moving scheduling decisions onto a soft UI thread.

The project is early. The current repo is useful as a timing-kernel and
runtime prototype, not a finished device controller. Source builds and tests
are supported; crates.io publishing is intentionally deferred.

API docs are published at <https://cmk.github.io/agogo/>.

## Current Status

Implemented today:

- fixed-point musical time, grid, swing, tempo, and sample-rate conversions;
- per-channel scheduling from `ChannelCommon` / channel specs;
- internal sample-clock driven playback through `Playhead::on_buffer`;
- MIDI clock and MIDI click rendering through a best-effort `midir` sink;
- stereo ch1-2 audio-click rendering through `cpal`;
- synthetic PLL/sync tracing and scheduler/MIDI trace CLI commands;
- Ableton Link session utilities and a Link-backed `agogo run --source link`
  mode behind feature flags;
- a hard-time bridge skeleton for bounded command admission and snapshots.

Known gaps:

- native timestamped MIDI backends are not implemented yet, so the current
  `midir` output path is best effort at the host boundary;
- CV pulse rendering has a dual-mono stereo-output MVP; CV LFO roles are still spec
  stubs;
- generated audio metronome output is limited to one or two click channels,
  panned left then right;
- multi-device output routing is deliberately constrained to one MIDI target
  and one audio target per run;
- there is no crates.io release contract yet.

The roadmap lives in [`doc/versions/`](doc/versions/).

## Build From Source

The repo uses the pinned Rust toolchain in
[`rust-toolchain.toml`](rust-toolchain.toml). From a fresh checkout:

```bash
cargo test --workspace
cargo clippy --all-targets -- -D warnings
```

The default CLI build includes pure core utilities:

```bash
cargo run -p agogo-cli --bin agogo -- --help
```

Runtime commands that talk to host audio, MIDI, or Link are feature-gated so
default CI and source builds stay lean:

```bash
cargo run -p agogo-cli --features run --bin agogo -- run --help
```

## Hardware-Free Commands

These commands run without audio, MIDI, or Link hardware.

Trace one bar of a straight quarter-note scheduler at 120 BPM:

```bash
cargo run -p agogo-cli --bin agogo -- channel trace \
  --bpm 120 --sr 48000 --grid t4 --buffers 96 --frames 256
```

Render a pure MIDI-clock byte trace into the in-memory test sink:

```bash
cargo run -p agogo-cli --bin agogo -- midi trace \
  --bpm 120 --sr 48000 --grid t32t --buffers 96 --frames 256
```

Drive the runtime `Playhead::on_buffer` path offline and print deterministic
JSON:

```bash
cargo run -p agogo-cli --bin agogo -- render \
  --source internal \
  --bpm 120 \
  --sr 48000 \
  --duration-bars 1 \
  --ch 'id=three,dev=midi,mode=clock,grid=t2t,out=diag' \
  --ch 'id=two,dev=midi,mode=clock,grid=t2,out=diag'
```

Render a hardware-free CV pulse diagnostic:

```bash
cargo run -p agogo-cli --bin agogo -- render \
  --source internal \
  --bpm 120 \
  --sr 48000 \
  --duration-bars 1 \
  --ch 'id=cv,dev=cv,mode=pulse,grid=t4,out=diag'
```

Run the synthetic audio-clock PLL trace:

```bash
cargo run -p agogo-cli --bin agogo -- sync trace \
  --bpm 120 --sr 48000 --ppq 4 --pulses 16
```

## Hardware-Backed Runs

`agogo run` opens host devices and runs until Ctrl-C. It currently supports
`--source internal`, `--source external`, and `--source link` with the `run`
feature enabled.

Internal 3:2 audio-click example. Each `dev=audio` channel declares its
output channel via `out=N` (the audio output lane index, 0-based); two
audio-click channels here write to lanes 0 and 1 of the host's default
stereo output.

```bash
cargo run -p agogo-cli --features run --bin agogo -- run \
  --source internal \
  --bpm 120 \
  --sr 48000 \
  --ch 'id=three,dev=audio,mode=click,grid=t2t,out=0' \
  --ch 'id=two,dev=audio,mode=click,grid=t2,out=1'
```

Generated audio output requires a stereo f32 device config. The live `run`
path opens a 2-channel stream, so audio specs must use `out=0` or `out=1`;
multi-channel host streams are tracked as a follow-up.

Internal MIDI clock example using the first MIDI output:

```bash
cargo run -p agogo-cli --features run --bin agogo -- run \
  --source internal \
  --bpm 120 \
  --sr 48000 \
  --ch 'id=clock,dev=midi,mode=clock,grid=t32t,out=default'
```

Internal CV pulse smoke example using the default audio output:

```bash
cargo run -p agogo-cli --features run --bin agogo -- run \
  --source internal \
  --bpm 120 \
  --sr 48000 \
  --ch 'id=cv,dev=cv,mode=pulse,grid=t4,out=default'
```

CV pulse output is a raw sync signal. Route it only to an interface output
intended for clock/CV, not to speakers or a processed monitor bus.

List visible MIDI outputs:

```bash
cargo run -p agogo-cli --features demo --bin agogo -- demo list-midi-outputs
```

When the binary is already installed locally, replace the `cargo run ... --`
prefix with `agogo`.

## Timing Model

Inside the audio callback, agogo schedules against sample indices and keeps the
hard-time path free of locks, allocation, async, and filesystem access. That is
the internal timing contract.

The current external MIDI backend uses `midir`, which does not give agogo a
portable native timestamped send contract. MIDI sent through this path should be
treated as best effort at the host boundary. Version 0.2 and later roadmap docs
separate this from future native timestamped backends such as CoreMIDI, JACK,
or platform-specific equivalents.

## Why Galois Connections?

Agogo uses the [`connections`](https://github.com/cmk/connections) crate for
numeric conversions where the rounding direction is part of the correctness
contract: microseconds to picoseconds, picoseconds to sample time, sample time
to whole sample counts, BPM to fixed-point tempo, and similar boundaries.

The practical idea is simple: a named connection carries the conversion and its
rounding policy together.

```rust
let pico = FD12FD06.inner(micro); // exact Micro -> Pico refinement
let samples = FD12R048.ceil(pico); // round Pico up to R048 sample time
```

For a connection `Fine => Coarse`:

- `inner` embeds a coarse value into the finer representation;
- `ceil` returns the smallest coarse value that covers a fine value;
- `floor` returns the largest coarse value that does not exceed a fine value.

That makes review questions concrete. Instead of asking whether an arithmetic
formula happens to round correctly, the code names the source type, target type,
and adjoint being used. Property tests then check the monotonicity, closure,
and adjoint laws over the relevant domains.

Not every transform in agogo is a connection. When a conversion is
runtime-parameterized, intentionally one-sided, or has domain-specific boundary
semantics, agogo uses a normal named helper and tests that helper directly. The
goal is explicit rounding at timing boundaries, not forcing every function into
the abstraction.

## Workspace Layout

```text
crates/
  agogo/       public facade crate used by the CLI
  chan/        pure channel, timing, connection, control, and sink logic
  core/        runtime orchestration: driver, bridge, transport, snapshots
  cli/         `agogo` binary and command parsing
  host-cpal/   detached cpal host adapter
  host-midi/   detached midir host adapter
  host-link/   detached Ableton Link host adapter
```

The detached host crates are pulled in by CLI feature flags instead of being
default workspace members. That keeps `cargo test --workspace` usable on
machines without every audio/MIDI/Link system dependency.

## Feature Flags

`agogo-cli` defaults to `core`.

- `core`: pure CLI utilities and the `agogo` facade.
- `cpal`: audio host adapter.
- `midi`: MIDI host adapter.
- `demo`: single-channel demo command; enables `core`, `cpal`, and `midi`.
- `link`: Ableton Link utilities.
- `run`: end-to-end runtime command; enables `demo`, `link`, and Ctrl-C
  handling.

## Publishing Status

This repository is being prepared for public GitHub use independently from
crates.io. Every crate currently keeps `publish = false`; that is intentional.

Supported now:

- cloning the GitHub repo;
- building and testing from source;
- reading generated API docs.

Deferred to a later publishing plan:

- package descriptions, keywords, categories, and registry metadata;
- `cargo package --list` review;
- `cargo publish --dry-run`;
- removing or replacing any dependency constraints that are acceptable for
  source builds but unsuitable for registry publishing.

## Contributing

Agent and maintainer workflow details live in [`AGENTS.md`](AGENTS.md).
That file is intentionally stricter than this README: it documents the branch
state machine, review process, layering checks, float rules, and property-test
expectations used while developing agogo.
