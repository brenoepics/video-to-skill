//! Workspace guardrails: no Rust source file may exceed the line limit.

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};
use vts_extract::guardrails::{files_over_limit, MAX_RS_LINES};

fn fixture_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("vts-guardrails-tests")
        .join(format!("{name}-{}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_lines(path: &Path, lines: usize) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, "// filler\n".repeat(lines)).unwrap();
}

#[test]
fn reports_rs_file_over_the_limit_with_its_line_count() {
    let dir = fixture_dir("over");
    write_lines(&dir.join("big.rs"), 301);

    let offenders = files_over_limit(&dir, 300);

    assert_eq!(offenders.len(), 1);
    assert!(offenders[0].0.ends_with("big.rs"));
    assert_eq!(offenders[0].1, 301);
}

#[test]
fn accepts_rs_file_exactly_at_the_limit() {
    let dir = fixture_dir("at-limit");
    write_lines(&dir.join("ok.rs"), 300);

    assert!(files_over_limit(&dir, 300).is_empty());
}

#[test]
fn ignores_target_directories_and_non_rust_files() {
    let dir = fixture_dir("ignored");
    write_lines(&dir.join("target/debug/gen.rs"), 500);
    write_lines(&dir.join("notes.md"), 500);
    write_lines(&dir.join("nested/src/lib.rs"), 100);

    assert!(files_over_limit(&dir, 300).is_empty());
}

#[test]
fn workspace_rs_files_stay_within_the_limit() {
    // CARGO_MANIFEST_DIR = crates/extractor → workspace root is two levels up.
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .to_path_buf();

    let offenders = files_over_limit(&root, MAX_RS_LINES);

    assert!(
        offenders.is_empty(),
        "source files exceed the {MAX_RS_LINES}-line limit — split them by responsibility:\n{}",
        offenders
            .iter()
            .map(|(p, n)| format!("  {} ({n} lines)", p.display()))
            .collect::<Vec<_>>()
            .join("\n")
    );
}
