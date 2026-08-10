//! Seatbelt profile generation for macOS `sandbox-exec`.
//!
//! Pure: a `Confinement` in, SBPL text out, so the policy is unit-tested
//! without executing anything. The policy answers the audit finding that
//! verification executes commands transcribed from an untrusted video —
//! those commands run with no network, no writes outside the throwaway
//! workspace, and no reads of the machine's credential stores.
//!
//! SBPL evaluates rules in order and the LAST match wins, so the broad
//! `(allow file-read*)` needed to load dyld and shared libraries is
//! clawed back afterwards by the credential denials.

use std::fmt::Write as _;
use std::path::Path;

use anyhow::{bail, Result};

/// Directories under `$HOME` that stay unreadable inside the sandbox.
const CREDENTIAL_DIRS: &[&str] = &[
    ".ssh",
    ".aws",
    ".gnupg",
    ".docker",
    ".kube",
    ".azure",
    ".password-store",
    ".config/gh",
    ".config/gcloud",
    ".config/git",
    ".local/share/keyrings",
    "Library/Keychains",
    // agent config dirs hold API keys for the very agent running this
    ".claude",
    ".codex",
    ".agents",
];

/// Individual dotfiles under `$HOME` that stay unreadable.
const CREDENTIAL_FILES: &[&str] = &[
    ".netrc",
    ".npmrc",
    ".pypirc",
    ".git-credentials",
    ".env",
    ".gitconfig",
];

/// Absolute paths outside `$HOME` that stay unreadable.
const SYSTEM_SECRET_PATHS: &[&str] = &[
    "/Library/Keychains",
    "/private/etc/ssh",
    "/private/etc/master.passwd",
    "/private/var/db/shadow",
];

/// Character devices a normal program may write to.
const WRITABLE_DEVICES: &[&str] = &[
    "/dev/null",
    "/dev/zero",
    "/dev/random",
    "/dev/urandom",
    "/dev/tty",
    "/dev/stdout",
    "/dev/stderr",
    "/dev/dtracehelper",
];

/// What the sandboxed command may touch.
#[derive(Debug, Clone, Copy)]
pub struct Confinement<'a> {
    /// The throwaway verification workspace — the only writable tree.
    pub workspace: &'a Path,
    /// The user's home dir, used to locate the credential stores to deny.
    pub home: &'a Path,
    /// Temp dir, writable because toolchains stage files there.
    pub tmp: &'a Path,
}

/// Render the seatbelt profile for `c`.
pub fn seatbelt_profile(c: &Confinement) -> Result<String> {
    for (label, path) in [
        ("workspace", c.workspace),
        ("home", c.home),
        ("temp dir", c.tmp),
    ] {
        if !path.is_absolute() {
            bail!("{label} must be an absolute path, got {}", path.display());
        }
    }

    let mut p = String::from("(version 1)\n(deny default)\n\n");

    p.push_str("; the command must be able to start and fork\n");
    p.push_str("(allow process-exec)\n(allow process-fork)\n");
    p.push_str("(allow signal (target self))\n");
    p.push_str("(allow sysctl-read)\n(allow mach-lookup)\n(allow ipc-posix-shm)\n\n");

    p.push_str("; reads are broad so dyld and shared libraries resolve …\n");
    p.push_str("(allow file-read* file-read-metadata)\n\n");

    p.push_str("; … then clawed back: credential stores are never readable\n");
    for dir in CREDENTIAL_DIRS {
        writeln!(
            p,
            "(deny file-read* (subpath {}))",
            sbpl(&c.home.join(dir))?
        )?;
    }
    for file in CREDENTIAL_FILES {
        writeln!(
            p,
            "(deny file-read* (literal {}))",
            sbpl(&c.home.join(file))?
        )?;
    }
    for path in SYSTEM_SECRET_PATHS {
        writeln!(p, "(deny file-read* (subpath {}))", sbpl(Path::new(path))?)?;
    }

    p.push_str("\n; writes: the throwaway workspace and scratch space only\n");
    writeln!(p, "(allow file-write* (subpath {}))", sbpl(c.workspace)?)?;
    writeln!(p, "(allow file-write* (subpath {}))", sbpl(c.tmp)?)?;
    for dev in WRITABLE_DEVICES {
        writeln!(
            p,
            "(allow file-write-data file-ioctl (literal {}))",
            sbpl(Path::new(dev))?
        )?;
    }

    p.push_str("\n; nothing read in here can leave the machine\n(deny network*)\n");
    Ok(p)
}

