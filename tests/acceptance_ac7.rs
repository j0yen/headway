//! AC7: `headway --version` and `headway build --help` work on the built binary;
//! `cargo test` is green and `clippy` produces no new warnings.
//! (The latter two are enforced by the harness; this test covers the binary invocations.)

use std::path::PathBuf;

fn headway_bin() -> PathBuf {
    let mut p = std::env::current_exe().expect("current_exe");
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    p.join("headway")
}

#[test]
fn ac7_version_flag_works() {
    let output = std::process::Command::new(headway_bin())
        .arg("--version")
        .output()
        .expect("run headway --version");

    assert!(
        output.status.success(),
        "headway --version exited {:?}",
        output.status.code()
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("headway"),
        "version output should contain 'headway': {stdout}"
    );
}

#[test]
fn ac7_build_help_works() {
    let output = std::process::Command::new(headway_bin())
        .arg("build")
        .arg("--help")
        .output()
        .expect("run headway build --help");

    assert!(
        output.status.success(),
        "headway build --help exited {:?}",
        output.status.code()
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Should mention crate-dir and dry-run
    assert!(
        stdout.contains("crate-dir") || stdout.contains("CRATE_DIR"),
        "help should mention crate-dir argument: {stdout}"
    );
}
