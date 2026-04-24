# mtc — triage of Gemini chat, MIDI Time Code generator

**Source**: note lines 2199–2378 (MTC vs MIDI Clock, quarter-frame
state machine, SMPTE frame-rate prime-factorization, non-drop advice).

**Context**: v0.5 Sprint 04 is MTC quarter-frame generation. The
v0.5 verification includes `mtc_quarter_frame_round_trips` —
generated MTC fed back through a reference reader must recover the
original SMPTE timecode exactly.

## Adopt

- **Quarter-frame messages via `0xF1 nn` where `nn = 0yyyzzzz`.**
  8 messages carry one full timecode (2 frames' worth of data
  spread across 8 quarter-frame slots). State machine cycles
  through `piece_index: 0..8`; each piece encodes one nibble of
  the SMPTE `(hh, mm, ss, ff)` quadruple.
- **30 fps non-drop is the default at 48 kHz.** 48_000 / 30 = 1600
  samples per frame, `1600 / 4 = 400` samples per quarter-frame.
  Integer math on the sample counter with no f64 anywhere —
  matches agogo's integer-exactness philosophy. Prime-factor tie-in
  with the 960 PPQN retrofit from v0.2.
- **Also-integer rates: 24, 25, 30.** At 48 kHz, samples-per-frame
  is `2000`, `1920`, `1600` respectively. Ship all three as
  first-class options. 25 fps is essential for PAL video
  interop, 24 for film.
- **Drop-frame (29.97) is opt-in only.** `48000 / 29.97 =
  1601.601…` — non-integer. Supporting it correctly requires
  frame-skip accounting (drop 2 frames every minute except every
  10th) plus a fractional sample counter. Do not ship by default;
  add behind a feature flag only if a user asks.
- **Drive MTC from the musical timeline, not wall-clock time.**
  Gemini flags this correctly (lines 2235–2240): when Link speeds
  up, the musical tick stream accelerates but real-world seconds
  don't. Users expect "Bar 100" to land on the same SMPTE time
  every take, so MTC should be derived from the Tick-master, not
  from `std::time::Instant`.

## Defer

- **MMC (MIDI Machine Control) Stop/Start/Locate.** Natural
  companion to MTC — a hardware recorder receiving MTC + MMC can
  chase agogo's transport. Belongs in a post-0.5 sprint; name the
  sprint-slot `mmc-transport` and note it as a v0.5 deferred item.
- **TUI display of the current SMPTE position.** Trivial once the
  MTC generator holds an `(hh, mm, ss, ff)` field; surfaces
  through the v0.4 observation snapshot. Add it when agogo's TUI
  grows a timecode widget, not before.

## Reject

- **Supporting `29.97` without drop-frame accounting.** "Just
  approximate" looks innocent and compounds across a 30-minute
  take into visible offset. Either do drop-frame properly or
  refuse the setting.
- **Deriving MTC from wall-clock time so it stays "real time"
  during tempo changes.** Musical sync users want the opposite
  (`Bar 100 → 00:01:38:12` every take). Video/film sync users who
  need real-time SMPTE are not served by agogo — they need a
  genlock or LTC reader, which is out of scope.
