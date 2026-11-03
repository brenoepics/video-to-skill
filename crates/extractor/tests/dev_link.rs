#![cfg(unix)]

//! `scripts/dev-link.sh`: the one command that points Claude Code at this
//! checkout, sets the dev data dir, and opens a chat with the local build.
//!
//! Every test runs the real script against a throwaway HOME, so nothing
//! here touches `~/.claude` or launches a real chat: the checkout copy
//! carries a fake `target/release/vts-extract`, the script is told not to
//! build, and `claude` is a recorder planted on PATH where a test needs
//! one. The script is POSIX `sh`, and Git Bash's `ln -s` copies instead
//! of linking unless MSYS is configured, so this binary is empty on
//! Windows.

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used)]

use std::fs;

#[path = "dev_link/sandbox.rs"]
mod sandbox;
#[path = "dev_link/unlink.rs"]
mod unlink;
use sandbox::{stderr, stdout, symlinks_available, Sandbox};

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// A sandbox, or `None` with a reason when the script cannot run here.
fn sandbox(name: &str) -> Option<Sandbox> {
    let sandbox = Sandbox::new(name, VERSION);
    if !symlinks_available(&sandbox.home) {
        eprintln!("skipped {name}: this process may not create symlinks here");
        return None;
    }
    Some(sandbox)
}

#[test]
fn no_chat_links_the_checkout_and_prints_what_to_run() {
    let Some(sandbox) = sandbox("no-chat") else {
        return;
    };

    let output = sandbox.run(&["--no-chat", "--no-build"]);

    let out = stdout(&output);
    assert!(
        output.status.success(),
        "stdout:\n{out}\nstderr:\n{}",
        stderr(&output)
    );
    assert_eq!(
        fs::read_link(sandbox.skill_link()).unwrap(),
        sandbox.checkout.canonicalize().unwrap(),
        "~/.claude/skills/video-to-skill must point at the checkout"
    );
    let data_dir = sandbox.data_dir();
    assert!(
        data_dir.is_dir(),
        "the dev data dir must exist for the skill to fill"
    );
    assert_eq!(
        fs::read_link(data_dir.join(format!("bin/vts-extract-{VERSION}")))
            .unwrap()
            .canonicalize()
            .unwrap(),
        sandbox
            .checkout
            .join("target/release/vts-extract")
            .canonicalize()
            .unwrap(),
        "SKILL.md's first candidate must resolve to the dev build"
    );
    let export = format!("export VTS_DATA_DIR={}", data_dir.display());
    assert!(out.contains(&export), "missing `{export}` in:\n{out}");
    assert!(
        out.contains("/video-to-skill <video>"),
        "missing the invocation in:\n{out}"
    );
    assert!(
        out.contains(&format!("vts-extract {VERSION}")) && out.contains("matches"),
        "must say the SKILL.md pin matches the build:\n{out}"
    );
    assert!(
        out.contains("claude is not on PATH"),
        "must notice claude is absent:\n{out}"
    );
    assert!(
        !sandbox.home.join("claude-run").exists(),
        "--no-chat never launches claude"
    );
}

#[test]
fn by_default_it_launches_claude_here_with_the_dev_data_dir_set() {
    let Some(sandbox) = sandbox("chat") else {
        return;
    };
    sandbox.plant_fake_claude();

    let output = sandbox.run(&["--no-build"]);

    let out = stdout(&output);
    assert!(
        output.status.success(),
        "stdout:\n{out}\nstderr:\n{}",
        stderr(&output)
    );
    let run = sandbox.claude_run();
    assert!(
        run.contains(&format!(
            "cwd={}\n",
            sandbox.home.canonicalize().unwrap().display()
        )),
        "claude must start in the directory the script was run from:\n{run}"
    );
    assert!(
        run.contains(&format!("VTS_DATA_DIR={}\n", sandbox.data_dir().display())),
        "claude must inherit the dev data dir:\n{run}"
    );
    assert!(
        out.contains("/video-to-skill <video>"),
        "must say what to type:\n{out}"
    );
    assert!(
        sandbox.skill_link().is_symlink(),
        "the link must be in place before the chat"
    );
}

#[test]
fn without_claude_the_default_mode_fails_before_building_or_linking() {
    let Some(sandbox) = sandbox("no-claude") else {
        return;
    };

    let output = sandbox.run(&["--no-build"]);

    assert!(
        !output.status.success(),
        "no claude, no chat: the run must fail"
    );
    let err = stderr(&output);
    assert!(
        err.contains("claude is not on PATH") && err.contains("--no-chat"),
        "got:\n{err}"
    );
    assert!(
        fs::symlink_metadata(sandbox.skill_link()).is_err(),
        "nothing may be linked yet"
    );
}

#[test]
fn honours_a_data_dir_override() {
    let Some(sandbox) = sandbox("data-dir") else {
        return;
    };
    let custom = sandbox.home.join("elsewhere/vts");

    let output = sandbox.run(&[
        "--no-chat",
        "--no-build",
        "--data-dir",
        custom.to_str().unwrap(),
    ]);

    let out = stdout(&output);
    assert!(output.status.success(), "stderr:\n{}", stderr(&output));
    assert!(custom
        .join(format!("bin/vts-extract-{VERSION}"))
        .is_symlink());
    assert!(
        !sandbox.data_dir().exists(),
        "the default dir must stay untouched"
    );
    assert!(out.contains(&format!("export VTS_DATA_DIR={}", custom.display())));
}

#[test]
fn a_pin_mismatch_fails_before_linking_and_names_the_pin_to_update() {
    let Some(sandbox) = sandbox("pin") else {
        return;
    };
    sandbox.pin_skill_to("0.0.1");

    let output = sandbox.run(&["--no-chat", "--no-build"]);

    assert!(!output.status.success(), "a stale pin must fail the run");
    let err = stderr(&output);
    assert!(
        err.contains("0.0.1") && err.contains(VERSION),
        "must name both versions:\n{err}"
    );
    assert!(
        err.contains("SKILL.md"),
        "must say where the pin lives:\n{err}"
    );
    assert!(
        fs::symlink_metadata(sandbox.skill_link()).is_err(),
        "the checkout must not be linked while its SKILL.md disagrees with the build"
    );
}
