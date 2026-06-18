//! AC4: When the configured cloudbuild script is absent or exits with the
//! unreachable code, `build` returns `status: cloudbuild-unreachable`, emits
//! a structured error to stderr, and exits non-zero — never silently builds local.

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

#[test]
fn ac4_absent_cloudbuild_returns_unreachable_nonzero() {
    let tmp = TempDir::new().expect("tmpdir");
    let crate_dir = make_fake_crate(tmp.path(), "unreachcrate");
    let absent = tmp.path().join("does_not_exist.sh");

    let output = std::process::Command::new(headway_bin())
        .arg("build")
        .arg(&crate_dir)
        .arg("--no-dry-run")
        .arg("--format")
        .arg("json")
        .arg("--no-require-fresh")
        .env("HEADWAY_CLOUDBUILD", &absent)
        .output()
        .expect("run headway");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout is valid JSON");

    assert_eq!(
        json["status"].as_str(),
        Some("cloudbuild-unreachable"),
        "status should be cloudbuild-unreachable; stdout: {stdout}"
    );
    assert!(
        !output.status.success(),
        "process must exit non-zero for cloudbuild-unreachable"
    );
}

#[test]
fn ac4_unreachable_exit_code_2_from_stub() {
    let tmp = TempDir::new().expect("tmpdir");
    let crate_dir = make_fake_crate(tmp.path(), "unreachcrate2");

    // exit code 2 = cloudbuild.sh's "unreachable" convention
    let script = tmp.path().join("stub_exit2.sh");
    std::fs::write(
        &script,
        "#!/bin/bash\necho 'unreachable: cannot reach Hetzner' >&2\nexit 2\n",
    )
    .expect("write stub");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
        .expect("chmod");

    let output = std::process::Command::new(headway_bin())
        .arg("build")
        .arg(&crate_dir)
        .arg("--no-dry-run")
        .arg("--format")
        .arg("json")
        .arg("--no-require-fresh")
        .env("HEADWAY_CLOUDBUILD", &script)
        .output()
        .expect("run headway");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout is valid JSON");

    assert_eq!(
        json["status"].as_str(),
        Some("cloudbuild-unreachable"),
        "exit code 2 should map to cloudbuild-unreachable"
    );
    assert!(!output.status.success(), "must exit non-zero");
}

#[test]
fn ac4_no_local_cargo_fallback_on_unreachable() {
    // This test verifies the hard contract: when cloudbuild is absent,
    // we do NOT fall back to local cargo. The verdict must be unreachable,
    // not built.
    let tmp = TempDir::new().expect("tmpdir");
    let crate_dir = make_fake_crate(tmp.path(), "nofallback");

    let output = std::process::Command::new(headway_bin())
        .arg("build")
        .arg(&crate_dir)
        .arg("--no-dry-run")
        .arg("--format")
        .arg("json")
        .arg("--no-require-fresh")
        .env("HEADWAY_CLOUDBUILD", "/nonexistent/cloudbuild.sh")
        .output()
        .expect("run headway");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout is valid JSON");

    // Must be unreachable — never "built" via local fallback
    assert_ne!(
        json["status"].as_str(),
        Some("built"),
        "must not fallback to local build"
    );
    assert_eq!(
        json["status"].as_str(),
        Some("cloudbuild-unreachable"),
        "must be cloudbuild-unreachable"
    );
}
