#![forbid(unsafe_code)]

fn main() {
    #[cfg(feature = "core")]
    let tag = "with core";
    #[cfg(not(feature = "core"))]
    let tag = "core disabled";

    println!("agogo-cli ({tag})");
}
