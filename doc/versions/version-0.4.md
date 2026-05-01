# agogo v0.4 - Heterogeneous Output And Latency

## Thesis

v0.4 expands from "clock engine with honest MIDI timing" to "multi-format
studio timing engine." The work is not just adding formats; it is proving that
each format declares its timing capability and latency model so the product does
not make false sample-accuracy claims.

## In Scope

- Unified output dispatch with explicit output classes:
  - MIDI clock/transport/CC
  - CV/gate pulse output through audio interfaces
  - OSC tick/control messages
  - MTC quarter-frame output
- Per-format latency profiles:
  - static compensation
  - measured diagnostics
  - backend capability report
- CV output:
  - cpal output host
  - single-sample impulse mode
  - configurable polarity and amplitude
  - interleaved multi-channel output
- MTC:
  - SMPTE rate selection
  - quarter-frame generator
  - reader round-trip tests
- Snapshot extensions for per-output health and timing capability.

## Properties

| Property | Invariant |
| --- | --- |
| `hetero_dispatch_preserves_tick_order` | Mixed output formats preserve master tick order. |
| `cv_impulse_sample_exact` | CV impulses land on the intended sample index. |
| `cv_impulse_one_sample_energy` | Each CV impulse is exactly one non-zero sample unless a configured pulse width says otherwise. |
| `mtc_quarter_frame_roundtrips` | Generated MTC reconstructs the original timecode in a reference reader. |
| `latency_compensation_is_declared` | Every output backend reports whether compensation is exact, measured, estimated, or unsupported. |
| `diagnostic_sink_reports_jitter` | Diagnostic output records intended and observed timing without entering the callback hot path. |

## Acceptance

- Demo: one running `Machine` drives MIDI clock, CV pulse, and OSC tick outputs
  from the same master timeline.
- Snapshot reports output health, drop counters, and timing capability per
  output.
- Documentation distinguishes internal sample accuracy from external transport
  accuracy for every output class.

## Deferred To v0.5

- Preset/project persistence.
- Remote control mappings.
- Product-level calibration workflow.
- Hardware-in-the-loop certification matrix.
