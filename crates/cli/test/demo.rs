use std::process::Command;

#[test]
fn demo_help_lists_demo_subcommands() {
    let output = Command::new(env!("CARGO_BIN_EXE_agogo"))
        .args(["demo", "--help"])
        .output()
        .expect("run agogo demo --help");

    assert!(
        output.status.success(),
        "agogo demo --help failed: status={:?}, stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("run"), "{stdout}");
    assert!(stdout.contains("list-audio-inputs"), "{stdout}");
    assert!(stdout.contains("list-midi-outputs"), "{stdout}");
}
