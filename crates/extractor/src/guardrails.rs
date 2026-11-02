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
    visit(root, &mut |path, contents| {
        let lines = contents.lines().count();
        if lines > limit {
            offenders.push((path.to_path_buf(), lines));
        }
    });
    offenders.sort();
    offenders
}

/// Walk `root` and return every `.rs` file that gates a test module with an
/// `all(..)` predicate folding `test` together with another condition,
/// rather than with a literal `cfg(test)`.
///
/// Clippy recognises test code by a literal `cfg(test)`, so folding a
/// platform gate into one `all(..)` predicate silently withdraws the
/// `unwrap_used` and `expect_used` allowances inside that module — and
/// only on the platforms that compile it, so the lint lands on one CI
/// runner and not the machine it was written on. Stacking `#[cfg(test)]`
/// with a separate `#[cfg(unix)]` gates the module exactly the same way
/// and keeps the allowances.
#[must_use]
pub fn files_with_folded_test_cfg(root: &Path) -> Vec<PathBuf> {
    // Assembled at runtime so this file never matches its own needle.
    let needle = format!("cfg(all({}", "test");
    let mut offenders = Vec::new();
    visit(root, &mut |path, contents| {
        if contents.contains(&needle) {
            offenders.push(path.to_path_buf());
        }
    });
    offenders.sort();
    offenders
}

fn visit(dir: &Path, on_rust_file: &mut impl FnMut(&Path, &str)) {
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
                visit(&path, on_rust_file);
            }
        } else if path.extension().is_some_and(|e| e == "rs") {
            if let Ok(contents) = fs::read_to_string(&path) {
                on_rust_file(&path, &contents);
            }
        }
    }
}
