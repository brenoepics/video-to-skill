//! Project-policy checks enforced by the test suite.

use std::fs;
use std::path::{Path, PathBuf};

/// Maximum allowed lines per Rust source file, workspace-wide.
pub const MAX_RS_LINES: usize = 300;

/// Directories never scanned for policy violations.
const SKIPPED_DIRS: &[&str] = &["target", ".git", "node_modules"];

/// Walk `root` and return every `.rs` file whose line count exceeds `limit`,
/// with its line count. Build output and VCS directories are skipped.
#[must_use]
pub fn files_over_limit(root: &Path, limit: usize) -> Vec<(PathBuf, usize)> {
    let mut offenders = Vec::new();
    visit(root, limit, &mut offenders);
    offenders.sort();
    offenders
}

fn visit(dir: &Path, limit: usize, offenders: &mut Vec<(PathBuf, usize)>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let skipped = path
                .file_name()
                .is_some_and(|n| SKIPPED_DIRS.iter().any(|s| n == *s));
            if !skipped {
                visit(&path, limit, offenders);
            }
        } else if path.extension().is_some_and(|e| e == "rs") {
            if let Ok(contents) = fs::read_to_string(&path) {
                let lines = contents.lines().count();
                if lines > limit {
                    offenders.push((path, lines));
                }
            }
        }
    }
}
