//! layer: command
//! depends-on: parse
//!
//! Top-level CLI parser and dispatcher.

use bpaf::Bpaf;

#[cfg(feature = "core")]
use agogo::core::channel::ChannelCommon;
#[cfg(feature = "core")]
use agogo::core::conn::fixed::Micro;
#[cfg(feature = "core")]
use agogo::core::time::grid::Grid;
#[cfg(feature = "core")]
use agogo::core::time::swing::SwingConfig;
#[cfg(feature = "core")]
use agogo::core::time::tbase::TBase;

#[cfg(feature = "core")]
mod channel;
#[cfg(feature = "demo")]
mod demo;
#[cfg(feature = "link")]
mod link;
#[cfg(feature = "core")]
mod midi;
#[cfg(feature = "run")]
mod run;
#[cfg(feature = "core")]
mod sync;
#[cfg(feature = "core")]
mod time;

/// agogo workspace CLI
#[derive(Debug, Clone, Bpaf)]
#[bpaf(options, version)]
#[cfg(any(feature = "core", feature = "link", feature = "demo", feature = "run"))]
pub struct Cli {
    #[bpaf(external(command), optional)]
    command: Option<Command>,
}

#[derive(Debug, Clone, Bpaf)]
#[bpaf(options, version)]
#[cfg(not(any(feature = "core", feature = "link", feature = "demo", feature = "run")))]
pub struct Cli {}

#[derive(Debug, Clone, Bpaf)]
#[cfg(any(feature = "core", feature = "link", feature = "demo", feature = "run"))]
enum Command {
    /// Audio-clock sync utilities.
    #[cfg(feature = "core")]
    #[bpaf(command("sync"))]
    Sync {
        #[bpaf(external(sync::sync_sub))]
        sub: sync::SyncSub,
    },
    /// Musical-time operations (Recologic grid algebra).
    #[cfg(feature = "core")]
    #[bpaf(command("time"))]
    Time {
        #[bpaf(external(time::time_op))]
        op: time::TimeOp,
    },
    /// Per-channel scheduler utilities.
    #[cfg(feature = "core")]
    #[bpaf(command("channel"))]
    Channel {
        #[bpaf(external(channel::channel_sub))]
        sub: channel::ChannelSub,
    },
    /// MIDI output utilities.
    #[cfg(feature = "core")]
    #[bpaf(command("midi"))]
    Midi {
        #[bpaf(external(midi::midi_sub))]
        sub: midi::MidiSub,
    },
    /// Ableton Link integration utilities.
    #[cfg(feature = "link")]
    #[bpaf(command("link"))]
    Link {
        #[bpaf(external(link::link_sub))]
        sub: link::LinkSub,
    },
    /// End-to-end demo: cpal audio in -> PLL / Internal clock ->
    /// scheduler -> renderer -> SPSC drain -> midir MIDI out.
    #[cfg(feature = "demo")]
    #[bpaf(command("demo"))]
    Demo {
        #[bpaf(external(demo::demo_sub))]
        sub: demo::DemoSub,
    },
    /// End-to-end runner. N-channel Playhead, six-rate dispatch,
    /// internal/external/link sources, Ctrl-C teardown.
    #[cfg(feature = "run")]
    #[bpaf(command("run"))]
    Run {
        #[bpaf(external(run::run_args))]
        args: run::RunArgs,
    },
}

pub fn dispatch(cli: Cli) -> Result<(), String> {
    #[cfg(not(any(feature = "core", feature = "link", feature = "demo", feature = "run")))]
    {
        let _ = cli;
        println!("agogo-cli (core disabled)");
        Ok(())
    }

    #[cfg(any(feature = "core", feature = "link", feature = "demo", feature = "run"))]
    match cli.command {
        #[cfg(feature = "core")]
        Some(Command::Sync { sub }) => sync::dispatch(sub),
        #[cfg(feature = "core")]
        Some(Command::Time { op }) => time::dispatch(op),
        #[cfg(feature = "core")]
        Some(Command::Channel { sub }) => channel::dispatch(sub),
        #[cfg(feature = "core")]
        Some(Command::Midi { sub }) => midi::dispatch(sub),
        #[cfg(feature = "link")]
        Some(Command::Link { sub }) => {
            link::dispatch(sub);
            Ok(())
        }
        #[cfg(feature = "demo")]
        Some(Command::Demo { sub }) => demo::dispatch(sub),
        #[cfg(feature = "run")]
        Some(Command::Run { args }) => run::run(&args),
        None => {
            #[cfg(feature = "core")]
            let tag = "with core";
            #[cfg(not(feature = "core"))]
            let tag = "core disabled";
            println!("agogo-cli ({tag})");
            Ok(())
        }
    }
}

#[cfg(feature = "core")]
fn parse_grid_arg(grid: &str) -> Result<Grid, String> {
    grid.parse()
        .map_err(|e| format!("invalid --grid {grid}: {e}"))
}

#[cfg(feature = "core")]
fn validate_audio_rate(sr: u32) -> Result<(), String> {
    match sr {
        44_100 | 48_000 | 88_200 | 96_000 | 176_400 | 192_000 => Ok(()),
        _ => Err(format!(
            "--sr {sr} unsupported; expected one of 44_100 / 48_000 / 88_200 / 96_000 / 176_400 / 192_000"
        )),
    }
}

#[cfg(feature = "core")]
fn straight_common(divider: Grid, delay: Micro) -> ChannelCommon {
    ChannelCommon {
        divider,
        shuffle: SwingConfig {
            resolution: TBase::T16,
            amount: 0,
        },
        delay,
        offset: Micro::ZERO,
        bar_multiplier: None,
    }
}

#[cfg(feature = "core")]
fn checked_trace_frames(frames: usize, buffers: u32) -> Result<u64, String> {
    let frames_u64 =
        u64::try_from(frames).map_err(|_| format!("trace range exceeds u64: --frames {frames}"))?;
    let _ = u64::from(buffers).checked_mul(frames_u64).ok_or_else(|| {
        format!("trace range exceeds u64: --frames {frames} × --buffers {buffers}")
    })?;
    Ok(frames_u64)
}
