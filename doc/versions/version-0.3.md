# agogo v0.3 - Clock Domains And Followers

## Thesis

v0.3 turns the v0.2 steel-thread kernel into a clock-domain engine. It proves
agogo can be both source and follower across the domains producers actually use:
audio sample time, musical tick time, Link host time, MIDI clock, and external
audio pulse.

## In Scope

- Per-buffer Link host-time anchor with torn-read-safe publication.
- Link follower/source state machine:
  - local-only internal source
  - Link follower
  - Link source/publisher
  - transport start/stop sync gates
- MIDI clock follower:
  - incoming realtime-byte parser off the callback path
  - jitter model and PLL input
  - lock/loss transitions
- Audio pulse follower hardening:
  - detector/PLL convergence specs
  - dropouts and noisy inputs
  - explicit lock state in snapshots
- 960 PPQN grid expansion if not fully completed in v0.2.

## Out Of Scope

- CV output, MTC, and OSC output formatting.
- Full preset/project persistence.
- Product UI beyond snapshot/report surfaces.

## Properties

| Property | Invariant |
| --- | --- |
| `host_time_anchor_torn_read_safe` | Racing anchor reader/writer never observes mixed fields. |
| `link_phase_query_bounded` | Link phase queries meet the callback budget. |
| `midi_clock_follower_converges` | Follower converges within the documented jitter/tempo envelope. |
| `follower_loss_is_explicit` | Lost external clock moves to an explicit unlocked/lost state. |
| `source_switch_has_defined_phase` | Switching source domains has declared phase behavior and no implicit jump. |
| `sample_tick_roundtrip_960` | Tick/sample conversion round-trips for supported exact-rate bands. |

## Acceptance

- Demo: agogo follows a Link peer through start/stop and a tempo step while
  emitting clock through the v0.2 output backend.
- Demo: agogo follows incoming MIDI clock and reports lock/loss in
  `agogo-state`.
- No source/follower code introduces model, stdio-core, MCP, filesystem, or
  logging dependencies into the callback path.

## Deferred To v0.4

- Heterogeneous output dispatch.
- CV pulse output.
- MTC quarter-frame generation.
- Per-format latency compensation.
