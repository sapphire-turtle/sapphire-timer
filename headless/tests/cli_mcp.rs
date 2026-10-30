use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_sapphire-timer")
}

/// `mcp` is the framework's: it serves the running server's workspace, so
/// the MVP's `--data-dir` is gone.
#[test]
fn mcp_is_the_frameworks_subcommand() {
    let out = Command::new(bin())
        .args(["mcp", "--help"])
        .output()
        .expect("run");
    assert!(out.status.success(), "mcp --help failed");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(!text.contains("--data-dir"), "got: {text}");
}
