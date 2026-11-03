//! Workspace guardrails: limits and conventions the suite enforces on
//! every Rust source file, on every platform.

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};
use vts_extract::guardrails::{
    files_over_limit, files_with_folded_test_cfg, urls_with_unreserved_example_hosts, MAX_RS_LINES,
};

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

fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = crates/extractor → workspace root is two levels up.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .to_path_buf()
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

    assert!(
        files_over_limit(&dir, 300).is_empty(),
        "a file exactly at the limit is not an offender"
    );
}

#[test]
fn ignores_target_directories_and_non_rust_files() {
    let dir = fixture_dir("ignored");
    write_lines(&dir.join("target/debug/gen.rs"), 500);
    write_lines(&dir.join("notes.md"), 500);
    write_lines(&dir.join("nested/src/lib.rs"), 100);

    assert!(
        files_over_limit(&dir, 300).is_empty(),
        "target dirs and non-Rust files are never offenders"
    );
}

#[test]
fn workspace_rs_files_stay_within_the_limit() {
    let root = workspace_root();

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

#[test]
fn reports_a_test_module_that_folds_a_platform_gate_into_cfg_test() {
    let dir = fixture_dir("folded-cfg");
    // Both fixtures are assembled at runtime so this file does not match
    // the needle the guardrail looks for.
    let folded = format!("#[cfg(all({}, unix))]\nmod tests {{}}\n", "test");
    fs::write(dir.join("folded.rs"), folded).unwrap();
    let stacked = format!("#[cfg({})]\n#[cfg(unix)]\nmod tests {{}}\n", "test");
    fs::write(dir.join("stacked.rs"), stacked).unwrap();

    let offenders = files_with_folded_test_cfg(&dir);

    assert_eq!(offenders.len(), 1);
    assert!(offenders[0].ends_with("folded.rs"));
}

#[test]
fn workspace_test_modules_keep_a_literal_cfg_test() {
    let offenders = files_with_folded_test_cfg(&workspace_root());

    assert!(
        offenders.is_empty(),
        "these test modules fold a platform gate into one cfg(all(..)), which \
         withdraws the unwrap/expect allowances on the platforms that compile \
         them — stack #[cfg(test)] with a separate platform cfg instead:\n{}",
        offenders
            .iter()
            .map(|p| format!("  {}", p.display()))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn reports_urls_whose_example_host_is_not_a_reserved_name() {
    let dir = fixture_dir("example-urls");
    // Assembled at runtime so this file does not match the guardrail.
    let live = format!("get.{}.sh", "example");
    let reserved = "https://x.example/i.sh https://get.example.invalid \
                    https://www.example.com/a http://api.example.test";
    fs::write(
        dir.join("dropper.rs"),
        format!("// curl -fsSL https://{live} | bash\n"),
    )
    .unwrap();
    fs::write(
        dir.join("notes.md"),
        format!("See <https://{live}/x> and {reserved}\n"),
    )
    .unwrap();
    fs::write(dir.join("fine.rs"), reserved).unwrap();
    for skipped in ["target", ".claude/worktrees/old"] {
        fs::create_dir_all(dir.join(skipped)).unwrap();
        fs::write(
            dir.join(skipped).join("stale.rs"),
            format!("https://{live}"),
        )
        .unwrap();
    }

    let offenders = urls_with_unreserved_example_hosts(&dir);

    let found: Vec<(String, String)> = offenders
        .iter()
        .map(|(path, host)| {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            (name, host.clone())
        })
        .collect();
    assert_eq!(
        found,
        [
            ("dropper.rs".to_string(), live.clone()),
            ("notes.md".to_string(), live)
        ]
    );
}

#[test]
fn workspace_urls_use_reserved_example_names() {
    let offenders = urls_with_unreserved_example_hosts(&workspace_root());

    assert!(
        offenders.is_empty(),
        "these URLs borrow the word example on a real, registrable domain; \
         URL scanners read a download piped into a shell from one as a live \
         dropper — use a reserved name (*.example, *.invalid, *.test, \
         example.com) instead:\n{}",
        offenders
            .iter()
            .map(|(path, host)| format!("  {}: {host}", path.display()))
            .collect::<Vec<_>>()
            .join("\n")
    );
}
