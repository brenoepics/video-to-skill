//! Cross-process serialization for one workspace ACL transaction.

use std::path::Path;

use anyhow::{bail, Context, Result};
use windows_sys::Win32::Foundation::{WAIT_ABANDONED, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows_sys::Win32::System::Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject};

use super::handle::OwnedHandle;

const LOCK_TIMEOUT_MS: u32 = 6 * 60 * 1_000;

pub(super) struct WorkspaceLock {
    handle: OwnedHandle,
}

impl WorkspaceLock {
    pub(super) fn acquire(workspace: &Path) -> Result<Self> {
        let name = mutex_name(workspace);
        // SAFETY: the optional security descriptor is null and `name` is a
        // terminated UTF-16 buffer that remains alive for the call.
        let raw = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
        let handle = OwnedHandle::from_api(raw, "creating the workspace sandbox mutex")?;
        // SAFETY: the mutex handle is open with synchronization access.
        match unsafe { WaitForSingleObject(handle.raw(), LOCK_TIMEOUT_MS) } {
            WAIT_OBJECT_0 | WAIT_ABANDONED => Ok(Self { handle }),
            WAIT_TIMEOUT => bail!(
                "timed out after {LOCK_TIMEOUT_MS} ms waiting for another sandbox run in {}",
                workspace.display()
            ),
            _ => Err(std::io::Error::last_os_error())
                .context("waiting for the workspace sandbox mutex"),
        }
    }
}

impl Drop for WorkspaceLock {
    fn drop(&mut self) {
        // SAFETY: successful construction means this thread owns the mutex.
        unsafe {
            ReleaseMutex(self.handle.raw());
        }
    }
}

fn mutex_name(workspace: &Path) -> Vec<u16> {
    let mut path_bytes = Vec::new();
    for unit in workspace
        .as_os_str()
        .to_string_lossy()
        .to_lowercase()
        .encode_utf16()
    {
        path_bytes.extend_from_slice(&unit.to_le_bytes());
    }
    let digest = crate::hashing::sha256_hex(&path_bytes);
    format!(r"Local\video-to-skill-sandbox-{digest}")
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    use super::WorkspaceLock;

    #[test]
    fn the_same_workspace_is_serialized_across_threads() {
        let workspace =
            std::env::temp_dir().join(format!("vts-workspace-lock-test-{}", std::process::id()));
        let first = WorkspaceLock::acquire(&workspace).unwrap();
        let (ready_tx, ready_rx) = mpsc::channel();
        let (acquired_tx, acquired_rx) = mpsc::channel();
        let contender = std::thread::spawn(move || {
            ready_tx.send(()).unwrap();
            let _lock = WorkspaceLock::acquire(&workspace).unwrap();
            acquired_tx.send(()).unwrap();
        });
        ready_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(
            acquired_rx.recv_timeout(Duration::from_millis(50)).is_err(),
            "a second run entered the same ACL transaction"
        );
        drop(first);
        acquired_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        contender.join().unwrap();
    }
}
