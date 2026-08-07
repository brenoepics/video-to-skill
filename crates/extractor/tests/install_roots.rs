//! Agent-root detection and skill-package validation. Detection reads a
//! caller-supplied base dir — never `$HOME` — so these tests are hermetic.

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};

use vts_extract::install::package::skill_name_of;
use vts_extract::install::roots::{detect, known_agents, unknown_agents};
use vts_extract::install::SkipReason;

fn sandbox(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("vts-install-roots")
        .join(format!("{name}-{}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn state<'a>(
    states: &'a [vts_extract::install::RootState],
    agent: &str,
) -> &'a vts_extract::install::RootState {
    states.iter().find(|s| s.agent == agent).unwrap()
}

#[test]
fn every_known_agent_is_reported_present_or_absent() {
    let base = sandbox("presence");
    fs::create_dir_all(base.join(".agents/skills")).unwrap();
    fs::create_dir_all(base.join(".codex/skills")).unwrap();

    let states = detect(&base, "vim-basics", &[]);

    assert_eq!(states.len(), known_agents().len());
    assert!(state(&states, "universal").root.is_some());
    assert!(state(&states, "codex").root.is_some());
    assert!(state(&states, "claude").root.is_none());
    assert!(state(&states, "cursor").root.is_none());
    // Detection is read-only: an absent root stays absent.
    assert!(!base.join(".cursor/skills").exists());
}

#[test]
fn amp_is_found_under_either_of_its_two_conventions() {
    let dotted = sandbox("amp-dotted");
    fs::create_dir_all(dotted.join(".amp/skills")).unwrap();
    assert_eq!(
        state(&detect(&dotted, "s", &[]), "amp").root,
        Some(fs::canonicalize(dotted.join(".amp/skills")).unwrap())
    );

    let config = sandbox("amp-config");
    fs::create_dir_all(config.join(".config/amp/skills")).unwrap();
    assert_eq!(
        state(&detect(&config, "s", &[]), "amp").root,
        Some(fs::canonicalize(config.join(".config/amp/skills")).unwrap())
    );
}

#[test]
fn only_filters_detection_to_the_named_agents() {
    let base = sandbox("only");
    fs::create_dir_all(base.join(".agents/skills")).unwrap();
    fs::create_dir_all(base.join(".claude/skills")).unwrap();

    let states = detect(&base, "vim-basics", &["claude".to_string()]);

    assert_eq!(states.len(), 1);
    assert_eq!(states[0].agent, "claude");
}

#[test]
fn unknown_agent_names_are_reported_for_the_caller_to_reject() {
    assert!(unknown_agents(&["claude".into(), "amp".into()]).is_empty());
    assert_eq!(
        unknown_agents(&["claude".into(), "clyde".into()]),
        vec!["clyde".to_string()]
    );
}

#[test]
fn an_existing_skill_is_seen_as_occupied_and_a_symlink_is_resolved() {
    let base = sandbox("occupancy");
    let universal = base.join(".agents/skills");
    let claude = base.join(".claude/skills");
    let codex = base.join(".codex/skills");
    fs::create_dir_all(universal.join("vim-basics")).unwrap();
    fs::create_dir_all(&claude).unwrap();
    fs::create_dir_all(codex.join("vim-basics")).unwrap();
    symlink(
        Path::new("../../.agents/skills/vim-basics"),
        &claude.join("vim-basics"),
    );

    let states = detect(&base, "vim-basics", &[]);

    assert!(state(&states, "universal").occupied);
    assert!(state(&states, "codex").occupied);
    assert_eq!(state(&states, "codex").link_target, None);
    let linked = state(&states, "claude");
    assert!(linked.occupied);
    assert_eq!(
        linked.link_target,
        Some(fs::canonicalize(universal.join("vim-basics")).unwrap())
    );
}

#[test]
fn a_detected_symlink_lets_the_planner_skip_that_root() {
    let base = sandbox("skip-linked");
    let universal = base.join(".agents/skills");
    let claude = base.join(".claude/skills");
    fs::create_dir_all(universal.join("vim-basics")).unwrap();
    fs::create_dir_all(&claude).unwrap();
    symlink(
        Path::new("../../.agents/skills/vim-basics"),
        &claude.join("vim-basics"),
    );

    let states = detect(&base, "vim-basics", &[]);
    let plan = vts_extract::install::plan_install("vim-basics", &states, true).unwrap();

    assert_eq!(plan.canonical.agent, "universal");
    assert!(plan.links.is_empty());
    assert!(plan
        .skips
        .iter()
        .any(|s| s.agent == "claude" && s.reason == SkipReason::AlreadyLinked));
}

#[test]
fn a_package_without_skill_md_is_refused() {
    let dir = sandbox("not-a-package");
    fs::write(dir.join("README.md"), "hi").unwrap();

    let err = skill_name_of(&dir).unwrap_err().to_string();

    assert!(err.contains("SKILL.md"), "{err}");
    assert!(err.contains(&dir.display().to_string()), "{err}");
}

#[test]
fn the_skill_name_comes_from_the_frontmatter_then_the_directory() {
    let named = sandbox("named").join("out");
    fs::create_dir_all(&named).unwrap();
    fs::write(
        named.join("SKILL.md"),
        "---\nname: vim-basics\ndescription: x\n---\n\n# Vim\n",
    )
    .unwrap();
    assert_eq!(skill_name_of(&named).unwrap(), "vim-basics");

    let unnamed = sandbox("unnamed").join("excel-formulas");
    fs::create_dir_all(&unnamed).unwrap();
    fs::write(unnamed.join("SKILL.md"), "# no frontmatter here\n").unwrap();
    assert_eq!(skill_name_of(&unnamed).unwrap(), "excel-formulas");
}

#[cfg(unix)]
fn symlink(target: &Path, link: &Path) {
    std::os::unix::fs::symlink(target, link).unwrap();
}

#[cfg(windows)]
fn symlink(target: &Path, link: &Path) {
    std::os::windows::fs::symlink_dir(target, link).unwrap();
}
