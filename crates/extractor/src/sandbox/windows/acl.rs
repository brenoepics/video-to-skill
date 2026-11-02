//! Transactional DACL grant for the disposable workspace.

mod snapshot;

use std::path::Path;

use anyhow::{bail, Result};
use windows_sys::Win32::Security::PSID;

use self::snapshot::DaclSnapshot;

pub(super) struct WorkspaceAccess {
    snapshots: Vec<DaclSnapshot>,
    active: bool,
}

impl WorkspaceAccess {
    pub(super) fn grant(workspace: &Path, sid: PSID) -> Result<Self> {
        if sid.is_null() {
            bail!("cannot grant workspace access to a null AppContainer SID");
        }
        // Capture every original DACL before the first inheritable grant can
        // propagate into the tree. Each snapshot retains a handle, so cleanup
        // cannot be redirected by replacing a path with a junction or hardlink.
        let snapshots = DaclSnapshot::capture_tree(workspace)?;
        let mut access = Self {
            snapshots,
            active: true,
        };
        for index in 0..access.snapshots.len() {
            if let Err(error) = access.snapshots[index].grant(sid) {
                let cleanup = access.restore();
                return match cleanup {
                    Ok(()) => Err(error),
                    Err(cleanup) => Err(error.context(format!(
                        "also failed to restore a partial workspace grant: {cleanup:#}"
                    ))),
                };
            }
        }
        Ok(access)
    }

    pub(super) fn restore(&mut self) -> Result<()> {
        if !self.active {
            return Ok(());
        }
        let mut failure = None;
        // Snapshots are parent-first. Restoring in that order lets Windows
        // remove inherited temporary ACEs before each child's exact restore.
        for snapshot in &self.snapshots {
            if let Err(error) = snapshot.restore() {
                failure.get_or_insert(error);
            }
        }
        for snapshot in &self.snapshots {
            if let Err(error) = snapshot.verify() {
                failure.get_or_insert(error);
            }
        }
        if let Some(error) = failure {
            return Err(error);
        }
        self.active = false;
        Ok(())
    }
}

impl Drop for WorkspaceAccess {
    fn drop(&mut self) {
        self.restore().ok();
    }
}
