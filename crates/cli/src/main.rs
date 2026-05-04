#![forbid(unsafe_code)]

mod command;
mod parse;

fn main() {
    let cli = command::cli().run();
    if let Err(e) = command::dispatch(cli) {
        eprintln!("error: {e}");
        std::process::exit(2);
    }
}
