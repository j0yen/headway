//! `headway` — sanctioned cloudbuild-only build primitive.
//!
//! Routes all Rust crate recompiles through `cloudbuild.sh` (Hetzner),
//! never local `cargo`. The single entry point is [`build`].

#![deny(unsafe_code)]
#![warn(missing_docs, unreachable_pub)]

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Instant,
};

/// Default path to `cloudbuild.sh`, overridden by `HEADWAY_CLOUDBUILD` env var.
pub const DEFAULT_CLOUDBUILD_PATH: &str =
    "~/.claude/skills/cloudbuild/cloudbuild.sh";

/// Plan computed before any subprocess is invoked.
///
/// Printed by `--dry-run` (the default posture).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildPlan {
    /// Absolute path to the crate directory.
    pub crate_dir: PathBuf,
    /// Crate name (derived from the directory basename or `Cargo.toml`).
    pub crate_name: String,
    /// Git HEAD SHA of the crate source (or `"unknown"` if not a git repo).
    pub source_head: String,
    /// Currently installed version of the binary, if any.
    pub installed_version: Option<String>,
    /// The exact `cloudbuild.sh build <name>` command that `--no-dry-run` would run.
    pub cloudbuild_command: String,
}

/// Whether a build was actually fresh (no-op) or the script was unreachable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    /// `cloudbuild.sh` ran and the artifact was produced.
    Built,
    /// `installed_version == source_head`; cloudbuild was not invoked.
    NoOpFresh,
    /// `cloudbuild.sh` was absent, SSH-unreachable, or returned the
    /// "unreachable" exit code (exit 1 before any build). Local cargo
    /// was NOT invoked.
    CloudbuildUnreachable,
    /// `cloudbuild.sh` was reachable but the build itself failed.
    BuildFailed,
}

/// Result returned after a `--no-dry-run` invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildVerdict {
    /// Crate name.
    pub crate_name: String,
    /// Path to the pulled artifact, if `status == Built`.
    pub artifact_path: Option<PathBuf>,
    /// Installed version before this build (mirrors `BuildPlan.installed_version`).
    pub version_before: Option<String>,
    /// Installed version after this build (re-probed from the installed binary).
    pub version_after: Option<String>,
    /// Raw exit code from `cloudbuild.sh`.
    pub cloudbuild_exit_code: Option<i32>,
    /// Elapsed wall-clock milliseconds for the subprocess call.
    pub elapsed_ms: u64,
    /// High-level verdict.
    pub status: Status,
}

/// Configuration for a [`build`] call.
#[derive(Debug, Clone)]
pub struct BuildConfig {
    /// Path to `cloudbuild.sh`. Defaults to `HEADWAY_CLOUDBUILD` env, then
    /// [`DEFAULT_CLOUDBUILD_PATH`].
    pub cloudbuild_path: PathBuf,
    /// If `true`, return immediately with `NoOpFresh` when
    /// `installed_version == source_head`.
    pub require_fresh: bool,
}

impl Default for BuildConfig {
    fn default() -> Self {
        Self {
            cloudbuild_path: resolve_cloudbuild_path(),
            require_fresh: true,
        }
    }
}

/// Resolve `HEADWAY_CLOUDBUILD` → default path, expanding `~`.
/// Public alias for use by the CLI binary.
#[must_use]
pub fn resolve_cloudbuild_path_pub() -> PathBuf {
    resolve_cloudbuild_path()
}

fn resolve_cloudbuild_path() -> PathBuf {
    let raw = std::env::var("HEADWAY_CLOUDBUILD")
        .unwrap_or_else(|_| DEFAULT_CLOUDBUILD_PATH.to_owned());
    expand_tilde(&raw)
}

/// Naively expand a leading `~/` to `$HOME/`.
fn expand_tilde(p: &str) -> PathBuf {
    if let Some(rest) = p.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(p)
}

