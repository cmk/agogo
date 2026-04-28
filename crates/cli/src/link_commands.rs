//! Plan 09 link subcommands: `push-tempo`, `transport`, `diag`.
//! All three exit after a bounded duration — none is a persistent
//! daemon. `transport` drives the FSM headlessly; audio-callback
//! integration (real `agogo run --link`) lands with Plan 05.
//!
//! Plan 2026-04-28-05 T4: extracted from `cli/main.rs`.

use agogo_core::boundary::tempo_to_f64_bpm;
use agogo_core::time::tempo::Tempo;
use agogo_host_link::{HostTimeAnchor, LinkSession, LinkWriteConfig, Quantum};
use std::num::NonZeroU32;
use std::thread::sleep;
use std::time::{Duration, Instant};

fn anchor_for(sr: u32) -> HostTimeAnchor {
    let sr = NonZeroU32::new(sr).expect("sr validated at argv boundary");
    HostTimeAnchor {
        host_origin_micros: 0,
        sample_rate: sr,
    }
}

/// One-shot tempo push. Enables the network, calls
/// `session.set_tempo`, sleeps `settle_ms` so peers can capture,
/// disables, and exits.
pub fn push_tempo(bpm: Tempo, settle_ms: u32) {
    let mut session = LinkSession::new(bpm, anchor_for(48_000), LinkWriteConfig::default());
    session.enable(true);
    session.set_tempo(bpm);
    sleep(Duration::from_millis(u64::from(settle_ms)));
    session.enable(false);
    // stdout for scripts: single line with the pushed BPM,
    // formatted at two decimal places (round-tripped through
    // `Tempo`'s integer µBPM storage, so sub-2-decimal precision
    // is meaningless to print).
    println!("pushed_bpm={:.2}", tempo_to_f64_bpm(bpm));
}

/// Headless transport runner. Subscribes to Link's `is_playing`
/// via `poll_transport` every 10 ms, prints state transitions,
/// and optionally drives `UserStart` / `UserStop` at the bounds.
pub fn transport(
    bpm: Tempo,
    quantum: Quantum,
    sr: u32,
    duration_ms: u32,
    start: bool,
    stop_on_exit: bool,
) {
    let config = LinkWriteConfig {
        default_quantum: quantum,
        ..LinkWriteConfig::default()
    };
    let mut session = LinkSession::new(bpm, anchor_for(sr), config);
    session.enable(true);
    if start {
        session.user_start();
    }
    let deadline = Instant::now() + Duration::from_millis(u64::from(duration_ms));
    let poll_period = Duration::from_millis(10);
    let mut last_playing = session.is_playing();
    let mut last_peers = session.num_peers();
    let mut last_tempo = session.tempo();
    println!("t_ms,peers,tempo_bpm,is_playing");
    let start_instant = Instant::now();
    println!(
        "{},{},{:.4},{}",
        0,
        last_peers,
        tempo_to_f64_bpm(last_tempo),
        last_playing as u8,
    );
    while Instant::now() < deadline {
        sleep(poll_period);
        session.poll_transport();
        let playing = session.is_playing();
        let peers = session.num_peers();
        let tempo = session.tempo();
        if playing != last_playing || peers != last_peers || tempo != last_tempo {
            let t_ms = start_instant.elapsed().as_millis() as u64;
            println!(
                "{},{},{:.4},{}",
                t_ms,
                peers,
                tempo_to_f64_bpm(tempo),
                playing as u8,
            );
            last_playing = playing;
            last_peers = peers;
            last_tempo = tempo;
        }
    }
    if stop_on_exit {
        session.user_stop();
    }
    session.enable(false);
}

/// Single-line diagnostic summary.
pub fn diag(bpm: Tempo, sr: u32, settle_ms: u32) {
    let mut session = LinkSession::new(bpm, anchor_for(sr), LinkWriteConfig::default());
    session.enable(true);
    sleep(Duration::from_millis(u64::from(settle_ms)));
    session.poll_transport();
    let peers = session.num_peers();
    let tempo = session.tempo();
    let playing = session.is_playing();
    session.enable(false);
    println!(
        "peers={peers} tempo_bpm={:.4} is_playing={}",
        tempo_to_f64_bpm(tempo),
        playing as u8,
    );
}
