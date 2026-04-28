//! `agogo link probe` — sample tempo / peers / phase from a live
//! Link session at fixed cadence.
//!
//! Plan 2026-04-28-05 T5: extracted from `cli/main.rs`.

use agogo_core::sync::PhaseSourceImpl;
use agogo_host_link::{HostTimeAnchor, LinkClock};
use std::num::NonZeroU32;
use std::thread::sleep;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy)]
pub struct ProbeRow {
    /// Milliseconds since probe start. `u64` so a
    /// `--duration-ms u32::MAX` probe (~49 days) still represents
    /// monotonically-increasing timestamps end-to-end.
    pub t_ms: u64,
    pub peers: u64,
    pub tempo: agogo_core::time::tempo::Tempo,
    /// Beat-phase in `[0, 1)` at sample `t_ms × sr / 1000`,
    /// mapped through the anchor captured at probe start.
    pub phase: agogo_core::sync::phase::Phase,
}

/// Run a probe loop for `duration_ms`, sampling every `period_ms`.
/// Each sampled row is passed to `on_row` synchronously so callers
/// can stream directly to stdout (or collect into a Vec for
/// tests). Peer discovery is enabled for the duration of the call
/// and disabled before return. Blocks the calling thread; intended
/// for the CLI, not the audio callback.
///
/// `period_ms` is clamped to a minimum of 1 — a zero period would
/// turn the `sleep(Duration::ZERO)` inside the loop into a no-op
/// and starve the row consumer if it can't keep up.
pub fn probe<F: FnMut(ProbeRow)>(
    initial_tempo: agogo_core::time::tempo::Tempo,
    sr: u32,
    duration_ms: u32,
    period_ms: u32,
    mut on_row: F,
) {
    let period_ms = period_ms.max(1);
    // The CLI parser (`parse_positive_u32`) already enforces
    // `sr >= 1`. Preserve that invariant explicitly here so
    // non-CLI callers fail fast on `sr = 0` instead of silently
    // mapping to 1 and producing wrong sample-index math.
    let sr = NonZeroU32::new(sr).expect("probe requires a non-zero sample rate");
    // Capture Link's current host-time once and use it as the
    // anchor origin so the phase column reads as "cycles elapsed
    // since probe start" rather than against an arbitrary epoch.
    // Construct with a placeholder anchor, read `clock_micros`,
    // then `set_anchor` with the real origin — avoids the
    // two-AblLink-instance throwaway pattern.
    let mut clock = LinkClock::new(
        initial_tempo,
        HostTimeAnchor {
            host_origin_micros: 0,
            sample_rate: sr,
        },
    );
    clock.set_anchor(HostTimeAnchor {
        host_origin_micros: clock.clock_micros(),
        sample_rate: sr,
    });
    clock.enable(true);
    let start = Instant::now();
    let duration = Duration::from_millis(u64::from(duration_ms));
    let period = Duration::from_millis(u64::from(period_ms));
    loop {
        let elapsed = start.elapsed();
        if elapsed > duration {
            break;
        }
        let t_ms = elapsed.as_millis() as u64;
        // Convert t_ms → sample index using the anchor's sample
        // rate, then query phase.
        let n = t_ms * u64::from(sr.get()) / 1_000;
        let phase_u32 = clock.phase_at_sample(n).0;
        on_row(ProbeRow {
            t_ms,
            peers: clock.num_peers(),
            tempo: clock.tempo(),
            phase: agogo_core::sync::phase::Phase(phase_u32),
        });
        sleep(period);
    }
    clock.enable(false);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Smoke-test: probing for 100ms at 50ms period emits at
    /// least one row; first row has t_ms ≈ 0, peers = 0 (no LAN
    /// peer in test), tempo equal to the initial BPM, and phase
    /// in `[0, 1)`.
    ///
    /// Touches the network via `LinkClock::enable(true)` under
    /// the hood. `peers == 0` fails if a real Link peer is
    /// reachable on the test LAN; Plan 09 adds a
    /// `fixture_or_skip!`-style network gate.
    #[test]
    fn probe_emits_rows_and_keeps_initial_tempo() {
        let mut rows = Vec::new();
        probe(
            agogo_core::time::tempo::Tempo::from_bpm_integer(125),
            48_000,
            100,
            50,
            |row| rows.push(row),
        );
        assert!(!rows.is_empty(), "probe returned no rows");
        let first = rows[0];
        assert_eq!(first.peers, 0);
        // Tempo is integer µBPM: 125 BPM → 125_000_000.
        assert_eq!(
            first.tempo,
            agogo_core::time::tempo::Tempo(125_000_000),
            "tempo {:?} differs from initial Tempo(125_000_000)",
            first.tempo
        );
        // Phase is Q0.32 — in [0, 2^32), representing [0, 1) cycles.
        let phase_cycles = f64::from(first.phase.0) / (1u64 << 32) as f64;
        assert!(
            (0.0..1.0).contains(&phase_cycles),
            "phase {} not in [0, 1)",
            phase_cycles
        );
    }
}