/// Quote a path as an SBPL string literal. A path containing a quote or
/// a backslash must not be able to terminate the literal and inject
/// policy — this is the sandbox's own injection boundary.
fn sbpl(path: &Path) -> Result<String> {
    let Some(text) = path.to_str() else {
        bail!("path is not valid UTF-8: {}", path.display());
    };
    if text.contains('\n') || text.contains('\r') {
        bail!("path contains a newline, refusing to build a profile: {text}");
    }
    let escaped = text.replace('\\', r"\\").replace('"', "\\\"");
    Ok(format!("\"{escaped}\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile() -> String {
        seatbelt_profile(&Confinement {
            workspace: Path::new("/tmp/ws"),
            home: Path::new("/Users/someone"),
            tmp: Path::new("/private/var/folders/xx"),
        })
        .unwrap()
    }

    #[test]
    fn denies_by_default_and_denies_network() {
        let p = profile();
        assert!(p.contains("(deny default)"));
        assert!(p.contains("(deny network*)"));
        // no rule may hand network access back
        assert!(!p.contains("(allow network"));
    }

    #[test]
    fn only_the_workspace_and_temp_are_writable_trees() {
        let p = profile();
        assert!(p.contains(r#"(allow file-write* (subpath "/tmp/ws"))"#));
        assert!(p.contains(r#"(allow file-write* (subpath "/private/var/folders/xx"))"#));
        // the home dir as a whole is never a writable subpath
        assert!(!p.contains(r#"(allow file-write* (subpath "/Users/someone"))"#));
        assert!(!p.contains(r#"(allow file-write* (subpath "/"))"#));
    }

    #[test]
    fn credential_stores_are_unreadable_despite_broad_reads() {
        let p = profile();
        assert!(p.contains("(allow file-read* file-read-metadata)"));
        for expected in [
            r#"(deny file-read* (subpath "/Users/someone/.ssh"))"#,
            r#"(deny file-read* (subpath "/Users/someone/.aws"))"#,
            r#"(deny file-read* (subpath "/Users/someone/.config/gh"))"#,
            r#"(deny file-read* (subpath "/Users/someone/Library/Keychains"))"#,
            r#"(deny file-read* (subpath "/Users/someone/.claude"))"#,
            r#"(deny file-read* (literal "/Users/someone/.netrc"))"#,
            r#"(deny file-read* (literal "/Users/someone/.git-credentials"))"#,
            r#"(deny file-read* (subpath "/Library/Keychains"))"#,
        ] {
            assert!(p.contains(expected), "profile missing rule: {expected}");
        }
    }

    #[test]
    fn denials_come_after_the_broad_read_allowance() {
        // SBPL is last-match-wins: a denial placed before the blanket
        // allow would be silently overridden.
        let p = profile();
        let allow = p.find("(allow file-read* file-read-metadata)").unwrap();
        let deny = p
            .find(r#"(deny file-read* (subpath "/Users/someone/.ssh"))"#)
            .unwrap();
        assert!(deny > allow, "credential denials must follow the allow");
    }

    #[test]
    fn relative_paths_are_refused() {
        let err = seatbelt_profile(&Confinement {
            workspace: Path::new("relative/ws"),
            home: Path::new("/Users/someone"),
            tmp: Path::new("/tmp"),
        })
        .unwrap_err();
        assert!(err
            .to_string()
            .contains("workspace must be an absolute path"));
    }

    #[test]
    fn quotes_in_paths_cannot_escape_the_literal() {
        let p = seatbelt_profile(&Confinement {
            workspace: Path::new(r#"/tmp/we"ird"#),
            home: Path::new("/Users/someone"),
            tmp: Path::new("/tmp"),
        })
        .unwrap();
        assert!(p.contains(r#"(allow file-write* (subpath "/tmp/we\"ird"))"#));
    }

    #[test]
    fn newlines_in_paths_are_refused() {
        let err = seatbelt_profile(&Confinement {
            workspace: Path::new("/tmp/a\n(allow network*)"),
            home: Path::new("/Users/someone"),
            tmp: Path::new("/tmp"),
        })
        .unwrap_err();
        assert!(err.to_string().contains("newline"));
    }
}
