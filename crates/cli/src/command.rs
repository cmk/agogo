//! Top-level CLI parser and dispatcher.

use bpaf::Bpaf;

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
        #[bpaf(external(crate::trace::sync_sub))]
        sub: crate::trace::SyncSub,
    },
    /// Musical-time operations (Cirklon grid algebra).
    #[cfg(feature = "core")]
    #[bpaf(command("time"))]
    Time {
        #[bpaf(external(crate::time::time_op))]
        op: crate::time::TimeOp,
    },
    /// Per-channel scheduler utilities.
    #[cfg(feature = "core")]
    #[bpaf(command("channel"))]
    Channel {
        #[bpaf(external(crate::trace::channel_sub))]
        sub: crate::trace::ChannelSub,
    },
    /// MIDI output utilities.
    #[cfg(feature = "core")]
    #[bpaf(command("midi"))]
    Midi {
        #[bpaf(external(crate::trace::midi_sub))]
        sub: crate::trace::MidiSub,
    },
    /// Ableton Link integration utilities.
    #[cfg(feature = "link")]
    #[bpaf(command("link"))]
    Link {
        #[bpaf(external(crate::link::link_sub))]
        sub: crate::link::LinkSub,
    },
    /// End-to-end demo: cpal audio in -> PLL / Internal clock ->
    /// scheduler -> renderer -> SPSC drain -> midir MIDI out.
    #[cfg(feature = "demo")]
    #[bpaf(command("demo"))]
    Demo {
        #[bpaf(external(crate::demo::demo_sub))]
        sub: crate::demo::DemoSub,
    },
    /// Plan 14's end-to-end runner. N-channel Machine, six-rate
    /// dispatch, internal/external/link sources, Ctrl-C teardown.
    #[cfg(feature = "run")]
    #[bpaf(command("run"))]
    Run {
        #[bpaf(external(crate::run::run_args))]
        args: crate::run::RunArgs,
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
        Some(Command::Sync { sub }) => crate::trace::dispatch_sync(sub),
        #[cfg(feature = "core")]
        Some(Command::Time { op }) => crate::time::dispatch(op),
        #[cfg(feature = "core")]
        Some(Command::Channel { sub }) => crate::trace::dispatch_channel(sub),
        #[cfg(feature = "core")]
        Some(Command::Midi { sub }) => crate::trace::dispatch_midi(sub),
        #[cfg(feature = "link")]
        Some(Command::Link { sub }) => {
            crate::link::dispatch(sub);
            Ok(())
        }
        #[cfg(feature = "demo")]
        Some(Command::Demo { sub }) => crate::demo::dispatch(sub),
        #[cfg(feature = "run")]
        Some(Command::Run { args }) => crate::run::run(&args),
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
