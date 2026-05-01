# pid — triage of Gemini chat, PID tuning for external-sync follower

**Source**: note lines 1222–1398 (coefficient tuning guide,
anti-windup, low-pass pre-filter, deadband, hard-sync threshold).

**Context**: agogo already ships a Type-II PI loop filter in
`crates/core/src/sync/pll.rs` tracking an audio-sync pulse train.
The PID material Gemini discusses is primarily about a *different*
controller: the one that drags agogo's phase toward Ableton Link's
continuous timeline (v0.3 `PhaseSource::Link`). This doc triages the
chat into reusable principles for both loops.

## Adopt

- **Start with just P and I; leave D at zero.** The PLL in
  `sync/pll.rs` is already a PI controller. For the v0.3 Link
  follower, start the same way. D amplifies noise in a jittery
  timeline like Link's network-derived beat, and the existing PLL
  proves PI alone is enough to hit ±0.05 BPM at ≤200 µs input
  jitter.
- **Anti-windup clamp on the integrator is non-negotiable.**
  `state.integral_error.clamp(-MAX_I, MAX_I)`. If the clock is
  paused (or the Link peer teleports) the integrator runs away
  without this. The existing `sync/pll.rs` clamps through
  `clamp_hz` on the integrator-derived frequency; the Link
  follower needs the same shape on phase.
- **Deadband inside the lock region.** Below a threshold error
  (chat suggests "1/nᵗʰ of a tick"), don't update the controller at
  all. This stops "hunting" around true zero caused by quantization
  noise in the measurement chain. Makes sense as a v0.3 addition
  for the Link follower; optional for the audio-sync PLL where it
  hasn't been an observed problem yet.
- **One-pole low-pass on the error signal, pre-controller.**
  Gemini's `smooth_error = (smooth_error * 9 + raw_error) / 10`.
  Smooths network / OS-derived jitter before it reaches the
  controller. The existing PLL accomplishes the same thing via
  the integrator's inherent averaging; the Link follower needs it
  more explicitly because Link's raw-beat readout can bounce by
  tens of µs between callbacks.
- **Hard-sync threshold beyond which the PID is bypassed.** If the
  phase error exceeds ~1/8 note, teleport instead of steering.
  Expand in `transport.md` — this is the interface between PID and
  FSM.
- **Tuning procedure as doc, not code.** Keep the "zero I and D,
  find P, add I, run the jerk test" sequence in this doc as a
  guide for whoever tunes the v0.3 Link follower. Not an automated
  autotune; just a procedure.

## Defer

- **Logging sync-quality history to CSV for post-session review.**
  Nice for tuning debugging; v0.2's `agogo-state` stream and v0.4's
  diagnostic sink may already give the same visibility. Revisit if
  telemetry lacks the resolution for offline analysis.
- **Wrapped error calculation around the Link quantum.** Covered
  in `link.md`. It's a PID concern — the controller has to take
  the short path around a quantum — but the derivation lives with
  the Link-specific math.

## Reject

- **Hand-tuned PID coefficients in the chat.** Gemini suggests
  `error >> 11` / `total >> 16`. These are starting-point-only;
  the actual values should come from the tuning procedure applied
  to agogo's specific plant (sample rate, buffer size, Link
  jitter profile). Don't copy the numbers.
- **"You probably don't need D at all."** True for most cases, but
  this is a rule of thumb not a rule. Leave the controller
  general: `PidSettings { kp, ki, kd, … }` with `kd: 0.0` default.
  Forcing `kd` out of the struct makes it impossible for a
  contributor to experiment without surgery.
