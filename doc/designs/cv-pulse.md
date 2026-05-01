# cv-pulse — triage of Gemini chat, CV impulse rendering

**Source**: note lines 448–560 (single-sample impulse, bipolar reset)
and 1883–1926 (the "pending reset" flag in the full audio callback).

**Context**: v0.4 ships CV/gate output through the heterogeneous output
layer and owns properties `cv_impulse_sample_exact` and
`cv_impulse_one_sample_energy`.

## Adopt

- **Single-sample Dirac impulse as the CV pulse shape.** One sample
  at full scale followed by zero. Hardware clock inputs trigger on
  the rising edge, which means the pulse's energy can be confined to
  one sample without hurting detection. This is exactly the v0.4
  `cv_impulse_one_sample_energy` property.
- **Bipolar option (±1.0) to avoid DC creep on AC-coupled outputs.**
  Many audio interfaces have AC coupling on outputs; a monopolar
  train of 1.0 spikes drifts the DC offset and eventually the
  capacitor blocks the pulse. One sample at `+1.0`, next sample at
  `-1.0` keeps integrated DC at zero. Ship both shapes and default
  to bipolar when the user hasn't opted into a DC-coupled interface.
- **"Pending reset" flag for buffer-boundary pulses.** When a pulse
  lands on the last sample of buffer N, the `-1.0` reset has to
  spill into sample 0 of buffer N+1. Gemini's
  `pending_bipolar_reset: bool` on the channel state is the cleanest
  shape. Add a proptest: across concatenated buffers, every
  bipolar-enabled pulse has exactly one `+1.0` and one `-1.0`
  separated by one sample, regardless of where the pulse falls
  relative to buffer boundaries.
- **0 dBFS output + no limiter on the sync bus.** Gate this as an
  out-of-band doc line rather than code: when agogo writes to a
  cpal output stream, it assumes the stream is routed to an
  interface output with no master processing. Worth a line in the
  CLI `--help` text.

## Defer

- **Widening the pulse to 2–4 samples if an interface's
  anti-aliasing filter smears a 1-sample spike.** Worth having as a
  `--pulse-width N` CLI flag but not a v0.4 baseline concern — v0.4 proves
  sample-accurate alignment, which is the hard part. Extra-width
  modes are cosmetic once the timing is right.

## Reject

- **Clearing the output buffer at the top of every render.** cpal
  callbacks receive a buffer whose contents are undefined; we have
  to write every sample anyway. Gemini's `for sample in
  output_buffer.iter_mut() { *sample = 0.0; }` preamble is
  redundant with a properly structured render loop that writes
  every index.
- **Routing "channel 1 → output 3" as a hand-written interleave.**
  cpal already exposes multi-channel output buffers; we should use
  its channel layout rather than Gemini's `sample_idx = (f *
  num_hardware_channels) + ch.output_index` hand-rolled math. The
  shape is the same; we just use the idiomatic cpal API.
