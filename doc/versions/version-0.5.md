# agogo v0.5 - Persistence, Calibration, And Product Hardening

## Thesis

v0.5 makes the timing kernel operationally durable. The hard technical risks
should already be retired by v0.2-v0.4; v0.5 is about saving state, calibrating
hardware, producing diagnostics users can trust, and tightening the API for
long-lived studio sessions.

## In Scope

- Preset/state persistence:
  - serialize machine configuration without sample-rate-specific drift
  - versioned schema
  - migration tests
- Calibration:
  - per-device and per-output latency profiles
  - diagnostic capture mode
  - measured jitter summaries
- Remote control mappings:
  - MIDI CC/NRPN/pitch-bend control of safe parameters
  - command admission still goes through the v0.2 bridge
- Hardware-in-the-loop suites:
  - opt-in only
  - explicit device selection
  - safe defaults for monitor/speaker-sensitive paths
- API hardening:
  - stable command envelope
  - stable snapshot schema
  - compatibility tests against stdio-core and stdio steel-thread fixtures

## Properties

| Property | Invariant |
| --- | --- |
| `preset_roundtrip_stable` | Serialized and deserialized machine state is equivalent. |
| `preset_sample_rate_independent_where_claimed` | Presets marked sample-rate-independent retain musical timing across supported rates. |
| `calibration_profile_applies` | Applying a latency profile changes scheduled output by the declared amount. |
| `remote_control_respects_admission` | Remote mappings cannot bypass command validation or safety class. |
| `steel_thread_compatibility` | agogo remains compatible with the stdio-core and stdio roadmap fixtures. |

## Acceptance

- Long-running demo can be stopped, saved, restored, and resumed with the same
  musical state.
- Calibration diagnostics produce a human-readable report and machine-readable
  profile.
- All hard-time contracts from v0.2 remain enforced.

## Post-v0.5

- Additional platform-native MIDI backends beyond the first proven backend.
- rtpMIDI/network MIDI.
- DAW-specific integrations.
- Plugin formats, if ever needed, remain a separate product decision.
