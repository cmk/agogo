use std::process::Command;

#[test]
fn link_help_lists_link_subcommands() {
    let output = Command::new(env!("CARGO_BIN_EXE_agogo"))
        .args(["link", "--help"])
        .output()
        .expect("run agogo link --help");

    assert!(
        output.status.success(),
        "agogo link --help failed: status={:?}, stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("probe"), "{stdout}");
    assert!(stdout.contains("push-tempo"), "{stdout}");
    assert!(stdout.contains("transport"), "{stdout}");
    assert!(stdout.contains("diag"), "{stdout}");
}
