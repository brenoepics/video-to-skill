//! Safe conversion from canonical verbatim paths to local Win32 disk paths.

use std::ffi::OsString;
use std::fs::OpenOptions;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::{Component, Path, PathBuf, Prefix};

use anyhow::{bail, Context, Result};
use windows_sys::Win32::Storage::FileSystem::{
    GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, FILE_READ_ATTRIBUTES,
    FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
};
use windows_sys::Win32::System::SystemInformation::GetWindowsDirectoryW;

const INITIAL_WINDOWS_DIRECTORY_UNITS: usize = 260;

pub(super) fn ensure_single_link(path: &Path) -> Result<()> {
    let file = OpenOptions::new()
        .access_mode(FILE_READ_ATTRIBUTES)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .open(path)
        .with_context(|| format!("opening {} to inspect its link count", path.display()))?;
    let mut information = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: `file` owns a valid handle and `information` is writable for the
    // duration of the call.
    let succeeded =
        unsafe { GetFileInformationByHandle(file.as_raw_handle(), &raw mut information) };
    if succeeded == 0 {
        return Err(std::io::Error::last_os_error())
            .with_context(|| format!("reading the link count for {}", path.display()));
    }
    if information.nNumberOfLinks != 1 {
        bail!(
            "workspace contains a hard link, refusing ambiguous file boundary: {}",
            path.display()
        );
    }
    Ok(())
}

pub(super) fn windows_directory() -> Result<PathBuf> {
    let mut buffer = vec![0_u16; INITIAL_WINDOWS_DIRECTORY_UNITS];
    loop {
        let capacity =
            u32::try_from(buffer.len()).context("Windows directory buffer is too large")?;
        // SAFETY: `buffer` is writable for `capacity` UTF-16 units.
        let written = unsafe { GetWindowsDirectoryW(buffer.as_mut_ptr(), capacity) };
        if written == 0 {
            return Err(std::io::Error::last_os_error()).context("locating the Windows directory");
        }
        let written = usize::try_from(written).context("Windows directory length overflow")?;
        if written < buffer.len() {
            buffer.truncate(written);
            return Ok(PathBuf::from(OsString::from_wide(&buffer)));
        }
        buffer.resize(written.saturating_add(1), 0);
    }
}

pub(super) fn local_disk(path: &Path, label: &str) -> Result<PathBuf> {
    let Some(Component::Prefix(prefix)) = path.components().next() else {
        bail!(
            "{label} is not an absolute Windows disk path: {}",
            path.display()
        );
    };
    match prefix.kind() {
        Prefix::Disk(_) => Ok(path.to_path_buf()),
        Prefix::VerbatimDisk(_) => strip_verbatim_prefix(path, label),
        _ => bail!(
            "{label} must be on a local disk; UNC and device paths are refused: {}",
            path.display()
        ),
    }
}

fn strip_verbatim_prefix(path: &Path, label: &str) -> Result<PathBuf> {
    let encoded: Vec<_> = path.as_os_str().encode_wide().collect();
    let prefix = [
        u16::from(b'\\'),
        u16::from(b'\\'),
        u16::from(b'?'),
        u16::from(b'\\'),
    ];
    if !encoded.starts_with(&prefix) || encoded.len() == prefix.len() {
        bail!("invalid canonical {label} path: {}", path.display());
    }
    Ok(PathBuf::from(OsString::from_wide(&encoded[prefix.len()..])))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_the_verbatim_prefix_from_a_local_disk_path() {
        assert_eq!(
            local_disk(Path::new(r"\\?\C:\work\sandbox"), "workspace").unwrap(),
            Path::new(r"C:\work\sandbox")
        );
    }

    #[test]
    fn refuses_unc_paths() {
        assert!(local_disk(Path::new(r"\\server\share\sandbox"), "workspace").is_err());
    }
}
