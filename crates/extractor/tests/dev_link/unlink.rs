//! Giving `~/.claude/skills/video-to-skill` to the checkout and back:
//! replacing an npx link, refusing a real directory, and `--unlink`.

use std::fs;

use crate::sandbox;
use crate::sandbox::stderr;
use crate::sandbox::stdout;

#[test]
fn replaces_an_existing_symlink_such_as_the_npx_install() {
    let Some(sandbox) = sandbox("relink") else {
        return;
    };
    fs::create_dir_all(sandbox.npx_install()).unwrap();
    fs::create_dir_all(sandbox.skill_link().parent().unwrap()).unwrap();
    std::os::unix::fs::symlink("../../.agents/skills/video-to-skill", sandbox.skill_link())
        .unwrap();

    let output = sandbox.run(&["--no-chat", "--no-build"]);

    assert!(output.status.success(), "stderr:\n{}", stderr(&output));
    assert_eq!(
        fs::read_link(sandbox.skill_link()).unwrap(),
        sandbox.checkout.canonicalize().unwrap()
    );
}

#[test]
fn refuses_to_clobber_a_real_directory_without_force() {
    let Some(sandbox) = sandbox("force") else {
        return;
    };
    let link = sandbox.skill_link();
    fs::create_dir_all(&link).unwrap();
    fs::write(link.join("SKILL.md"), "hand-copied install").unwrap();

    let refused = sandbox.run(&["--no-chat", "--no-build"]);

    assert!(
        !refused.status.success(),
        "a real directory must not be replaced silently"
    );
    let err = stderr(&refused);
    assert!(
        err.contains("--force"),
        "the refusal must name the override:\n{err}"
    );
    assert!(
        link.join("SKILL.md").is_file(),
        "the directory must be intact"
    );
    assert!(
        !sandbox.data_dir().exists(),
        "nothing else may run after a refusal"
    );

    let forced = sandbox.run(&["--no-chat", "--no-build", "--force"]);

    assert!(forced.status.success(), "stderr:\n{}", stderr(&forced));
    assert_eq!(
        fs::read_link(&link).unwrap(),
        sandbox.checkout.canonicalize().unwrap()
    );
    let moved_aside = fs::read_dir(link.parent().unwrap())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("video-to-skill.bak")
        })
        .expect("--force moves the directory aside instead of deleting it");
    assert_eq!(
        fs::read_to_string(moved_aside.join("SKILL.md")).unwrap(),
        "hand-copied install"
    );
}

#[test]
fn unlink_restores_the_npx_install_when_present() {
    let Some(sandbox) = sandbox("unlink") else {
        return;
    };
    fs::create_dir_all(sandbox.npx_install()).unwrap();
    fs::write(sandbox.npx_install().join("SKILL.md"), "npx copy").unwrap();
    assert!(sandbox.run(&["--no-chat", "--no-build"]).status.success());

    let output = sandbox.run(&["--unlink"]);

    assert!(output.status.success(), "stderr:\n{}", stderr(&output));
    assert_eq!(
        sandbox.skill_link().canonicalize().unwrap(),
        sandbox.npx_install().canonicalize().unwrap(),
        "the link must point back at ~/.agents/skills/video-to-skill"
    );
    assert_eq!(
        fs::read_to_string(sandbox.skill_link().join("SKILL.md")).unwrap(),
        "npx copy"
    );
}

#[test]
fn unlink_without_an_npx_install_removes_the_link_and_says_how_to_reinstall() {
    let Some(sandbox) = sandbox("unlink-bare") else {
        return;
    };
    assert!(sandbox.run(&["--no-chat", "--no-build"]).status.success());

    let output = sandbox.run(&["--unlink"]);

    assert!(output.status.success(), "stderr:\n{}", stderr(&output));
    assert!(
        fs::symlink_metadata(sandbox.skill_link()).is_err(),
        "nothing to restore: the link must be gone"
    );
    assert!(
        stdout(&output).contains("npx skills"),
        "must say how to reinstall:\n{}",
        stdout(&output)
    );
}
