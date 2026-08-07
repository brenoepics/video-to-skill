//! End-to-end installation against temp dirs standing in for agent
//! roots: one real copy, relative symlinks everywhere else, a dry run
//! that writes nothing, and a copy fallback where symlinks are refused.

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};

use vts_extract::install::apply::{apply, ApplyOptions};
use vts_extract::install::report::Method;
use vts_extract::install::roots::detect;
use vts_extract::install::{plan_install, Plan, SkipReason};

fn sandbox(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("vts-install-apply")
        .join(format!("{name}-{}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A minimal but realistically-shaped compiled package.
fn package(at: &Path, extra_step: Option<&str>) -> PathBuf {
    let dir = at.join("compiled");
    fs::create_dir_all(dir.join("steps")).unwrap();
    fs::create_dir_all(dir.join("references/frames")).unwrap();
    fs::write(
        dir.join("SKILL.md"),
        "---\nname: vim-basics\ndescription: d\n---\n\n# Vim Basics\n",
    )
    .unwrap();
    fs::write(dir.join("steps/01-quit-vim.md"), "type :q!\n").unwrap();
    fs::write(
        dir.join("references/frames/shot000.jpg"),
        [0xff, 0xd8, 0x00],
    )
    .unwrap();
    if let Some(name) = extra_step {
        fs::write(dir.join("steps").join(name), "stale\n").unwrap();
    }
    dir
}

fn roots(base: &Path, agents: &[&str]) {
    for agent in agents {
        fs::create_dir_all(base.join(format!(".{agent}")).join("skills")).unwrap();
    }
}

fn planned(base: &Path, force: bool) -> Plan {
    let states = detect(base, "vim-basics", &[]);
    plan_install("vim-basics", &states, force).unwrap()
}

fn real() -> ApplyOptions {
    ApplyOptions {
        symlinks: true,
        dry_run: false,
    }
}

#[test]
fn installs_one_real_copy_and_links_every_other_detected_root() {
    let base = sandbox("links");
    let source = package(&base, None);
    roots(&base, &["agents", "claude", "codex"]);

    let report = apply(&planned(&base, false), &source, real()).unwrap();

    // One real copy, byte-identical including nested dirs.
    let canonical = base.join(".agents/skills/vim-basics");
    assert_eq!(report.canonical, fs::canonicalize(&canonical).unwrap());
    assert_eq!(report.canonical_agent, "universal");
    for relative in [
        "SKILL.md",
        "steps/01-quit-vim.md",
        "references/frames/shot000.jpg",
    ] {
        assert_eq!(
            fs::read(canonical.join(relative)).unwrap(),
            fs::read(source.join(relative)).unwrap(),
            "{relative} differs"
        );
    }

    // Every other root is a relative symlink into the canonical copy.
    assert_eq!(report.links.len(), 2);
    for link in &report.links {
        assert_eq!(link.method, Method::Symlink);
        let path = &link.path;
        assert!(
            fs::symlink_metadata(path).unwrap().file_type().is_symlink(),
            "{} is not a symlink",
            path.display()
        );
        assert_eq!(
            fs::read_link(path).unwrap(),
            Path::new("../../.agents/skills/vim-basics")
        );
        assert_eq!(
            fs::read_to_string(path.join("SKILL.md")).unwrap(),
            fs::read_to_string(source.join("SKILL.md")).unwrap()
        );
    }
    assert!(report.to_string().contains("/vim-basics"));
}

#[test]
fn a_dry_run_reports_the_whole_plan_and_writes_nothing() {
    let base = sandbox("dry-run");
    let source = package(&base, None);
    roots(&base, &["agents", "claude", "gemini"]);

    let report = apply(
        &planned(&base, false),
        &source,
        ApplyOptions {
            symlinks: true,
            dry_run: true,
        },
    )
    .unwrap();

    assert!(report.dry_run);
    assert_eq!(report.links.len(), 2);
    assert!(!base.join(".agents/skills/vim-basics").exists());
    assert!(fs::symlink_metadata(base.join(".claude/skills/vim-basics")).is_err());
    assert!(fs::symlink_metadata(base.join(".gemini/skills/vim-basics")).is_err());
    let text = report.to_string();
    assert!(text.contains("dry run"), "{text}");
    assert!(text.contains(".agents/skills/vim-basics"), "{text}");
}

#[test]
fn force_replaces_cleanly_leaving_no_stale_files_behind() {
    let base = sandbox("force");
    let old = package(&base, Some("99-removed.md"));
    roots(&base, &["agents", "claude"]);
    apply(&planned(&base, false), &old, real()).unwrap();
    // A second agent shows up later holding its own older real copy.
    roots(&base, &["codex"]);
    let stray = base.join(".codex/skills/vim-basics");
    fs::create_dir_all(&stray).unwrap();
    fs::write(stray.join("SKILL.md"), "ancient\n").unwrap();

    fs::remove_file(old.join("steps/99-removed.md")).unwrap();
    let report = apply(&planned(&base, true), &old, real()).unwrap();

    let canonical = base.join(".agents/skills/vim-basics");
    assert!(report.replaced);
    assert!(!canonical.join("steps/99-removed.md").exists());
    assert!(canonical.join("steps/01-quit-vim.md").exists());
    // The stray real copy became a link; the canonical copy survived.
    assert!(fs::symlink_metadata(&stray)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(
        fs::read_to_string(stray.join("SKILL.md")).unwrap(),
        fs::read_to_string(old.join("SKILL.md")).unwrap()
    );
    assert!(fs::read_to_string(canonical.join("SKILL.md"))
        .unwrap()
        .contains("vim-basics"));
}

#[test]
fn a_root_already_linked_is_left_alone_on_a_forced_reinstall() {
    let base = sandbox("idempotent");
    let source = package(&base, None);
    roots(&base, &["agents", "claude"]);
    apply(&planned(&base, false), &source, real()).unwrap();

    let report = apply(&planned(&base, true), &source, real()).unwrap();

    assert!(report.links.is_empty());
    assert!(
        report
            .skips
            .iter()
            .any(|s| s.agent == "claude" && s.reason == SkipReason::AlreadyLinked),
        "{:?}",
        report.skips
    );
    assert!(fs::symlink_metadata(base.join(".claude/skills/vim-basics"))
        .unwrap()
        .file_type()
        .is_symlink());
}

#[test]
fn installing_a_package_that_is_already_the_target_is_refused_not_deleted() {
    let base = sandbox("self-install");
    roots(&base, &["agents", "claude"]);
    let source = base.join(".agents/skills/vim-basics");
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("SKILL.md"), "---\nname: vim-basics\n---\n").unwrap();

    let err = apply(&planned(&base, true), &source, real())
        .unwrap_err()
        .to_string();

    assert!(err.contains("already installed"), "{err}");
    assert!(
        source.join("SKILL.md").is_file(),
        "the source package was destroyed"
    );
}

#[test]
fn a_full_copy_replaces_the_symlink_when_symlinks_are_unavailable() {
    let base = sandbox("no-symlinks");
    let source = package(&base, None);
    roots(&base, &["agents", "claude"]);

    let report = apply(
        &planned(&base, false),
        &source,
        ApplyOptions {
            symlinks: false,
            dry_run: false,
        },
    )
    .unwrap();

    let copied = base.join(".claude/skills/vim-basics");
    assert_eq!(report.links[0].method, Method::Copy);
    assert!(copied.is_dir());
    assert!(!fs::symlink_metadata(&copied)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(
        fs::read(copied.join("references/frames/shot000.jpg")).unwrap(),
        fs::read(source.join("references/frames/shot000.jpg")).unwrap()
    );
    let text = report.to_string();
    assert!(text.contains("copy"), "{text}");
}
