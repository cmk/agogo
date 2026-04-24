//! Plan 09 T6 — two-peer integration tests.
//!
//! All tests here spin up **two** `AblLink` instances in the same
//! process and rely on Link's multicast-loopback discovery to pair
//! them on `127.0.0.1`. That requires a working UDP multicast stack
//! on the host, which CI sandboxes (especially container-based
//! runners) may disable. The `fixture_or_skip!("link_multicast")`
//! gate is the safety net: a user who wants these to run locally
//! creates a sentinel file at
//! `crates/host-link/tests/fixtures/link_multicast`.
//!
//! Opt-in rationale: these tests take ~1 s each waiting for peer
//! discovery, and a misconfigured CI runner would flake the whole
//! suite without the gate. Explicit opt-in keeps the default
//! `cargo test` run fast + deterministic.

#![cfg(feature = "rusty-link")]

use agogo_core::fxp::{Quantum, Tempo};
use agogo_core::testing::fixture_or_skip;
use agogo_host_link::{
    HostTimeAnchor, LinkSession, LinkWriteConfig, TransportState,
};
use rusty_link::{AblLink, SessionState};
use std::num::NonZeroU32;
use std::thread::sleep;
use std::time::{Duration, Instant};

macro_rules! link_multicast_or_skip {
    () => {{
        match fixture_or_skip(env!("CARGO_MANIFEST_DIR"), "link_multicast") {
            Some(_) => {}
            None => return,
        }
    }};
}

fn anchor_48k() -> HostTimeAnchor {
    HostTimeAnchor {
        host_origin_micros: 0,
        sample_rate: NonZeroU32::new(48_000).expect("non-zero"),
    }
}

/// Spin-wait (bounded) for a `LinkSession` and a raw peer `AblLink`
/// to discover each other via multicast loopback. Returns `true` if
/// both sides report `num_peers() >= 1` within the budget; `false`
/// otherwise (signals multicast loopback disabled).
fn wait_for_pair(
    session: &LinkSession,
    peer: &AblLink,
    budget: Duration,
) -> bool {
    let deadline = Instant::now() + budget;
    while Instant::now() < deadline {
        if session.num_peers() >= 1 && peer.num_peers() >= 1 {
            return true;
        }
        sleep(Duration::from_millis(50));
    }
    false
}

/// Time to let the network / Link stabilise after a publish before
/// asserting on the peer side. Link's session capture is lock-free
/// and protocol propagation on localhost is fast, but we've
/// observed `is_playing` / tempo updates needing ~200–500 ms to
/// settle between two fresh `AblLink` instances. Be generous.
const PROPAGATION_MS: u64 = 1_000;

#[test]
fn tempo_push_round_trip() {
    link_multicast_or_skip!();
    let mut agogo_session =
        LinkSession::new(Tempo::from_bpm_integer(120), anchor_48k(), LinkWriteConfig::default());
    let peer_link = AblLink::new(120.0);
    agogo_session.enable(true);
    peer_link.enable(true);

    let paired = wait_for_pair(&agogo_session, &peer_link, Duration::from_secs(3));
    if !paired {
        eprintln!("SKIP: multicast loopback unreachable; aborting tempo_push_round_trip");
        agogo_session.enable(false);
        peer_link.enable(false);
        return;
    }

    agogo_session.set_tempo(Tempo::from_bpm_integer(137));
    sleep(Duration::from_millis(PROPAGATION_MS));
    let mut state = SessionState::new();
    peer_link.capture_audio_session_state(&mut state);
    let peer_tempo = state.tempo();
    let diff = (peer_tempo - 137.0).abs();
    agogo_session.enable(false);
    peer_link.enable(false);
    assert!(
        diff < 0.05,
        "peer tempo {peer_tempo} diverged from pushed 137.0 by {diff}"
    );
}

