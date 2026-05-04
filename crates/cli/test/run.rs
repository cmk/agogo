use std::process::Command;

#[test]
fn run_help_lists_runtime_flags() {
    let output = Command::new(env!("CARGO_BIN_EXE_agogo"))
        .args(["run", "--help"])
        .output()
        .expect("run agogo run --help");

    assert!(
        output.status.success(),
        "agogo run --help failed: status={:?}, stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--bpm"), "{stdout}");
    assert!(stdout.contains("--sr"), "{stdout}");
    assert!(stdout.contains("--ch"), "{stdout}");
    assert!(stdout.contains("--source"), "{stdout}");
}
