//! AC2: With `--no-dry-run`, `build` invokes `cloudbuild.sh build <name>` as a
//! subprocess and returns a `BuildVerdict` whose `cloudbuild_status` reflects
//! the subprocess outcome. Tested with a stub cloudbuild script via
//! `HEADWAY_CLOUDBUILD` pointing at a fixture.

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use tempfile::TempDir;

fn make_fake_crate(dir: &std::path::Path, name: &str) -> PathBuf {
    let crate_dir = dir.join(name);
    std::fs::create_dir_all(&crate_dir).expect("mkdir");
    let cargo_toml = format!(
        "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"
    );
    std::fs::write(crate_dir.join("Cargo.toml"), cargo_toml).expect("Cargo.toml");
    crate_dir
}

fn headway_bin() -> PathBuf {
    let mut p = std::env::current_exe().expect("current_exe");
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    p.join("headway")
}

fn make_stub(dir: &std::path::Path, exit_code: i32, stdout_msg: &str) -> PathBuf {
    let script = dir.join("stub.sh");
    let content = format!("#!/bin/bash\necho \"{stdout_msg}\"\nexit {exit_code}\n");
    std::fs::write(&script, &content).expect("write stub");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
        .expect("chmod");
    script
}

#[test]
fn ac2_no_dry_run_invokes_cloudbuild_and_returns_verdict_built() {
    let tmp = TempDir::new().expect("tmpdir");
    let crate_dir = make_fake_crate(tmp.path(), "stubcrate");
    let stub = make_stub(tmp.path(), 0, "pulled: /tmp/stubcrate");

    let output = std::process::Command::new(headway_bin())
        .arg("build")
        .arg(&crate_dir)
        .arg("--no-dry-run")
        .arg("--format")
        .arg("json")
        .arg("--no-require-fresh")
        .env("HEADWAY_CLOUDBUILD", &stub)
        .output()
        .expect("run headway");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout is valid JSON");

    assert_eq!(
        json["status"].as_str(),
        Some("built"),
        "status should be 'built'; full output: {stdout}"
    );
    assert_eq!(
        json["crate_name"].as_str(),
        Some("stubcrate"),
        "crate_name in verdict"
    );
}

#[test]
fn ac2_no_dry_run_build_failure_reflected_in_verdict() {
    let tmp = TempDir::new().expect("tmpdir");
    let crate_dir = make_fake_crate(tmp.path(), "failcrate");
    let stub = make_stub(tmp.path(), 1, "build failed");

    let output = std::process::Command::new(headway_bin())
        .arg("build")
        .arg(&crate_dir)
        .arg("--no-dry-run")
        .arg("--format")
        .arg("json")
        .arg("--no-require-fresh")
        .env("HEADWAY_CLOUDBUILD", &stub)
        .output()
        .expect("run headway");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout is valid JSON");

    assert_eq!(
        json["status"].as_str(),
        Some("build-failed"),
        "status should be 'build-failed'; full output: {stdout}"
    );
    // Exit code should be non-zero
    assert!(
        !output.status.success(),
        "process should exit non-zero on build-failed"
    );
}
