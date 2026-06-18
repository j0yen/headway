//! AC1: `headway build <crate-dir> --dry-run --format json` prints a BuildPlan
//! plus the exact `cloudbuild.sh build <name>` command line that would run,
//! mutates nothing, and exits 0. Default posture is dry-run.

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use tempfile::TempDir;

fn make_fake_crate(dir: &std::path::Path, name: &str) -> PathBuf {
    let crate_dir = dir.join(name);
    std::fs::create_dir_all(&crate_dir).expect("mkdir");
    let cargo_toml = format!(
        "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"
    );
    std::fs::write(crate_dir.join("Cargo.toml"), cargo_toml).expect("write Cargo.toml");
    crate_dir
}

fn headway_bin() -> PathBuf {
    // Use the built binary from cargo test environment
    let mut p = std::env::current_exe().expect("current_exe");
    // Strip test binary name and go up to deps/
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    p.join("headway")
}

#[test]
fn ac1_dry_run_json_exits_zero_and_prints_plan() {
    let tmp = TempDir::new().expect("tmpdir");
    let crate_dir = make_fake_crate(tmp.path(), "my-crate");

    // Make a dummy cloudbuild script (won't be called in dry-run)
    let stub = tmp.path().join("stub.sh");
    std::fs::write(&stub, "#!/bin/bash\nexit 0\n").expect("write stub");
    std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755))
        .expect("chmod");

    let output = std::process::Command::new(headway_bin())
        .arg("build")
        .arg(&crate_dir)
        .arg("--dry-run")
        .arg("--format")
        .arg("json")
        .env("HEADWAY_CLOUDBUILD", &stub)
        .output()
        .expect("run headway");

    assert!(
        output.status.success(),
        "expected exit 0, got {:?}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Must parse as JSON containing a BuildPlan shape
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout is valid JSON");

    assert_eq!(
        json["crate_name"].as_str(),
        Some("my-crate"),
        "crate_name in plan"
    );
    assert!(
        json["cloudbuild_command"].as_str().is_some(),
        "cloudbuild_command field present"
    );
    let cmd = json["cloudbuild_command"].as_str().expect("cmd str");
    assert!(
        cmd.contains("my-crate"),
        "cloudbuild_command contains crate name: {cmd}"
    );
    assert!(
        cmd.contains("build"),
        "cloudbuild_command contains 'build': {cmd}"
    );
}

#[test]
fn ac1_default_is_dry_run_no_invocation() {
    // Without --no-dry-run, the command should be dry-run by default
    // (cloudbuild.sh should NOT be called even if it would error)
    let tmp = TempDir::new().expect("tmpdir");
    let crate_dir = make_fake_crate(tmp.path(), "drytest");

    // Point to a script that exits non-zero so we'd detect if it were called
    let fail_stub = tmp.path().join("fail_stub.sh");
    std::fs::write(
        &fail_stub,
        "#!/bin/bash\necho 'SHOULD NOT BE CALLED' >&2\nexit 42\n",
    )
    .expect("write fail stub");
    std::fs::set_permissions(&fail_stub, std::fs::Permissions::from_mode(0o755))
        .expect("chmod");

    let output = std::process::Command::new(headway_bin())
        .arg("build")
        .arg(&crate_dir)
        // NO --no-dry-run flag — default should be dry-run
        .env("HEADWAY_CLOUDBUILD", &fail_stub)
        .output()
        .expect("run headway");

    assert!(
        output.status.success(),
        "default dry-run should exit 0\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("SHOULD NOT BE CALLED"),
        "cloudbuild.sh must not be invoked in dry-run mode"
    );
}
