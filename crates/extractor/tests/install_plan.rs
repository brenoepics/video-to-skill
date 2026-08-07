//! Pure install planning: which root becomes the single source of truth,
//! which roots get a relative symlink, what is skipped, and what conflicts.
//! No test here touches the filesystem or the real `$HOME`.

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use vts_extract::install::{plan_install, PlanError, RootState, SkipReason};

fn root(agent: &str) -> PathBuf {
    Path::new("/home/u")
        .join(format!(".{agent}"))
        .join("skills")
}

fn agents_of(states: &[RootState]) -> Vec<String> {
    states.iter().map(|s| s.agent.clone()).collect()
}

#[test]
fn canonical_root_is_the_universal_one_when_it_exists() {
    let roots = vec![
        RootState::vacant("claude", root("claude")),
        RootState::vacant("universal", root("agents")),
        RootState::vacant("codex", root("codex")),
    ];

    let plan = plan_install("vim-basics", &roots, false).unwrap();

    assert_eq!(plan.canonical.agent, "universal");
    assert_eq!(plan.canonical.path, root("agents").join("vim-basics"));
    assert!(!plan.canonical.replaces);
}

#[test]
fn canonical_root_falls_back_to_claude_when_universal_is_absent() {
    let roots = vec![
        RootState::missing("universal"),
        RootState::vacant("codex", root("codex")),
        RootState::vacant("claude", root("claude")),
    ];

    let plan = plan_install("vim-basics", &roots, false).unwrap();

    assert_eq!(plan.canonical.agent, "claude");
    assert_eq!(plan.canonical.path, root("claude").join("vim-basics"));
}

#[test]
fn canonical_root_falls_back_to_the_first_detected_root() {
    let roots = vec![
        RootState::missing("universal"),
        RootState::missing("claude"),
        RootState::vacant("codex", root("codex")),
        RootState::vacant("gemini", root("gemini")),
    ];

    let plan = plan_install("vim-basics", &roots, false).unwrap();

    assert_eq!(plan.canonical.agent, "codex");
}

#[test]
fn every_other_detected_root_gets_a_relative_link_to_the_canonical_package() {
    let roots = vec![
        RootState::vacant("universal", root("agents")),
        RootState::vacant("claude", root("claude")),
        RootState::vacant("gemini", root("gemini")),
    ];

    let plan = plan_install("vim-basics", &roots, false).unwrap();

    assert_eq!(
        plan.links
            .iter()
            .map(|l| l.agent.clone())
            .collect::<Vec<_>>(),
        vec!["claude", "gemini"]
    );
    let claude = &plan.links[0];
    assert_eq!(claude.path, root("claude").join("vim-basics"));
    // Relative to the link's own parent dir, matching the convention
    // already used by the `npx skills` CLI on disk.
    assert_eq!(
        claude.relative_target,
        Path::new("../../.agents/skills/vim-basics")
    );
    assert!(!claude.replaces);
}

#[test]
fn roots_that_do_not_exist_are_skipped_as_not_installed() {
    let roots = vec![
        RootState::vacant("universal", root("agents")),
        RootState::missing("cursor"),
        RootState::missing("amp"),
    ];

    let plan = plan_install("vim-basics", &roots, false).unwrap();

    assert!(plan.links.is_empty());
    assert_eq!(agents_of_skips(&plan.skips), vec!["cursor", "amp"]);
    assert!(plan
        .skips
        .iter()
        .all(|s| s.reason == SkipReason::NotInstalled));
}

fn agents_of_skips(skips: &[vts_extract::install::Skip]) -> Vec<String> {
    skips.iter().map(|s| s.agent.clone()).collect()
}

#[test]
fn an_existing_skill_at_a_link_target_is_a_conflict_naming_the_path() {
    let roots = vec![
        RootState::vacant("universal", root("agents")),
        RootState::occupied("claude", root("claude")),
    ];

    let err = plan_install("vim-basics", &roots, false).unwrap_err();

    assert_eq!(
        err,
        PlanError::Conflict {
            agent: "claude".into(),
            path: root("claude").join("vim-basics"),
        }
    );
    let message = err.to_string();
    assert!(
        message.contains("/home/u/.claude/skills/vim-basics"),
        "{message}"
    );
    assert!(message.contains("--force"), "{message}");
}

#[test]
fn an_existing_skill_at_the_canonical_root_is_a_conflict_too() {
    let roots = vec![
        RootState::occupied("universal", root("agents")),
        RootState::vacant("claude", root("claude")),
    ];

    let err = plan_install("vim-basics", &roots, false).unwrap_err();

    assert_eq!(
        err,
        PlanError::Conflict {
            agent: "universal".into(),
            path: root("agents").join("vim-basics"),
        }
    );
}

#[test]
fn force_replaces_existing_skills_instead_of_erroring() {
    let roots = vec![
        RootState::occupied("universal", root("agents")),
        RootState::occupied("claude", root("claude")),
        RootState::vacant("codex", root("codex")),
    ];

    let plan = plan_install("vim-basics", &roots, true).unwrap();

    assert!(plan.canonical.replaces);
    assert!(plan.links[0].replaces);
    assert!(!plan.links[1].replaces);
}

#[test]
fn a_root_already_linked_to_the_canonical_package_is_skipped() {
    let canonical = root("agents").join("vim-basics");
    let roots = vec![
        RootState::vacant("universal", root("agents")),
        RootState::linked("claude", root("claude"), &canonical),
        RootState::linked("codex", root("codex"), "/somewhere/else/vim-basics"),
    ];

    let err = plan_install("vim-basics", &roots, false).unwrap_err();
    // A link pointing somewhere else is still an occupied path.
    assert!(matches!(err, PlanError::Conflict { ref agent, .. } if agent == "codex"));

    let plan = plan_install("vim-basics", &roots[..2], false).unwrap();
    assert!(plan.links.is_empty());
    assert_eq!(agents_of_skips(&plan.skips), vec!["claude"]);
    assert_eq!(plan.skips[0].reason, SkipReason::AlreadyLinked);
}

#[test]
fn no_detected_root_is_an_error_rather_than_creating_one() {
    let roots = vec![
        RootState::missing("universal"),
        RootState::missing("claude"),
    ];

    let err = plan_install("vim-basics", &roots, false).unwrap_err();

    assert_eq!(err, PlanError::NoAgentsDetected);
    assert!(err.to_string().contains("no agent skill directory"));
}

#[test]
fn root_states_keep_the_agent_name_they_were_built_with() {
    let states = vec![
        RootState::missing("cursor"),
        RootState::vacant("claude", root("claude")),
    ];
    assert_eq!(agents_of(&states), vec!["cursor", "claude"]);
}
