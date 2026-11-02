//! Native Windows confinement based on LPAC and Job Objects.

mod acl;
mod command;
mod environment;
mod handle;
mod job;
mod lock;
mod path;
mod process;
mod profile;
mod stdio;

use std::os::windows::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

use self::acl::WorkspaceAccess;
use self::command::ResolvedCommand;
use self::profile::AppContainerProfile;

/// Run a command under the native Windows boundary.
pub(super) fn run(workspace: &Path, command: &[String]) -> Result<i32> {
    let workspace = path::local_disk(workspace, "workspace")?;
    let _workspace_lock = lock::WorkspaceLock::acquire(&workspace)?;
    validate_workspace(&workspace)?;
    let command = ResolvedCommand::resolve(&workspace, command)?;
    let mut profile = AppContainerProfile::create()?;
    reject_reparse_points(&workspace)?;
    let mut access = match WorkspaceAccess::grant(&workspace, profile.sid()) {
        Ok(access) => access,
        Err(error) => {
            let cleanup = profile.delete();
            return combine_error(error, cleanup, "AppContainer profile cleanup");
        }
    };

    let execution = process::run(&workspace, profile.sid(), command);
    let access_cleanup = access.restore();
    let profile_cleanup = profile.delete();
    finish(execution, access_cleanup, profile_cleanup)
}

/// Render the security-relevant Windows policy in an auditable form.
pub(super) fn policy_text(workspace: &Path) -> Result<String> {
    let workspace = path::local_disk(workspace, "workspace")?;
    Ok(format!(
        "Windows sandbox policy\n\
         identity: Less-Privileged AppContainer (LPAC)\n\
         network capabilities: none\n\
         writable host tree: {}\n\
         private storage: ephemeral AppContainer profile\n\
         environment: allowlist\n\
         process handles: standard streams only\n\
         job: kill-on-close\n\
         failure mode: closed",
        workspace.display()
    ))
}

fn validate_workspace(workspace: &Path) -> Result<()> {
    if !workspace.is_absolute() || !workspace.is_dir() {
        bail!(
            "Windows sandbox workspace must be an absolute existing directory: {}",
            workspace.display()
        );
    }
    let normal_components = workspace
        .components()
        .filter(|component| matches!(component, Component::Normal(_)))
        .count();
    if normal_components < 2 {
        bail!(
            "refusing broad Windows sandbox workspace {}",
            workspace.display()
        );
    }
    for protected in protected_roots()? {
        if protected.starts_with(workspace) {
            bail!(
                "workspace {} contains protected host directory {}",
                workspace.display(),
                protected.display()
            );
        }
    }
    reject_reparse_points(workspace)
}

fn protected_roots() -> Result<Vec<PathBuf>> {
    let home = dirs::home_dir().context("Windows user profile directory is unavailable")?;
    let system = path::windows_directory()?;
    [home, std::env::temp_dir(), system]
        .into_iter()
        .map(|path| {
            let canonical = path
                .canonicalize()
                .with_context(|| format!("resolving protected directory {}", path.display()))?;
            self::path::local_disk(&canonical, "protected directory")
        })
        .collect()
}

fn reject_reparse_points(workspace: &Path) -> Result<()> {
    let mut pending = vec![workspace.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let entries = std::fs::read_dir(&directory)
            .with_context(|| format!("inspecting workspace {}", directory.display()))?;
        for entry in entries {
            let entry = entry.with_context(|| {
                format!("reading an entry beneath workspace {}", workspace.display())
            })?;
            let metadata = std::fs::symlink_metadata(entry.path())
                .with_context(|| format!("inspecting {}", entry.path().display()))?;
            if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                bail!(
                    "workspace contains a reparse point, refusing sandbox escape path: {}",
                    entry.path().display()
                );
            }
            if metadata.is_file() {
                path::ensure_single_link(&entry.path())?;
            }
            if metadata.is_dir() {
                pending.push(entry.path());
            }
        }
    }
    Ok(())
}

fn finish(
    execution: Result<i32>,
    access_cleanup: Result<()>,
    profile_cleanup: Result<()>,
) -> Result<i32> {
    let result = match access_cleanup {
        Ok(()) => execution,
        Err(cleanup) => match execution {
            Ok(_) => Err(cleanup.context("restoring sandbox workspace access")),
            Err(error) => combine_error(error, Err(cleanup), "workspace ACL cleanup"),
        },
    };
    match profile_cleanup {
        Ok(()) => result,
        Err(cleanup) => match result {
            Ok(_) => Err(cleanup.context("removing the sandbox identity")),
            Err(error) => combine_error(error, Err(cleanup), "AppContainer profile cleanup"),
        },
    }
}

fn combine_error(error: anyhow::Error, cleanup: Result<()>, label: &str) -> Result<i32> {
    match cleanup {
        Ok(()) => Err(error),
        Err(cleanup) => Err(anyhow!(
            "{error:#}; additionally, {label} failed: {cleanup:#}"
        )),
    }
}
