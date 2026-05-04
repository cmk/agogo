//! Ableton Link CLI commands.

mod commands;
mod probe;

use bpaf::Bpaf;

use crate::parse::{parse_bpm_to_tempo, parse_positive_u32, parse_quantum_from_beats};

#[derive(Debug, Clone, Bpaf)]
pub enum LinkSub {
    /// Probe a live Ableton Link session: emit CSV
    /// `t_ms,peers,tempo_bpm,phase` at a chosen period for a chosen
    /// duration.
    #[bpaf(command("probe"))]
    Probe {
        /// Tempo to initialise Link with (BPM).
        #[bpaf(long, argument::<String>("BPM"), parse(parse_bpm_to_tempo), fallback(agogo::chan::conn::tempo::Tempo::from_bpm_integer(120)))]
        initial_bpm: agogo::chan::conn::tempo::Tempo,
        /// Sample rate for the sample-index <-> host-time mapping.
        #[bpaf(long, argument("SR"), parse(parse_positive_u32), fallback(48_000))]
        sr: u32,
        /// Total probe duration in ms.
        #[bpaf(
            long,
            argument("DURATION_MS"),
            parse(parse_positive_u32),
            fallback(3_000)
        )]
        duration_ms: u32,
        /// Sampling period in ms.
        #[bpaf(long, argument("PERIOD_MS"), parse(parse_positive_u32), fallback(100))]
        period_ms: u32,
    },
    /// One-shot tempo push: connect to the Link network, set the
    /// session tempo, wait briefly so peers can capture the
    /// committed state, then exit.
    #[bpaf(command("push-tempo"))]
    PushTempo {
        /// New tempo in BPM.
        #[bpaf(long, argument::<String>("BPM"), parse(parse_bpm_to_tempo))]
        bpm: agogo::chan::conn::tempo::Tempo,
        /// How long to keep the network session alive after
        /// committing, so peers see the change.
        #[bpaf(long, argument("SETTLE_MS"), parse(parse_positive_u32), fallback(200))]
        settle_ms: u32,
    },
    /// Headless transport FSM runner: subscribes to Link's
    /// `is_playing`, optionally publishes one-shot `UserStart` /
    /// `UserStop`, and prints transport-state transitions to stdout.
    #[bpaf(command("transport"))]
    Transport {
        /// Initial BPM.
        #[bpaf(long, argument::<String>("BPM"), parse(parse_bpm_to_tempo), fallback(agogo::chan::conn::tempo::Tempo::from_bpm_integer(120)))]
        bpm: agogo::chan::conn::tempo::Tempo,
        /// Quantum in bars.
        #[bpaf(long, argument::<String>("QUANTUM"), parse(parse_quantum_from_beats), fallback(agogo::host::link::Quantum::from_bars(4)))]
        quantum: agogo::host::link::Quantum,
        /// Sample rate.
        #[bpaf(long, argument("SR"), parse(parse_positive_u32), fallback(48_000))]
        sr: u32,
        /// Total run duration, ms.
        #[bpaf(
            long,
            argument("DURATION_MS"),
            parse(parse_positive_u32),
            fallback(5_000)
        )]
        duration_ms: u32,
        /// Drive a `UserStart` at session start.
        #[bpaf(long)]
        start: bool,
        /// Drive a `UserStop` just before exit.
        #[bpaf(long)]
        stop_on_exit: bool,
    },
    /// Print a single line summary of the current Link session
    /// state: `peers,tempo_bpm,is_playing`. Useful from scripts.
    #[bpaf(command("diag"))]
    Diag {
        #[bpaf(long, argument::<String>("BPM"), parse(parse_bpm_to_tempo), fallback(agogo::chan::conn::tempo::Tempo::from_bpm_integer(120)))]
        bpm: agogo::chan::conn::tempo::Tempo,
        #[bpaf(long, argument("SR"), parse(parse_positive_u32), fallback(48_000))]
        sr: u32,
        /// How long to join the network before reading state. Too
        /// short and `peers` under-reports.
        #[bpaf(long, argument("SETTLE_MS"), parse(parse_positive_u32), fallback(500))]
        settle_ms: u32,
    },
}

pub fn dispatch(sub: LinkSub) {
    match sub {
        LinkSub::Probe {
            initial_bpm,
            sr,
            duration_ms,
            period_ms,
        } => {
            probe::print_csv(initial_bpm, sr, duration_ms, period_ms);
        }
        LinkSub::PushTempo { bpm, settle_ms } => {
            commands::push_tempo(bpm, settle_ms);
        }
        LinkSub::Transport {
            bpm,
            quantum,
            sr,
            duration_ms,
            start,
            stop_on_exit,
        } => {
            commands::transport(bpm, quantum, sr, duration_ms, start, stop_on_exit);
        }
        LinkSub::Diag { bpm, sr, settle_ms } => {
            commands::diag(bpm, sr, settle_ms);
        }
    }
}
