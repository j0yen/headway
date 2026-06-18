//! AC7: `headway --version`, `headway build --help`, `headway verify --help`,
//! and `headway run --help` work on the built binary.
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

#[test]
fn ac7_verify_help_works() {
    let output = std::process::Command::new(headway_bin())
        .arg("verify")
        .arg("--help")
        .output()
        .expect("run headway verify --help");

    assert!(
        output.status.success(),
        "headway verify --help exited {:?}",
        output.status.code()
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("daemon") || stdout.contains("DAEMON"),
        "verify help should mention daemon argument: {stdout}"
    );
}

#[test]
fn ac7_run_help_works() {
    let output = std::process::Command::new(headway_bin())
        .arg("run")
        .arg("--help")
        .output()
        .expect("run headway run --help");

    assert!(
        output.status.success(),
        "headway run --help exited {:?}",
        output.status.code()
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("daemon") || stdout.contains("DAEMON"),
        "run help should mention daemon argument: {stdout}"
    );
}
