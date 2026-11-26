//! Process names through a Toolhelp32 snapshot — the counterpart of the
//! macOS backend's `pgrep -x`.

use ::windows::Win32::Foundation::{CloseHandle, HANDLE};
use ::windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use anyhow::{Context, Result};

use super::com::failed;
use super::mapping::{is_process_named, wide_to_string};

/// Closes the snapshot handle however the walk ends.
struct Snapshot(HANDLE);

impl Drop for Snapshot {
    fn drop(&mut self) {
        // SAFETY: the handle came from CreateToolhelp32Snapshot and is
        // closed exactly once. A failed close leaks one handle, nothing
        // worse, so its result is dropped.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

/// Every running process as `(pid, executable name)`.
fn processes() -> Result<Vec<(i32, String)>> {
    // SAFETY: a process snapshot has no preconditions; the handle is
    // owned by the guard below.
    let snapshot = Snapshot(
        unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }
            .map_err(failed("listing running processes"))?,
    );
    let mut entry = PROCESSENTRY32W {
        dwSize: u32::try_from(size_of::<PROCESSENTRY32W>())
            .context("PROCESSENTRY32W does not fit its own size field")?,
        ..PROCESSENTRY32W::default()
    };
    let mut found = Vec::new();
    // SAFETY: `entry` is a valid, correctly sized out-parameter for the
    // whole walk; the walk ends at the first failure (ERROR_NO_MORE_FILES).
    let mut more = unsafe { Process32FirstW(snapshot.0, &raw mut entry) }.is_ok();
    while more {
        if let Ok(pid) = i32::try_from(entry.th32ProcessID) {
            found.push((pid, wide_to_string(&entry.szExeFile)));
        }
        // SAFETY: as above.
        more = unsafe { Process32NextW(snapshot.0, &raw mut entry) }.is_ok();
    }
    Ok(found)
}

/// The pids of every running process whose executable is `name`
/// (`notepad` or `Notepad.exe`, case ignored).
pub fn pids_named(name: &str) -> Result<Vec<i32>> {
    Ok(processes()?
        .into_iter()
        .filter(|(_, exe)| is_process_named(exe, name))
        .map(|(pid, _)| pid)
        .collect())
}

/// The executable name of `pid`, if it is still running.
pub(super) fn name_of(pid: i32) -> Result<Option<String>> {
    Ok(processes()?
        .into_iter()
        .find(|(candidate, _)| *candidate == pid)
        .map(|(_, exe)| exe))
}
