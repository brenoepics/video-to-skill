//! Ownership wrappers for Win32 handles and local allocations.

use std::ffi::c_void;

use anyhow::{Context, Result};
use windows_sys::Win32::Foundation::{
    CloseHandle, DuplicateHandle, LocalFree, DUPLICATE_SAME_ACCESS, HANDLE, INVALID_HANDLE_VALUE,
    TRUE,
};
use windows_sys::Win32::System::Threading::GetCurrentProcess;

pub(super) struct OwnedHandle(HANDLE);

impl OwnedHandle {
    pub(super) fn from_api(handle: HANDLE, operation: &str) -> Result<Self> {
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            return Err(std::io::Error::last_os_error()).context(operation.to_owned());
        }
        Ok(Self(handle))
    }

    pub(super) fn duplicate_inheritable(handle: HANDLE, operation: &str) -> Result<Self> {
        let mut duplicate = std::ptr::null_mut();
        // SAFETY: the pseudo process handle is valid, `duplicate` is writable,
        // and the caller supplies an open source handle.
        let succeeded = unsafe {
            let process = GetCurrentProcess();
            DuplicateHandle(
                process,
                handle,
                process,
                &raw mut duplicate,
                0,
                TRUE,
                DUPLICATE_SAME_ACCESS,
            )
        };
        if succeeded == 0 {
            return Err(std::io::Error::last_os_error()).context(operation.to_owned());
        }
        Self::from_api(duplicate, operation)
    }

    pub(super) fn raw(&self) -> HANDLE {
        self.0
    }
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: this wrapper uniquely owns a non-null, non-invalid handle.
        unsafe {
            CloseHandle(self.0);
        }
    }
}

pub(super) struct LocalAllocation(*mut c_void);

impl LocalAllocation {
    pub(super) fn new(pointer: *mut c_void) -> Self {
        Self(pointer)
    }

    pub(super) fn raw(&self) -> *mut c_void {
        self.0
    }
}

impl Drop for LocalAllocation {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: Win32 returned this pointer with LocalAlloc ownership.
            unsafe {
                LocalFree(self.0);
            }
        }
    }
}
