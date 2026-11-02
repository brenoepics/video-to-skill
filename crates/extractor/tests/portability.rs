//! The contract that must hold identically on every supported platform.
//!
//! `sandbox.rs` proves the Unix backends confine for real and
//! `sandbox_windows.rs` does the same for LPAC, but each is gated to one
//! platform family. The assertions here are the part that has to be true
//! everywhere, so a backend can never be added for one platform while the
//! shared entry points quietly stop working on another.

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};

use vts_extract::deps::{Env, Tool};
use vts_extract::sandbox::{self, Mechanism};

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("vts-portability-tests")
        .join(format!("{tag}-{}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn the_detected_mechanism_belongs_to_this_platform() {
    let detected = sandbox::detect();

    if cfg!(windows) {
        assert_eq!(
            detected,
            Some(Mechanism::WindowsAppContainer),
            "Windows confines with a native backend that needs no helper binary"
        );
    } else {
        assert_ne!(
            detected,
            Some(Mechanism::WindowsAppContainer),
            "the Windows backend must never be selected off Windows"
        );
    }
}

#[test]
fn every_mechanism_can_name_itself() {
    for mechanism in [
        Mechanism::Seatbelt,
        Mechanism::Bubblewrap,
        Mechanism::WindowsAppContainer,
    ] {
        assert!(
            !mechanism.name().trim().is_empty(),
            "{mechanism:?} must name itself for `check` to report it"
        );
    }
}

#[test]
fn the_policy_renders_on_every_platform() {
    let workspace = scratch("policy");

    let text = sandbox::policy_text(&workspace).expect("every platform renders a policy");

    assert!(
        !text.trim().is_empty(),
        "--print-policy must explain the confinement it applies"
    );
}

#[test]
fn an_empty_command_is_refused_before_any_mechanism_is_chosen() {
    let workspace = scratch("empty-command");

    let error = sandbox::run(&workspace, &[]).expect_err("an empty command must be refused");

    assert!(
        error.to_string().contains("no command"),
        "the refusal must say what was missing, got: {error}"
    );
}

#[test]
fn a_workspace_that_does_not_exist_is_refused() {
    let missing = scratch("missing").join("absent");

    assert!(
        sandbox::policy_text(&missing).is_err(),
        "a policy cannot be rendered for a workspace that does not exist"
    );
    assert!(
        sandbox::run(&missing, &["true".into()]).is_err(),
        "a command must never run against a workspace that does not exist"
    );
}

#[test]
fn managed_tool_file_names_match_this_platform() {
    for tool in Tool::ALL {
        let name = tool.file_name();
        assert!(!name.is_empty(), "{tool:?} must have a file name");
        let is_exe = Path::new(name)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"));
        if tool.is_executable() && cfg!(windows) {
            assert!(
                is_exe,
                "{tool:?} must be looked up as a Windows executable, got '{name}'"
            );
        } else {
            assert!(
                !is_exe,
                "{tool:?} must not carry a Windows suffix here, got '{name}'"
            );
        }
    }
}

#[test]
fn the_platform_data_dir_is_absolute() {
    if std::env::var_os("VTS_DATA_DIR").is_some() {
        eprintln!("VTS_DATA_DIR overrides the platform data dir — skipping");
        return;
    }

    let env = Env::from_system();

    assert!(
        env.data_dir.is_absolute(),
        "managed tools must install under an absolute path, got {}",
        env.data_dir.display()
    );
}
