//! Package names are portable single directory names before planning writes.

#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::PathBuf;

use vts_extract::install::package::skill_name_of;
use vts_extract::install::{plan_install, PlanError, RootState};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("vts-install-package-tests")
        .join(format!("{name}-{}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn crlf_frontmatter_keeps_its_declared_skill_name() {
    let package = scratch("crlf");
    fs::write(
        package.join("SKILL.md"),
        "---\r\nname: windows-basics\r\ndescription: Test\r\n---\r\n",
    )
    .unwrap();

    assert_eq!(skill_name_of(&package).unwrap(), "windows-basics");
}

#[test]
fn unsafe_frontmatter_names_are_rejected() {
    for (index, name) in ["../outside", r"..\outside", "C:/outside", "CON"]
        .into_iter()
        .enumerate()
    {
        let package = scratch(&format!("unsafe-{index}"));
        fs::write(
            package.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: Test\n---\n"),
        )
        .unwrap();

        let error = skill_name_of(&package).unwrap_err().to_string();

        assert!(error.contains("safe kebab-case"), "{name}: {error}");
    }
}

#[test]
fn planner_rejects_an_unsafe_name_even_when_called_directly() {
    let root = scratch("planner");
    let states = [RootState::vacant("universal", root.join("skills"))];

    let error = plan_install(r"..\outside", &states, false).unwrap_err();

    assert!(matches!(error, PlanError::InvalidSkillName { .. }));
}

/// Installation reads only SKILL.md, so a package compiled from a v1
/// Procedure IR keeps installing after the schema moved to v2.
#[test]
fn v1_packages_remain_installable() {
    let package = scratch("v1-package");
    fs::write(
        package.join("SKILL.md"),
        "---\nname: vim-basics\ndescription: Test\n---\n",
    )
    .unwrap();
    fs::write(
        package.join("provenance.json"),
        r#"{"schema_version": 1, "skill_name": "vim-basics", "title": "Vim Basics",
            "description": "Test", "overview": "o", "genre": "screencast",
            "source": {"original": "v.mp4", "title": null, "duration_secs": 1.0},
            "steps": [{"id": "01-quit", "goal": "g", "actions": "a", "success_criteria": "s",
                       "confidence": "high", "caveats": null,
                       "evidence": [{"timestamp_secs": 0.0, "frame": null, "quote": "q"}]}]}"#,
    )
    .unwrap();

    assert_eq!(skill_name_of(&package).unwrap(), "vim-basics");
    let states = [RootState::vacant("universal", package.join("skills"))];
    assert!(plan_install("vim-basics", &states, false).is_ok());
}
