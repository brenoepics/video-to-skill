//! Bubblewrap (`bwrap`) confinement for Linux — the same policy the
//! seatbelt profile expresses on macOS: no network, writes confined to
//! the throwaway workspace, credential stores unreadable.
//!
//! Pure argv construction so the policy is unit-tested without `bwrap`
//! installed. `bwrap` applies mounts in argv order, so the read-only
//! root comes first, the home tmpfs blanks the credential stores next,
//! and the workspace bind is last so it survives a workspace that lives
//! under `$HOME`.

use std::path::Path;

use anyhow::{bail, Result};

use super::profile::Confinement;

/// Build the `bwrap` argv (excluding the `bwrap` program name itself)
/// that runs `command` under confinement.
pub fn bwrap_args(c: &Confinement, command: &[String]) -> Result<Vec<String>> {
    if command.is_empty() {
        bail!("no command given to run under the sandbox");
    }
    for (label, path) in [
        ("workspace", c.workspace),
        ("home", c.home),
        ("temp dir", c.tmp),
    ] {
        if !path.is_absolute() {
            bail!("{label} must be an absolute path, got {}", path.display());
        }
    }
    let text = |p: &Path| -> Result<String> {
        p.to_str()
            .map(ToOwned::to_owned)
            .ok_or_else(|| anyhow::anyhow!("path is not valid UTF-8: {}", p.display()))
    };

    let (workspace, home, tmp) = (text(c.workspace)?, text(c.home)?, text(c.tmp)?);
    let flat: Vec<&str> = vec![
        // no network namespace at all: nothing read in here can leave
        "--unshare-net",
        "--unshare-ipc",
        "--unshare-uts",
        "--die-with-parent",
        "--new-session",
        // everything readable, nothing writable …
        "--ro-bind",
        "/",
        "/",
        "--dev",
        "/dev",
        "--proc",
        "/proc",
        // … the home dir blanked so credential stores simply are not there …
        "--tmpfs",
        &home,
        // … and the two writable trees restored on top.
        "--bind",
        &tmp,
        &tmp,
        "--bind",
        &workspace,
        &workspace,
        "--chdir",
        &workspace,
        "--",
    ];
    let mut args: Vec<String> = flat.into_iter().map(ToOwned::to_owned).collect();
    args.extend(command.iter().cloned());
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args() -> Vec<String> {
        bwrap_args(
            &Confinement {
                workspace: Path::new("/home/u/ws"),
                home: Path::new("/home/u"),
                tmp: Path::new("/tmp"),
            },
            &["sh".to_string(), "-c".to_string(), "echo hi".to_string()],
        )
        .unwrap()
    }

    fn index_of(args: &[String], needle: &str) -> usize {
        args.iter().position(|a| a == needle).expect(needle)
    }

    #[test]
    fn network_is_unshared() {
        assert!(args().contains(&"--unshare-net".to_string()));
    }

    #[test]
    fn root_is_read_only_and_workspace_is_writable() {
        let a = args();
        let ro = index_of(&a, "--ro-bind");
        assert_eq!(a[ro + 1], "/");
        assert_eq!(a[ro + 2], "/");
        // the workspace is bound read-write
        let binds: Vec<_> = a
            .windows(3)
            .filter(|w| w[0] == "--bind")
            .map(|w| w[1].clone())
            .collect();
        assert!(binds.contains(&"/home/u/ws".to_string()));
    }

    #[test]
    fn home_is_blanked_before_the_workspace_is_restored() {
        // a workspace under $HOME must survive the tmpfs that hides
        // the credential stores — order is the whole trick here
        let a = args();
        let tmpfs = index_of(&a, "--tmpfs");
        assert_eq!(a[tmpfs + 1], "/home/u");
        let ws = a
            .windows(2)
            .position(|w| w[0] == "--bind" && w[1] == "/home/u/ws")
            .expect("workspace bind");
        assert!(ws > tmpfs, "workspace bind must come after the home tmpfs");
    }

    #[test]
    fn the_command_follows_a_double_dash() {
        let a = args();
        let sep = index_of(&a, "--");
        assert_eq!(&a[sep + 1..], &["sh", "-c", "echo hi"]);
    }

    #[test]
    fn an_empty_command_is_refused() {
        let err = bwrap_args(
            &Confinement {
                workspace: Path::new("/home/u/ws"),
                home: Path::new("/home/u"),
                tmp: Path::new("/tmp"),
            },
            &[],
        )
        .unwrap_err();
        assert!(err.to_string().contains("no command"));
    }

    #[test]
    fn relative_paths_are_refused() {
        let err = bwrap_args(
            &Confinement {
                workspace: Path::new("ws"),
                home: Path::new("/home/u"),
                tmp: Path::new("/tmp"),
            },
            &["sh".to_string()],
        )
        .unwrap_err();
        assert!(err
            .to_string()
            .contains("workspace must be an absolute path"));
    }
}
