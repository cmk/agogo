# integer-math — triage of Gemini chat, PPQN / sample-rate lattice

**Source**: `doc/notes/note-2026-04-23-03.md` lines 1–317 (the PPQN
pitch; the 960 prime-factorization argument; the first fixed-point
accumulator example).

**Context**: v0.2 is already scoped around retrofitting 192 → 960 PPQN
and proving integer-exactness at 48 k / 96 k. This file just records
what from the chat is load-bearing vs. decorative for that sprint.

## Adopt

- **960 PPQN target, justified by prime factorization.** 960 = 2⁶·3·5
  divides `60·sr/ppqn` exactly at `sr ∈ {48_000, 96_000}` and at a
  usefully wide band of integer BPMs. The v0.2 plan already encodes
  this as `stc_samples_per_tick_is_exact_at_48k` / `_96k` properties.
- **The 5 factor gives pentuplets.** 192 PPQN cannot express
  quintuplets at integer tick counts; 960 can (`960 / 5 = 192` ticks
  per quintuplet). Add `T5`, `T10`, `T20`, `T40`, `T80` (or the chosen
  naming) to `TBase::ALL` in Plan v0.2-01 — the lattice closure proof
  already needs to extend to `2^i · 3^j · 5^k`.
- **Ticks-per-bar is always integer at 960.** For any `n/d` with
  `d ∈ {1,2,4,8,16,32}`, `ticks_per_bar = n·4·960/d ∈ ℤ`. v0.2's
  `ticks_per_bar_integer_all_time_sigs` property nails this.
- **The 44.1 kHz caveat is real but tolerable.** 44_100 / 960 is not an
  integer at any BPM; at 44.1 k we accept a few-picosecond rounding
  in the runtime-computed `samples_per_tick` (well under any MIDI
  jitter floor). Call this out in the v0.2 review doc so nobody
  expects the `stc_samples_per_tick_is_exact` property to hold at
  44.1.

## Defer

- **`u64.32` fixed-point phase accumulator.** Gemini's running
  example everywhere is `phase: u64` with 32 fractional bits and a
  `wrapping_add(advance_per_sample)` hot loop. This is fine engineering
  for a monolithic clock, but agogo's architecture (agogo.md §2) keeps
  time in `Tick(u32)` and converts to `Sample` only via
  `SampleTickConn` at the output boundary. The accumulator pattern is
  structurally the wrong shape for us. If a future sprint needs a
  sample-resolution phase inside one channel's renderer, revisit then.
- **Scaling BPM as `bpm * 100`.** The chat's convenience trick for
  decimal BPMs. agogo carries BPM as an explicit `Tempo` type
  (`crates/core/src/fxp.rs`); different scale, same idea. No change
  needed.

## Reject

- **"PPQN of 1<<32 gives you 4 billion sub-tick subdivisions".**
  Confuses two roles of fractional bits. We already have a clean
  separation: `Tick` is the musical grid, `Sample` is the output-rate
  grid, and fractional resolution below a tick only exists during
  audio-rate rendering (LFOs, sample-accurate impulse placement).
  Nothing in the core should be at 2³² sub-tick resolution.
- **`wrapping_add` on a 64-bit accumulator "so a jam session doesn't
  panic".** In agogo, `Tick(u32)` is explicitly not expected to wrap —
  one tick per 2²⁵ samples @ 48 k / 960 means ~24 hours of run-time
  before u32 ticks approach exhaustion, and the design permits the
  control plane to reset phase when that matters. Overflow would be a
  bug, not a wrap.