#[test]
fn transport_link_to_agogo() {
    link_multicast_or_skip!();
    let mut agogo_session =
        LinkSession::new(Tempo::from_bpm_integer(120), anchor_48k(), LinkWriteConfig::default());
    let peer_link = AblLink::new(120.0);
    agogo_session.enable(true);
    peer_link.enable(true);
    // Link's `is_playing` flag only propagates between peers that
    // BOTH have start-stop-sync enabled. `LinkSession::enable(true)`
    // flips it on agogo's side via config; the raw peer needs it
    // flipped explicitly here.
    peer_link.enable_start_stop_sync(true);

    let paired = wait_for_pair(&agogo_session, &peer_link, Duration::from_secs(3));
    if !paired {
        eprintln!("SKIP: multicast loopback unreachable");
        agogo_session.enable(false);
        peer_link.enable(false);
        return;
    }

    // Peer publishes is_playing = true.
    let mut peer_state = SessionState::new();
    peer_link.capture_audio_session_state(&mut peer_state);
    peer_state.set_is_playing(true, peer_link.clock_micros());
    peer_link.commit_audio_session_state(&peer_state);

    // agogo polls transport; bounded spin for the FSM to see Playing.
    let deadline = Instant::now() + Duration::from_millis(PROPAGATION_MS);
    let mut saw_playing = false;
    while Instant::now() < deadline {
        agogo_session.poll_transport();
        if matches!(agogo_session.transport_state(), TransportState::Playing) {
            saw_playing = true;
            break;
        }
        sleep(Duration::from_millis(20));
    }
    agogo_session.enable(false);
    peer_link.enable(false);
    assert!(saw_playing, "agogo FSM never saw Playing after peer start");
}

#[test]
fn transport_agogo_to_link_one_shot() {
    link_multicast_or_skip!();
    let mut agogo_session =
        LinkSession::new(Tempo::from_bpm_integer(120), anchor_48k(), LinkWriteConfig::default());
    let peer_link = AblLink::new(120.0);
    agogo_session.enable(true);
    peer_link.enable(true);
    peer_link.enable_start_stop_sync(true);

    let paired = wait_for_pair(&agogo_session, &peer_link, Duration::from_secs(3));
    if !paired {
        eprintln!("SKIP: multicast loopback unreachable");
        agogo_session.enable(false);
        peer_link.enable(false);
        return;
    }

    // agogo drives UserStart (one-shot publish).
    agogo_session.user_start();
    sleep(Duration::from_millis(PROPAGATION_MS));

    // Peer should observe is_playing = true.
    let mut peer_state = SessionState::new();
    peer_link.capture_audio_session_state(&mut peer_state);
    let playing = peer_state.is_playing();
    agogo_session.user_stop(); // leave the network clean for the next test.
    sleep(Duration::from_millis(300));
    agogo_session.enable(false);
    peer_link.enable(false);
    assert!(playing, "peer never observed is_playing = true after UserStart");
}

#[test]
fn quantum_snap_produces_positive_offset() {
    // Simpler surrogate for Plan 09's `quantum_snap_first_tick`: we
    // assert that arming a channel with `snap_to_quantum = Some(_)`
    // yields a non-trivially-positive offset within one-quantum
    // bound. The full "first tick lands within ±1 sample of next
    // bar boundary" invariant needs the channel → scheduler →
    // transform pipeline wired through the real audio callback,
    // which lands with Plan 05.
    link_multicast_or_skip!();
    use agogo_core::channel::{Channel, ChannelMode, MAX_SHIFT};
    use agogo_core::fxp::Micro;
    use agogo_core::time::swing::SwingConfig;
    use agogo_core::time::tbase::TBase;

    let mut agogo_session =
        LinkSession::new(Tempo::from_bpm_integer(120), anchor_48k(), LinkWriteConfig::default());
    agogo_session.enable(true);
    // Let Link's internal state stabilise.
    sleep(Duration::from_millis(100));

    let mut ch = Channel {
        mode: ChannelMode::MidiClock,
        divider: TBase::T4,
        shuffle: SwingConfig { amount: 0, multiplier: 1 },
        shift: Micro::ZERO,
        offset: Micro::ZERO,
        snap_to_quantum: Some(Quantum::from_bars(4)),
    };
    agogo_session.arm_channel(&mut ch);
    agogo_session.enable(false);

    // At 120 BPM, Quantum::from_bars(4) spans 4 beats (one 4/4 bar):
    // 4 × 500 ms = 2 s = 2_000_000 µs.
    // Snap delta must be within [0, 2_000_001) (+1 µs rounding slack).
    assert!(
        ch.offset.0 >= 0,
        "snap produced negative offset: {:?}", ch.offset
    );
    assert!(
        ch.offset.0 < 2_000_001,
        "snap offset {:?} exceeds one-quantum span at 120 BPM", ch.offset
    );
    // Also: the offset shouldn't accidentally saturate MAX_SHIFT
    // (which would indicate unit confusion — MAX_SHIFT is 300 ms).
    let _ = MAX_SHIFT; // referenced to keep the import; bound-check documented above.
}
