//! Filesystem operations whose semantics differ between Unix and Windows.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Write a complete sibling file, then atomically replace `path` with it.
///
/// The sibling keeps promotion on one filesystem. A unique name also keeps
/// concurrent writers from truncating one another's staging file.
pub(crate) fn write_atomic(path: &Path, contents: &[u8]) -> io::Result<()> {
    let (temporary, mut file) = create_temporary_sibling(path)?;
    let written = file.write_all(contents).and_then(|()| file.sync_all());
    drop(file);
    let outcome = written.and_then(|()| replace_file(&temporary, path));
    if outcome.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    outcome
}

fn create_temporary_sibling(path: &Path) -> io::Result<(PathBuf, fs::File)> {
    loop {
        let temporary = temporary_sibling(path)?;
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => return Ok((temporary, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
}

fn temporary_sibling(path: &Path) -> io::Result<PathBuf> {
    let name = path.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "atomic-write destination has no file name",
        )
    })?;
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temporary_name = format!(
        ".{}.{}.{}.tmp",
        name.to_string_lossy(),
        std::process::id(),
        sequence
    );
    Ok(path.with_file_name(temporary_name))
}

#[cfg(not(windows))]
fn replace_file(source: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(windows)]
fn replace_file(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;

    #[link(name = "Kernel32")]
    unsafe extern "system" {
        #[link_name = "MoveFileExW"]
        fn move_file_ex_w(
            existing_file_name: *const u16,
            new_file_name: *const u16,
            flags: u32,
        ) -> i32;
    }

    fn wide(path: &Path) -> io::Result<Vec<u16>> {
        let mut encoded: Vec<u16> = path.as_os_str().encode_wide().collect();
        if encoded.contains(&0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Windows path contains an embedded NUL",
            ));
        }
        encoded.push(0);
        Ok(encoded)
    }

    let source = wide(source)?;
    let destination = wide(destination)?;
    // SAFETY: both buffers are NUL-terminated and remain alive for the call;
    // the flags request a same-volume replace of the sibling staging file.
    let replaced = unsafe {
        move_file_ex_w(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if replaced == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
