//! AC3: There is no code path that runs `cargo build`/`cargo install` locally.
//! A grep in the test suite asserts the crate contains no local-cargo invocation
//! in the build path; the only compiler invocation is via the cloudbuild subprocess.

use std::path::Path;

fn scan_for_forbidden_patterns(src_dir: &Path) {
    let forbidden = [
        r#""cargo""#,
        "cargo build",
        "cargo install",
        "cargo run",
        "Command::new(\"cargo\")",
    ];

    let walker = std::fs::read_dir(src_dir).expect("read src/");
    for entry in walker {
        let entry = entry.expect("entry");
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            let content = std::fs::read_to_string(&path)
                .unwrap_or_default();
            for pat in &forbidden {
                assert!(
                    !content.contains(pat),
                    "Found forbidden local-cargo pattern {:?} in {}.\n\
                     headway must NEVER invoke local cargo; all builds route through cloudbuild.sh.",
                    pat,
                    path.display()
                );
            }
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
fn ac3_no_local_cargo_in_bin() {
    // Also check main.rs if it's outside src/ for any reason
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    // scan the root for any stray .rs files
    if let Ok(entries) = std::fs::read_dir(manifest_dir) {
        let forbidden = [r#""cargo""#, "cargo build", "cargo install"];
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                let content = std::fs::read_to_string(&path).unwrap_or_default();
                for pat in &forbidden {
                    assert!(
                        !content.contains(pat),
                        "Found forbidden local-cargo pattern {:?} in {}",
                        pat,
                        path.display()
                    );
                }
            }
        }
    }
}