/// Derive the crate name from `crate_dir/Cargo.toml` if possible,
/// otherwise fall back to the directory basename.
///
/// # Errors
///
/// Returns an error if the directory does not exist or has no readable basename.
pub fn derive_crate_name(crate_dir: &Path) -> Result<String> {
    // Try reading Cargo.toml [package].name
    let cargo_toml = crate_dir.join("Cargo.toml");
    if cargo_toml.exists() {
        if let Ok(contents) = std::fs::read_to_string(&cargo_toml) {
            for line in contents.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("name") {
                    // name = "foo" or name="foo"
                    if let Some(eq_pos) = rest.find('=') {
                        let value = rest[eq_pos + 1..].trim().trim_matches('"');
                        if !value.is_empty() {
                            return Ok(value.to_owned());
                        }
                    }
                }
            }
        }
    }
    // Fallback: directory basename
    crate_dir
        .file_name()
        .and_then(|n| n.to_str())
        .map(String::from)
        .context("crate_dir has no readable basename")
}

/// Read the git HEAD SHA for the crate's repository.
fn read_source_head(crate_dir: &Path) -> String {
    Command::new("git")
        .args(["-C", &crate_dir.to_string_lossy(), "rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_owned())
        .unwrap_or_else(|| "unknown".to_owned())
}

/// Probe the installed version of `crate_name` by running `<name> --version`.
fn probe_installed_version(crate_name: &str) -> Option<String> {
    let out = Command::new(crate_name)
        .arg("--version")
        .output()
        .ok()?;
    if out.status.success() {
        let line = String::from_utf8_lossy(&out.stdout);
        Some(line.trim().to_owned())
    } else {
        None
    }
}

/// Build a [`BuildPlan`] for the given crate directory.
///
/// # Errors
///
/// Returns an error if the crate directory doesn't exist or the name can't
/// be derived.
pub fn plan(crate_dir: &Path, cfg: &BuildConfig) -> Result<BuildPlan> {
    let crate_dir = crate_dir
        .canonicalize()
        .with_context(|| format!("crate_dir not found: {}", crate_dir.display()))?;
    let crate_name = derive_crate_name(&crate_dir)?;
    let source_head = read_source_head(&crate_dir);
    let installed_version = probe_installed_version(&crate_name);
    let script = cfg.cloudbuild_path.to_string_lossy();
    let cloudbuild_command = format!("bash {script} build {crate_name}");
    Ok(BuildPlan {
        crate_dir,
        crate_name,
        source_head,
        installed_version,
        cloudbuild_command,
    })
}

/// Execute a build according to `plan`, routing through `cloudbuild.sh`.
///
/// **No local `cargo` code path exists.** If `cloudbuild.sh` is absent or
/// unreachable, returns [`Status::CloudbuildUnreachable`] and exits non-zero.
///
/// # Errors
///
/// Returns an error only for truly unexpected I/O failures (e.g. couldn't
/// even spawn the subprocess). Cloudbuild failures are encoded in
/// [`BuildVerdict::status`].
pub fn build(build_plan: &BuildPlan, cfg: &BuildConfig) -> Result<BuildVerdict> {
    let version_before = build_plan.installed_version.clone();

    // No-op-fresh guard
    if cfg.require_fresh {
        if let Some(ref installed) = build_plan.installed_version {
            if installed.contains(&build_plan.source_head)
                || build_plan.source_head == "unknown"
            {
                // source_head is "unknown" only if not a git repo; treat
                // as potentially fresh to avoid spurious builds.
            } else if *installed == build_plan.source_head {
                return Ok(BuildVerdict {
                    crate_name: build_plan.crate_name.clone(),
                    artifact_path: None,
                    version_before,
                    version_after: Some(installed.clone()),
                    cloudbuild_exit_code: None,
                    elapsed_ms: 0,
                    status: Status::NoOpFresh,
                });
            }
        }
    }

    // Guard: cloudbuild.sh must exist
    let script = &cfg.cloudbuild_path;
    if !script.exists() {
        eprintln!(
            "headway: cloudbuild.sh not found at {}: returning cloudbuild-unreachable",
            script.display()
        );
        return Ok(BuildVerdict {
            crate_name: build_plan.crate_name.clone(),
            artifact_path: None,
            version_before,
            version_after: None,
            cloudbuild_exit_code: None,
            elapsed_ms: 0,
            status: Status::CloudbuildUnreachable,
        });
    }

    // Invoke cloudbuild.sh build <name>
    let t0 = Instant::now();
    let output = Command::new("bash")
        .arg(script)
        .arg("build")
        .arg(&build_plan.crate_name)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("failed to spawn cloudbuild.sh")?;
    let elapsed_ms = t0.elapsed().as_millis() as u64;

    let exit_code = output.status.code();
    let stderr_text = String::from_utf8_lossy(&output.stderr);

    // Exit code 2 means "SSH unreachable / Hetzner down" by cloudbuild.sh convention.
    // We also treat any non-zero exit that mentions "unreachable" as such.
    let is_unreachable = exit_code == Some(2)
        || (!output.status.success()
            && (stderr_text.contains("unreachable")
                || stderr_text.contains("Cannot reach")
                || stderr_text.contains("Connection refused")
                || stderr_text.contains("ssh: connect")));

    if is_unreachable {
        eprintln!(
            "headway: cloudbuild.sh reported unreachable (exit {:?}): {}",
            exit_code,
            stderr_text.trim()
        );
        return Ok(BuildVerdict {
            crate_name: build_plan.crate_name.clone(),
            artifact_path: None,
            version_before,
            version_after: None,
            cloudbuild_exit_code: exit_code,
            elapsed_ms,
            status: Status::CloudbuildUnreachable,
        });
    }

    if !output.status.success() {
        eprintln!(
            "headway: cloudbuild.sh build failed (exit {:?}):\n{}",
            exit_code,
            stderr_text.trim()
        );
        return Ok(BuildVerdict {
            crate_name: build_plan.crate_name.clone(),
            artifact_path: None,
            version_before,
            version_after: None,
            cloudbuild_exit_code: exit_code,
            elapsed_ms,
            status: Status::BuildFailed,
        });
    }

    // Build succeeded — re-probe installed version
    let version_after = probe_installed_version(&build_plan.crate_name);

    // Try to locate the artifact from cloudbuild stdout
    let stdout_text = String::from_utf8_lossy(&output.stdout);
    let artifact_path = extract_artifact_path(&stdout_text);

    Ok(BuildVerdict {
        crate_name: build_plan.crate_name.clone(),
        artifact_path,
        version_before,
        version_after,
        cloudbuild_exit_code: exit_code,
        elapsed_ms,
        status: Status::Built,
    })
}

/// Attempt to extract an artifact path from cloudbuild stdout.
/// Looks for lines like "pulled: /path/to/binary" or "artifact: /path".
fn extract_artifact_path(stdout: &str) -> Option<PathBuf> {
    for line in stdout.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed
            .strip_prefix("pulled:")
            .or_else(|| trimmed.strip_prefix("artifact:"))
        {
            let p = rest.trim();
            if !p.is_empty() {
                return Some(PathBuf::from(p));
            }
        }
    }
    None
}

/// Format a [`BuildPlan`] for table display.
#[must_use]
pub fn format_plan_table(plan: &BuildPlan) -> String {
    format!(
        "crate:             {}\ncrate_dir:         {}\nsource_head:       {}\ninstalled_version: {}\nwould_run:         {}",
        plan.crate_name,
        plan.crate_dir.display(),
        plan.source_head,
        plan.installed_version.as_deref().unwrap_or("(none)"),
        plan.cloudbuild_command,
    )
}

/// Format a [`BuildVerdict`] for table display.
#[must_use]
pub fn format_verdict_table(verdict: &BuildVerdict) -> String {
    format!(
        "crate:          {}\nstatus:         {:?}\nversion_before: {}\nversion_after:  {}\nelapsed_ms:     {}\nartifact:       {}",
        verdict.crate_name,
        verdict.status,
        verdict.version_before.as_deref().unwrap_or("(none)"),
        verdict.version_after.as_deref().unwrap_or("(none)"),
        verdict.elapsed_ms,
        verdict
            .artifact_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "(none)".to_owned()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;

    fn make_stub_cloudbuild(dir: &Path, exit_code: i32, stdout: &str) -> PathBuf {
        let script = dir.join("stub_cloudbuild.sh");
        let content = format!(
            "#!/usr/bin/env bash\necho \"{stdout}\"\nexit {exit_code}\n"
        );
        std::fs::write(&script, content).expect("write stub");
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
            .expect("chmod");
        script
    }

    fn make_fake_crate(dir: &Path, name: &str) -> PathBuf {
        let crate_dir = dir.join(name);
        std::fs::create_dir_all(&crate_dir).expect("mkdir");
        let cargo_toml = format!(
            "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"
        );
        std::fs::write(crate_dir.join("Cargo.toml"), cargo_toml).expect("write Cargo.toml");
        crate_dir
    }

    #[test]
    fn test_expand_tilde() {
        let home = std::env::var("HOME").expect("HOME");
        let expanded = expand_tilde("~/foo/bar");
        assert_eq!(expanded, PathBuf::from(&home).join("foo/bar"));
    }

    #[test]
    fn test_derive_crate_name_from_cargo_toml() {
        let tmp = TempDir::new().expect("tmpdir");
        let crate_dir = make_fake_crate(tmp.path(), "my-crate");
        let name = derive_crate_name(&crate_dir).expect("derive name");
        assert_eq!(name, "my-crate");
    }

    #[test]
    fn test_derive_crate_name_fallback_basename() {
        let tmp = TempDir::new().expect("tmpdir");
        let crate_dir = tmp.path().join("fallback-name");
        std::fs::create_dir_all(&crate_dir).expect("mkdir");
        let name = derive_crate_name(&crate_dir).expect("derive name");
        assert_eq!(name, "fallback-name");
    }

    #[test]
    fn test_extract_artifact_path_pulled() {
        let stdout = "building...\npulled: /home/jsy/.local/bin/mybin\ndone\n";
        let p = extract_artifact_path(stdout);
        assert_eq!(p, Some(PathBuf::from("/home/jsy/.local/bin/mybin")));
    }

    #[test]
    fn test_extract_artifact_path_none() {
        let stdout = "building...\ndone\n";
        assert!(extract_artifact_path(stdout).is_none());
    }

    #[test]
    fn test_build_cloudbuild_absent_returns_unreachable() {
        let tmp = TempDir::new().expect("tmpdir");
        let crate_dir = make_fake_crate(tmp.path(), "testcrate");
        let cfg = BuildConfig {
            cloudbuild_path: tmp.path().join("nonexistent_cloudbuild.sh"),
            require_fresh: false,
        };
        let bp = plan(&crate_dir, &cfg).expect("plan");
        let verdict = build(&bp, &cfg).expect("build call");
        assert_eq!(verdict.status, Status::CloudbuildUnreachable);
    }

    #[test]
    fn test_build_stub_success() {
        let tmp = TempDir::new().expect("tmpdir");
        let crate_dir = make_fake_crate(tmp.path(), "testcrate");
        let script = make_stub_cloudbuild(tmp.path(), 0, "pulled: /tmp/testcrate");
        let cfg = BuildConfig {
            cloudbuild_path: script,
            require_fresh: false,
        };
        let bp = plan(&crate_dir, &cfg).expect("plan");
        let verdict = build(&bp, &cfg).expect("build call");
        assert_eq!(verdict.status, Status::Built);
        assert_eq!(
            verdict.artifact_path,
            Some(PathBuf::from("/tmp/testcrate"))
        );
    }

    #[test]
    fn test_build_stub_build_failed() {
        let tmp = TempDir::new().expect("tmpdir");
        let crate_dir = make_fake_crate(tmp.path(), "testcrate");
        let script = make_stub_cloudbuild(tmp.path(), 1, "build error");
        let cfg = BuildConfig {
            cloudbuild_path: script,
            require_fresh: false,
        };
        let bp = plan(&crate_dir, &cfg).expect("plan");
        let verdict = build(&bp, &cfg).expect("build call");
        assert_eq!(verdict.status, Status::BuildFailed);
    }

    #[test]
    fn test_no_cargo_build_in_source() {
        // AC3: assert no local cargo invocation in src/
        let src_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        for entry in std::fs::read_dir(&src_dir).expect("read src/") {
            let entry = entry.expect("entry");
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                let content = std::fs::read_to_string(&path).expect("read file");
                // "cargo build" or "cargo install" must not appear in source
                assert!(
                    !content.contains("\"cargo\""),
                    "Found 'cargo' subprocess call in {}: local cargo is forbidden",
                    path.display()
                );
                assert!(
                    !content.contains("cargo build"),
                    "Found 'cargo build' in {}: local cargo is forbidden",
                    path.display()
                );
                assert!(
                    !content.contains("cargo install"),
                    "Found 'cargo install' in {}: local cargo is forbidden",
                    path.display()
                );
            }
        }
    }
}
