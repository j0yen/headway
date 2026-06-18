//! AC5: When `installed_version == source_head` (already fresh), `build` returns
//! `status: no-op-fresh` without invoking cloudbuild, unless `--no-require-fresh`
//! is passed.

use headway::{build, plan, BuildConfig, Status};
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
    // Init git so we can read HEAD
    std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(&crate_dir)
        .status()
        .expect("git init");
    std::process::Command::new("git")
        .args(["commit", "--allow-empty", "-m", "init"])
        .current_dir(&crate_dir)
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@test")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@test")
        .status()
        .expect("git commit");
    crate_dir
}

fn make_fail_stub(dir: &std::path::Path) -> PathBuf {
    let script = dir.join("fail_stub.sh");
    std::fs::write(
        &script,
        "#!/bin/bash\necho 'SHOULD NOT BE CALLED' >&2\nexit 99\n",
    )
    .expect("write fail stub");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
        .expect("chmod");
    script
}

#[test]
fn ac5_noop_fresh_when_versions_match() {
    let tmp = TempDir::new().expect("tmpdir");
    let crate_dir = make_fake_crate(tmp.path(), "freshcrate");
    let fail_stub = make_fail_stub(tmp.path());

    // Read the real HEAD SHA
    let head = {
        let out = std::process::Command::new("git")
            .args(["-C", &crate_dir.to_string_lossy(), "rev-parse", "HEAD"])
            .output()
            .expect("git rev-parse");
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    };

    let cfg = BuildConfig {
        cloudbuild_path: fail_stub.clone(),
        require_fresh: true,
    };

    let mut bp = plan(&crate_dir, &cfg).expect("plan");
    // Simulate: installed_version equals source_head
    bp.installed_version = Some(head.clone());
    bp.source_head = head;

    let verdict = build(&bp, &cfg).expect("build");
    assert_eq!(
        verdict.status,
        Status::NoOpFresh,
        "should be no-op-fresh when versions match"
    );
    assert_eq!(
        verdict.elapsed_ms, 0,
        "no cloudbuild invoked means elapsed_ms=0"
    );
}

#[test]
fn ac5_no_require_fresh_invokes_cloudbuild() {
    let tmp = TempDir::new().expect("tmpdir");
    let crate_dir = make_fake_crate(tmp.path(), "freshcrate2");

    // A success stub
    let script = tmp.path().join("success_stub.sh");
    std::fs::write(
        &script,
        "#!/bin/bash\necho 'pulled: /tmp/freshcrate2'\nexit 0\n",
    )
    .expect("write stub");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
        .expect("chmod");

    let head = {
        let out = std::process::Command::new("git")
            .args(["-C", &crate_dir.to_string_lossy(), "rev-parse", "HEAD"])
            .output()
            .expect("git rev-parse");
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    };

    let cfg = BuildConfig {
        cloudbuild_path: script,
        require_fresh: false, // --no-require-fresh
    };

    let mut bp = plan(&crate_dir, &cfg).expect("plan");
    // Even if versions match, require_fresh=false means we build anyway
    bp.installed_version = Some(head.clone());
    bp.source_head = head;

    let verdict = build(&bp, &cfg).expect("build");
    assert_eq!(
        verdict.status,
        Status::Built,
        "should be Built when require_fresh=false, even if version matches"
    );
}
