//! Project-policy checks enforced by the test suite.

use std::fs;
use std::path::{Path, PathBuf};

/// Maximum allowed lines per Rust source file, workspace-wide.
pub const MAX_RS_LINES: usize = 300;

/// Directories never scanned for policy violations. `.claude` holds the
/// worktrees of other branches, which are other checkouts, not this one.
const SKIPPED_DIRS: &[&str] = &["target", ".git", "node_modules", ".claude"];

/// Walk `root` and return every `.rs` file whose line count exceeds `limit`,
/// with its line count. Build output and VCS directories are skipped.
#[must_use]
pub fn files_over_limit(root: &Path, limit: usize) -> Vec<(PathBuf, usize)> {
    let mut offenders = Vec::new();
    visit(root, &is_rust, &mut |path, contents| {
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
    visit(root, &is_rust, &mut |path, contents| {
        if contents.contains(&needle) {
            offenders.push(path.to_path_buf());
        }
    });
    offenders.sort();
    offenders
}

/// Walk every UTF-8 file under `root` (whatever its extension, so
/// extensionless scripts count) and return every URL host, with the file it
/// appears in, that borrows the label `example` without being a name reserved for
/// documentation (RFC 2606, RFC 6761): `*.example`, `*.invalid`, `*.test`,
/// `*.localhost`, or `example.com`, `.net`, `.org` and their subdomains.
///
/// A host like `get.example` under a real country code reads as a
/// placeholder, but anyone can register it, and URL-reputation scanners
/// treat a download piped into a shell from it as a live dropper. That is
/// how the safety tests' fetch-and-execute fixtures got the skill flagged
/// by a marketplace audit.
#[must_use]
pub fn urls_with_unreserved_example_hosts(root: &Path) -> Vec<(PathBuf, String)> {
    let mut offenders = Vec::new();
    visit(root, &|_| true, &mut |path, contents| {
        for host in url_hosts(contents) {
            if borrows_example(&host) && !is_reserved(&host) {
                offenders.push((path.to_path_buf(), host));
            }
        }
    });
    offenders.sort();
    offenders.dedup();
    offenders
}

/// The lowercased host of every `http://` or `https://` URL in `text`,
/// with the scheme matched case-insensitively and the host read from the
/// authority: after any `user:password@`, before any `:port`.
fn url_hosts(text: &str) -> impl Iterator<Item = String> + '_ {
    text.match_indices("://").filter_map(move |(at, _)| {
        let scheme: String = text[..at]
            .chars()
            .rev()
            .take_while(char::is_ascii_alphabetic)
            .collect::<String>()
            .to_ascii_lowercase();
        if scheme != "ptth" && scheme != "sptth" {
            return None;
        }
        let rest = &text[at + 3..];
        let authority_end = rest
            .find(|c: char| c.is_whitespace() || "/?#\"'`<>()[]{}|\\,;".contains(c))
            .unwrap_or(rest.len());
        let authority = &rest[..authority_end];
        let host_and_port = authority.rsplit('@').next().unwrap_or(authority);
        let host = host_and_port.split(':').next().unwrap_or(host_and_port);
        let valid_end = host
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '.' || c == '-'))
            .unwrap_or(host.len());
        let host = host[..valid_end].trim_end_matches('.');
        (!host.is_empty()).then(|| host.to_ascii_lowercase())
    })
}

fn borrows_example(host: &str) -> bool {
    host.split('.').any(|label| label == "example")
}

fn is_rust(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == "rs")
}

fn is_reserved(host: &str) -> bool {
    let labels: Vec<&str> = host.split('.').collect();
    let tld = labels.last().copied().unwrap_or_default();
    let reserved_tld = matches!(tld, "example" | "invalid" | "test" | "localhost");
    let reserved_domain = labels.len() >= 2
        && labels[labels.len() - 2] == "example"
        && matches!(tld, "com" | "net" | "org");
    reserved_tld || reserved_domain
}

/// Files that are not valid UTF-8 (images, fixtures) are skipped: they
/// carry no URLs a reader or a scanner would follow.
fn visit(dir: &Path, accept: &dyn Fn(&Path) -> bool, on_file: &mut impl FnMut(&Path, &str)) {
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
                visit(&path, accept, on_file);
            }
        } else if accept(&path) {
            if let Ok(contents) = fs::read_to_string(&path) {
                on_file(&path, &contents);
            }
        }
    }
}
