# agogo v0.2

## Goal

Retrofit the time core from **192 PPQN** to **960 PPQN**, proving the
`TBase` lattice and the `SampleTickConn` algebra survive the jump
from `2^i · 3^j` tick counts to `2^i · 3^j · 5^k`. At 960 PPQN with
48 kHz or 96 kHz sample rate the Sample↔Tick relationship is
integer-exact for a wide band of integer BPMs, and this sprint
nails that down as property tests.

The two high-priority drivers for v0.2 are:

1. **Integer math.** 960 = 2⁶·3·5 divides `60·48_000 = 2_880_000`
   and `60·96_000 = 5_760_000` evenly; at the sweet-spot BPMs,
   samples-per-tick is an exact integer — no floor truncation, no
   drift over hours of playback.
2. **Pentuplets.** The Cirklon-style grid lattice now admits 5-factor
   subdivisions, which v0.1's 192-PPQN set could not express.

## Sprint slots

| # | Slug | Status | Scope |
|---|------|--------|-------|
| 01 | `plan-2026-04-2N-01` (TBD) | next | `time/` types & consts: bump `PPQN` (tick.rs:21) to 960; add pentuplet variants to `TBase::ALL` (tbase.rs); update `from_tick_count` lookup; relax the "tick counts are `2^i · 3^j`" assumption (tbase.rs:136–137) to `2^i · 3^j · 5^k` and re-prove LCM/GCD closure. |
| 02 | `plan-2026-04-2N-02` (TBD) | next-next | Lattice & Conn laws at 960: re-prove divisibility preorder (tbase.rs:79–134), round-trip `SampleTickConn` property (conn.rs:915–932). Widen `arb::arb_integer_stc` in `crates/core/src/arb.rs` to include 5-factor tick counts and 48k/96k exact-rate BPMs. |
| 03 | `plan-2026-04-2N-03` (TBD) | last | Integer-exactness properties at 48k / 96k: new module `time::exact_rates` with the property set below. Audit `channel/` (divider, shuffle, shift) and `sync/` PLL for regression under 960. |

## Properties (must pass)

| Property | Module | Invariant |
|----------|--------|-----------|
| `stc_samples_per_tick_is_exact_at_48k` | `time::exact_rates` | For `sr = 48_000`, `ppqn = 960`, and `bpm` in the integer divisors of `sr·60/ppqn = 3000`, `SampleTickConn::tick_to_sample(t)` is exact (no floor truncation) for all `t ≤ 2³²`. |
| `stc_samples_per_tick_is_exact_at_96k` | `time::exact_rates` | Same as above for `sr = 96_000`, where the BPM set expands to divisors of `6000`. |
| `stc_round_trip_identity_48k_96k` | `time::exact_rates` | At the exact-rate BPMs above, `sample_to_tick ∘ tick_to_sample = id` and `tick_to_sample ∘ sample_to_tick = id` on tick-aligned samples. Strengthens the existing `stc_round_trip` from a ceiling-bound to equality. |
| `ticks_per_bar_integer_all_time_sigs` | `time::tbase` | At `PPQN = 960`, for any time signature `n/d` with `d ∈ {1,2,4,8,16,32}`, `ticks_per_bar = n · 4 · 960 / d` is a positive integer. Covers 4/4, 5/4, 7/8, 11/16, 15/32 without rationals. |
| `pentuplet_subdiv_integer_ticks` | `time::tbase` | Each new quintuplet `TBase` variant has `tick_count` dividing 960 exactly, so channel-side subdivision never produces a fractional-tick offset. |
| `channel_subdiv_preserves_phase_960` | `channel::divider` | At `PPQN = 960` with any `TBase` in `ALL`, dividing a master-tick stream by the base's `tick_count` yields the same phase as generating at that base directly (idempotence of `subdivide ∘ join`). |
| `tbase_lattice_closure_960` | `time::tbase` | `TBase::ALL` under `lcm` / `gcd` is closed at `PPQN = 960` with the added 5-factor variants (extends the existing 192-era closure proof). |

## v0.2 acceptance

- `cargo test --workspace` green with `PPQN = 960`.
- All v0.1 property tests that parameterize on PPQN still pass.
- Every property in the table above passes without `#[ignore]`.
- No regression in PLL jitter spec from Plan 02 (±0.05 BPM
  steady-state at ≤ 200 µs input jitter).
- `cargo run -p agogo-cli -- agogo run --audio-in <dev> --midi-out
  <port> --bpm 120` still emits locked MIDI clock (acceptance
  inherited from v0.1).

## Deferred to v0.3 (and beyond)

- stdio-core driver surface & lock-free control plane — v0.3.
- Observation/telemetry push + CV pulse output — v0.4.
- Transport FSM, Ableton Link, heterogeneous outputs, MTC — v0.5.
- All v0.1 deferred items that carry into v0.2+ unchanged
  (preset I/O, platform-native MIDI sinks, LFO render, rtpMIDI,
  remote control).

## Reference

- `doc/agogo.md` §6 (Cirklon mapping — verify 960 PPQN doesn't
  break bar-length invariants), §7 (`SampleTickConn` workaround).
- `doc/notes/note-2026-04-23-03.md` lines 226–443 (integer-math
  rationale; the "50 samples per tick" Golden Ratio at 48k/960).
- `doc/versions/version-0.1.md` — deferred list carried forward.
- `crates/core/src/time/tick.rs:21` — `PPQN` const to flip.
- `crates/core/src/time/tbase.rs:136–137` — lattice invariant to
  extend from `2^i · 3^j` to `2^i · 3^j · 5^k`.
- `crates/core/src/time/conn.rs:241–330` — `SampleTickConn` (already
  PPQN-parameterized at runtime, so cheap to retarget).
