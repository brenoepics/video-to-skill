//! Cross-process serialization for managed dependency installation.

use std::fs::{File, OpenOptions, TryLockError};
use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};

const LOCK_FILE_NAME: &str = ".install.lock";
const LOCK_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const LOCK_POLL_INTERVAL: Duration = Duration::from_millis(25);

pub(crate) struct InstallLock {
    _file: File,
}

impl InstallLock {
    pub(crate) fn acquire(data_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(data_dir).with_context(|| {
            format!("creating dependency data directory {}", data_dir.display())
        })?;
        let path = data_dir.join(LOCK_FILE_NAME);
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .with_context(|| format!("opening dependency install lock {}", path.display()))?;
        let deadline = Instant::now() + LOCK_TIMEOUT;
        loop {
            match file.try_lock() {
                Ok(()) => return Ok(Self { _file: file }),
                Err(TryLockError::WouldBlock) if Instant::now() < deadline => {
                    std::thread::sleep(LOCK_POLL_INTERVAL);
                }
                Err(TryLockError::WouldBlock) => {
                    bail!(
                        "timed out waiting for dependency install lock {}",
                        path.display()
                    );
                }
                Err(TryLockError::Error(error)) => {
                    return Err(error).with_context(|| {
                        format!("locking dependency installer {}", path.display())
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;

    #[test]
    fn installers_for_the_same_data_directory_are_serialized() {
        let directory = std::env::temp_dir()
            .join("vts-install-lock-tests")
            .join(std::process::id().to_string());
        let first = InstallLock::acquire(&directory).unwrap();
        let (ready_tx, ready_rx) = mpsc::channel();
        let (acquired_tx, acquired_rx) = mpsc::channel();
        let contender = std::thread::spawn(move || {
            ready_tx.send(()).unwrap();
            let _second = InstallLock::acquire(&directory).unwrap();
            acquired_tx.send(()).unwrap();
        });
        ready_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(acquired_rx.recv_timeout(Duration::from_millis(50)).is_err());
        drop(first);
        acquired_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        contender.join().unwrap();
    }
}
