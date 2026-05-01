#![forbid(unsafe_code)]

mod command;
mod parsers;

#[cfg(feature = "core")]
mod time;

#[cfg(feature = "core")]
mod trace;

#[cfg(feature = "link")]
mod link;

#[cfg(feature = "demo")]
mod demo;

#[cfg(feature = "run")]
mod run;

fn main() {
    let cli = command::cli().run();
    if let Err(e) = command::dispatch(cli) {
        eprintln!("error: {e}");
        std::process::exit(2);
    }
}
