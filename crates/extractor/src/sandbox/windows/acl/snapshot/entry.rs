//! Race-resistant workspace entry handles used by DACL snapshots.

use std::fs::{File, OpenOptions};
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::Path;

use anyhow::{bail, Context, Result};
use windows_sys::Win32::Storage::FileSystem::{
    GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_DIRECTORY,
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, READ_CONTROL, WRITE_DAC,
};

pub(super) fn open_validated(path: &Path) -> Result<(File, bool)> {
    let file = OpenOptions::new()
        .access_mode(READ_CONTROL | WRITE_DAC)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .with_context(|| format!("opening workspace ACL handle for {}", path.display()))?;
    let mut information = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: `file` owns a valid handle and `information` is writable.
    let succeeded =
        unsafe { GetFileInformationByHandle(file.as_raw_handle(), &raw mut information) };
    if succeeded == 0 {
        return Err(std::io::Error::last_os_error())
            .with_context(|| format!("inspecting workspace entry {}", path.display()));
    }
    if information.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        bail!(
            "workspace contains a reparse point, refusing sandbox escape path: {}",
            path.display()
        );
    }
    let directory = information.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0;
    if !directory && information.nNumberOfLinks != 1 {
        bail!(
            "workspace contains a hard link, refusing ambiguous file boundary: {}",
            path.display()
        );
    }
    Ok((file, directory))
}
