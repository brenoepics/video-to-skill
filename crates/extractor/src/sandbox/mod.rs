//! OS-level confinement for executing steps transcribed from a video.
//!
//! Verification runs commands that were read off an untrusted video's
//! pixels. A protocol document asking an agent to "use a throwaway
//! workspace" is a convention, not a boundary — this module is the
//! boundary: the kernel refuses network access, refuses writes outside
//! the workspace, and refuses reads of credential stores, whatever the
//! command turns out to be.
//!
//! Confinement is mandatory. When no mechanism is available the run is
//! refused rather than silently downgraded to an unsandboxed shell.

pub mod bubblewrap;
pub mod profile;

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};

pub use profile::Confinement;

/// The confinement mechanism available on this machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mechanism {
    /// macOS `sandbox-exec` driven by a generated seatbelt profile.
    Seatbelt,
    /// Linux `bwrap` (bubblewrap).
    Bubblewrap,
}

impl Mechanism {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Seatbelt => "macOS seatbelt (sandbox-exec)",
            Self::Bubblewrap => "bubblewrap (bwrap)",
        }
    }
}

/// The mechanism this machine can use, if any.
#[must_use]
pub fn detect() -> Option<Mechanism> {
    if cfg!(target_os = "macos") && which("sandbox-exec").is_some() {
        return Some(Mechanism::Seatbelt);
    }
    which("bwrap").map(|_| Mechanism::Bubblewrap)
}

/// Run `command` confined to `workspace`, returning its exit code.
///
/// Fails — rather than running the command unconfined — when no
/// sandbox mechanism is present.
pub fn run(workspace: &Path, command: &[String]) -> Result<i32> {
    if command.is_empty() {
        bail!("no command given to run under the sandbox");
    }
    let Some(mechanism) = detect() else {
        bail!(
            "no sandbox mechanism available on this machine (looked for \
             sandbox-exec on macOS, bwrap on Linux) — refusing to execute \
             video-derived commands unconfined; mark the step 'unverifiable' \
             instead"
        );
    };
    let (workspace, home, tmp) = confinement_paths(workspace)?;
    let confinement = Confinement {
        workspace: &workspace,
        home: &home,
        tmp: &tmp,
    };

    let status = match mechanism {
        Mechanism::Seatbelt => run_seatbelt(&confinement, command),
        Mechanism::Bubblewrap => run_bwrap(&confinement, command),
    }?;
    Ok(status)
}

/// The exact policy `run` would apply, rendered for inspection — the
/// confinement is only trustworthy if a user can read it.
pub fn policy_text(workspace: &Path) -> Result<String> {
    let (workspace, home, tmp) = confinement_paths(workspace)?;
    let c = Confinement {
        workspace: &workspace,
        home: &home,
        tmp: &tmp,
    };
    match detect() {
        Some(Mechanism::Bubblewrap) => Ok(bubblewrap::bwrap_args(&c, &["<command>".into()])?
            .join(" ")
            .replace(" --", "\n  --")),
        // default to showing the seatbelt policy when nothing is
        // available, so `--print-policy` still explains the intent
        _ => profile::seatbelt_profile(&c),
    }
}

fn confinement_paths(workspace: &Path) -> Result<(PathBuf, PathBuf, PathBuf)> {
    let workspace = workspace
        .canonicalize()
        .with_context(|| format!("workspace {} does not exist", workspace.display()))?;
    let home = home_dir()?;
    let tmp = std::env::temp_dir();
    let tmp = tmp.canonicalize().unwrap_or(tmp);
    Ok((workspace, home, tmp))
}

/// Distinguishes concurrent sandboxed commands within one process, so
/// two of them never share (and delete) each other's profile file.
static PROFILE_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn run_seatbelt(c: &Confinement, command: &[String]) -> Result<i32> {
    let text = profile::seatbelt_profile(c)?;
    // The profile goes in a file rather than `-p` so a long workspace
    // path can't hit the argv limit.
    let seq = PROFILE_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = c
        .tmp
        .join(format!("vts-sandbox-{}-{seq}.sb", std::process::id()));
    std::fs::write(&path, &text)
        .with_context(|| format!("writing sandbox profile to {}", path.display()))?;
    let status = Command::new("sandbox-exec")
        .arg("-f")
        .arg(&path)
        .args(command)
        .current_dir(c.workspace)
        .status()
        .context("running sandbox-exec");
    std::fs::remove_file(&path).ok();
    Ok(exit_code(status?))
}

fn run_bwrap(c: &Confinement, command: &[String]) -> Result<i32> {
    let args = bubblewrap::bwrap_args(c, command)?;
    let status = Command::new("bwrap")
        .args(&args)
        .status()
        .context("running bwrap")?;
    Ok(exit_code(status))
}

/// 128 + signal is the shell convention for a signalled child; without
/// it a killed command would report success.
fn exit_code(status: std::process::ExitStatus) -> i32 {
    status.code().unwrap_or_else(|| {
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            status.signal().map_or(1, |s| 128 + s)
        }
        #[cfg(not(unix))]
        {
            1
        }
    })
}

fn home_dir() -> Result<PathBuf> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute());
    home.context("HOME is not set to an absolute path; cannot build a sandbox policy")
}

fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| is_executable(candidate))
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_command_is_refused_before_anything_runs() {
        let err = run(Path::new("/"), &[]).unwrap_err();
        assert!(err.to_string().contains("no command"));
    }

    #[test]
    fn which_finds_a_ubiquitous_binary() {
        assert!(which("sh").is_some());
        assert!(which("definitely-not-a-real-binary-xyzzy").is_none());
    }

    #[test]
    fn macos_always_has_a_mechanism() {
        // sandbox-exec ships with macOS, so the primary platform can
        // never silently fall through to the refusal path. Linux is
        // deliberately not asserted: bwrap is a package, and a
        // contributor without it should see skips, not failures.
        if cfg!(target_os = "macos") {
            assert_eq!(detect(), Some(Mechanism::Seatbelt));
        }
    }
}
