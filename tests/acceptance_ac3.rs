//! AC3: There is no code path that runs `cargo build`/`cargo install` locally.
//! A grep in the test suite asserts the crate contains no local-cargo invocation
//! in the build path; the only compiler invocation is via the cloudbuild subprocess.

use std::path::Path;

fn scan_for_forbidden_patterns(src_dir: &Path) {
    // Build the forbidden pattern at runtime so this test file cannot
    // self-match when src/ is scanned. The pattern is the subprocess-spawn
    // invocation for the cargo binary.
    let forbidden_spawn = format!("Command::new({})", "\"cargo\"");

    let walker = std::fs::read_dir(src_dir).expect("read src/");
    for entry in walker {
        let entry = entry.expect("entry");
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            let content = std::fs::read_to_string(&path)
                .unwrap_or_default();
            assert!(
                !content.contains(&forbidden_spawn),
                "Found forbidden local-cargo spawn in {}.\n\
                 headway must NEVER spawn local cargo; all builds route through cloudbuild.sh.",
                path.display()
            );
        }
    }
}

#[test]
fn ac3_no_local_cargo_in_src() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let src = manifest_dir.join("src");
    scan_for_forbidden_patterns(&src);
}

#[test]
fn ac3_no_local_cargo_in_root_rs_files() {
    // Also scan any .rs files at the repo root (e.g. build.rs)
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let forbidden_spawn = format!("Command::new({})", "\"cargo\"");
    if let Ok(entries) = std::fs::read_dir(manifest_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                let content = std::fs::read_to_string(&path).unwrap_or_default();
                assert!(
                    !content.contains(&forbidden_spawn),
                    "Found forbidden local-cargo spawn in {}",
                    path.display()
                );
            }
        }
    }
}
