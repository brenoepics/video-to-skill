//! Exact standard-handle allowlist for child-process inheritance.

use std::mem::size_of;

use anyhow::{Context, Result};
use windows_sys::Win32::Foundation::{
    GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE, TRUE,
};
use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows_sys::Win32::System::Console::{
    GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
};

use super::handle::OwnedHandle;

pub(super) struct StandardHandles {
    input: OwnedHandle,
    output: OwnedHandle,
    error: OwnedHandle,
}

impl StandardHandles {
    pub(super) fn inherit() -> Result<Self> {
        Ok(Self {
            input: inherited_standard(STD_INPUT_HANDLE, GENERIC_READ, "standard input")?,
            output: inherited_standard(STD_OUTPUT_HANDLE, GENERIC_WRITE, "standard output")?,
            error: inherited_standard(STD_ERROR_HANDLE, GENERIC_WRITE, "standard error")?,
        })
    }

    pub(super) fn as_array(&self) -> [HANDLE; 3] {
        [self.input.raw(), self.output.raw(), self.error.raw()]
    }
}

fn inherited_standard(which: u32, access: u32, label: &str) -> Result<OwnedHandle> {
    // SAFETY: GetStdHandle borrows a process-global handle.
    let current = unsafe { GetStdHandle(which) };
    if !current.is_null() && current != INVALID_HANDLE_VALUE {
        return OwnedHandle::duplicate_inheritable(current, &format!("duplicating {label}"));
    }

    let security = SECURITY_ATTRIBUTES {
        nLength: u32::try_from(size_of::<SECURITY_ATTRIBUTES>())
            .context("SECURITY_ATTRIBUTES size does not fit in u32")?,
        lpSecurityDescriptor: std::ptr::null_mut(),
        bInheritHandle: TRUE,
    };
    let nul = [u16::from(b'N'), u16::from(b'U'), u16::from(b'L'), 0];
    // SAFETY: the device name and security attributes are initialized.
    let handle = unsafe {
        CreateFileW(
            nul.as_ptr(),
            access,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            &raw const security,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            std::ptr::null_mut(),
        )
    };
    OwnedHandle::from_api(handle, &format!("opening NUL for {label}"))
}
