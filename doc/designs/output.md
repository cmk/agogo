# output — triage of Gemini chat, heterogeneous per-channel output

**Source**: note lines 1962–2197 (multiplexing, per-channel output
routing, heterogeneous `OutputFormat` enum, dispatcher thread,
latency-mismatch discussion) and 687–786 (OSC sync as a peer
protocol).

**Context**: v0.4 owns heterogeneous output dispatch — one
channel on CV, one on MIDI, one on OSC, one on MTC, all sample-
aligned via per-format latency compensation. v0.4 verification names
`hetero_dispatch_preserves_tick_order`, `latency_compensation_is_declared`,
and `diagnostic_sink_reports_jitter`.

## Adopt

- **Per-channel `OutputFormat` enum as the dispatch type.** Shape
  per Gemini lines 2086–2092: `{ CV { channel_idx }, Midi {
  port_idx }, Osc { addr }, Link, Mtc { fps } }`. The variant
  carries its own routing specifier. Sits on `Channel` next to
  the existing `ChannelMode` (see `crates/core/src/channel/mode.rs`).
  Likely `ChannelMode` and `OutputFormat` merge into one enum in
  v0.4 — design the naming then.
- **Two-phase dispatch: compute tick → sample index on RT,
  dispatch non-audio from a worker.** CV writes directly into the
  cpal output buffer inside the RT callback. MIDI/OSC events get
  pushed to an SPSC queue with a `(message, target_host_time)`
  tuple; a dedicated dispatcher thread reads the queue and sleeps
  until `Instant::now() ≥ target_host_time` before handing off to
  the OS. Keeps the RT thread allocation-free.
- **Per-format latency compensation table.** CV is zero latency
  once the sample reaches the DAC. USB MIDI is ~1 ms.
  rtpMIDI/network OSC is tens of ms plus jitter. Each format
  declares its compensation, and the dispatch layer subtracts it
  from the target host-time so all formats land on "the one"
  simultaneously. The v0.4 contract is truthful capability reporting:
  exact, measured, estimated, or unsupported.
- **MIDI Clock at 24 PPQN, derived from the master tick stream.**
  At agogo's 192/960 PPQN master, one MIDI tick (0xF8) every
  `PPQN/24` master ticks — integer at both 192 and 960. Send
  `0xFA` on transport start, `0xFC` on stop, `0xFB` on continue.
  Standard wire protocol, no invention.
- **OSC payload includes a high-resolution timestamp.** `/clock/tick
  [beat_index, host_time_micros]`. NTP-style or raw µs is fine —
  stdio-core already has an opinion on this via
  `stdio-core-osc`; align with that. The timestamp lets a receiver
  on a different machine reason about network jitter.

## Defer

- **OSC-based peer-to-peer sync (agogo ↔ agogo over OSC).**
  Mentioned in the chat but not a v0.4 requirement. Ableton Link
  is the supported peer protocol; OSC is output-only unless a
  concrete use case surfaces.
- **SIMD-optimized interleaving for 32+ channels.** Gemini's
  concern about interleaved memory access at scale. The Multiclock
  hardware is 4 channels; agogo.md §10 flags "4 vs N generic" as
  an open question. Until that's answered "N generic and > 16,"
  don't spend on SIMD.
- **Auto-detection of physical interface output count.** Useful
  polish for the CLI, but in v0.4 the user passes a flag
  (`--cv-out-device`, `--midi-out-port`, …). Device enumeration is
  cpal-level boilerplate and belongs near the CLI arg parsing.

## Reject

- **Pushing MIDI sends from the RT audio thread directly through
  `midir`.** `midir::MidiOutputConnection::send` is not
  realtime-safe (allocates, blocks on OS MIDI driver). Hence the
  dispatcher-thread design above. Flagged because the chat at one
  point sketches sending MIDI from inside the buffer loop.
- **MTC at odd subdivisions (e.g. triplets) as "advanced option
  B".** Gemini floats this in lines 2174–2177. Reject: MTC is a
  wire protocol with a fixed frame-rate schema; bending it to
  agogo's subdivision lattice breaks receiver compatibility.
  Keep MTC as a fixed 24/25/30 fps option, see `mtc.md`.
- **Writing directly to specific interleaved audio indices with
  hand-rolled `f * num_channels + ch.output_index` arithmetic.**
  cpal's channel layout APIs already do this correctly; use them.
