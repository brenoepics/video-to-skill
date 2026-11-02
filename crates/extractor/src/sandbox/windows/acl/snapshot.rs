//! Handle-backed DACL snapshots for one disposable workspace tree.

mod entry;
#[cfg(test)]
mod tests;

use std::fs::File;
use std::mem::size_of;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use windows_sys::Win32::Foundation::{ERROR_SUCCESS, WIN32_ERROR};
use windows_sys::Win32::Security::Authorization::{
    GetSecurityInfo, SetEntriesInAclW, SetSecurityInfo, EXPLICIT_ACCESS_W, GRANT_ACCESS,
    SE_FILE_OBJECT, TRUSTEE_IS_SID, TRUSTEE_IS_UNKNOWN,
};
use windows_sys::Win32::Security::{
    GetSecurityDescriptorControl, GetSecurityDescriptorDacl, IsValidAcl, ACL,
    CONTAINER_INHERIT_ACE, DACL_SECURITY_INFORMATION, OBJECT_INHERIT_ACE,
    PROTECTED_DACL_SECURITY_INFORMATION, PSID, SECURITY_DESCRIPTOR_CONTROL, SE_DACL_PROTECTED,
    UNPROTECTED_DACL_SECURITY_INFORMATION,
};
use windows_sys::Win32::Storage::FileSystem::{
    DELETE, FILE_GENERIC_EXECUTE, FILE_GENERIC_READ, FILE_GENERIC_WRITE,
};

use self::entry::open_validated;
use super::super::handle::LocalAllocation;

const WORKSPACE_PERMISSIONS: u32 =
    FILE_GENERIC_READ | FILE_GENERIC_WRITE | FILE_GENERIC_EXECUTE | DELETE;

pub(super) struct DaclSnapshot {
    path: PathBuf,
    handle: File,
    original: SecurityState,
    directory: bool,
}

impl DaclSnapshot {
    pub(super) fn capture_tree(root: &Path) -> Result<Vec<Self>> {
        let mut snapshots = Vec::new();
        let mut pending = vec![root.to_path_buf()];
        while let Some(path) = pending.pop() {
            let (handle, directory) = open_validated(&path)?;
            let original = read_security(&handle, &path)?;
            original.ensure_restorable(&path)?;
            if directory {
                let mut children = std::fs::read_dir(&path)
                    .with_context(|| format!("enumerating workspace directory {}", path.display()))?
                    .collect::<std::io::Result<Vec<_>>>()?;
                children.sort_by_key(std::fs::DirEntry::file_name);
                pending.extend(children.into_iter().rev().map(|entry| entry.path()));
            }
            snapshots.push(Self {
                path,
                handle,
                original,
                directory,
            });
        }
        Ok(snapshots)
    }

    pub(super) fn grant(&self, sid: PSID) -> Result<()> {
        let current = read_security(&self.handle, &self.path)?;
        let Some(current_dacl) = current.dacl.as_acl() else {
            // A NULL DACL already grants access to every SID. Preserve it in
            // the snapshot and avoid converting it merely to add this SID.
            return Ok(());
        };
        let inheritance = if self.directory {
            CONTAINER_INHERIT_ACE | OBJECT_INHERIT_ACE
        } else {
            0
        };
        let mut access = EXPLICIT_ACCESS_W {
            grfAccessPermissions: WORKSPACE_PERMISSIONS,
            grfAccessMode: GRANT_ACCESS,
            grfInheritance: inheritance,
            ..Default::default()
        };
        access.Trustee.TrusteeForm = TRUSTEE_IS_SID;
        access.Trustee.TrusteeType = TRUSTEE_IS_UNKNOWN;
        access.Trustee.ptstrName = sid.cast();

        let mut updated_dacl = std::ptr::null_mut();
        // SAFETY: the copied ACL and explicit entry remain valid for this call.
        let status =
            unsafe { SetEntriesInAclW(1, &raw const access, current_dacl, &raw mut updated_dacl) };
        win32(status, "building the temporary workspace DACL")?;
        let updated = LocalAllocation::new(updated_dacl.cast());
        if updated.raw().is_null() {
            bail!("building the temporary workspace DACL returned a null ACL");
        }
        // SAFETY: the handle was opened with WRITE_DAC and SetSecurityInfo
        // copies the ACL before returning.
        let status = unsafe {
            SetSecurityInfo(
                self.handle.as_raw_handle(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                updated_dacl,
                std::ptr::null(),
            )
        };
        win32(
            status,
            &format!("granting AppContainer access to {}", self.path.display()),
        )
    }

    pub(super) fn restore(&self) -> Result<()> {
        if self.original.protected {
            set_dacl(&self.handle, &self.original.dacl, true, &self.path)?;
        } else {
            // Install the exact bytes without inheritance changing them, then
            // re-enable inheritance after the parent has already been restored.
            set_dacl(&self.handle, &self.original.dacl, true, &self.path)?;
            set_dacl(&self.handle, &self.original.dacl, false, &self.path)?;
        }
        self.verify()
    }

    pub(super) fn verify(&self) -> Result<()> {
        let restored = read_security(&self.handle, &self.path)?;
        if restored == self.original {
            return Ok(());
        }
        bail!(
            "workspace DACL was not restored exactly for {}",
            self.path.display()
        )
    }
}

#[derive(PartialEq, Eq)]
struct SecurityState {
    dacl: SavedDacl,
    protected: bool,
}

impl SecurityState {
    fn ensure_restorable(&self, path: &Path) -> Result<()> {
        if matches!(&self.dacl, SavedDacl::Null) && !self.protected {
            bail!(
                "workspace entry has an inheritable NULL DACL; refusing a temporary access mutation: {}",
                path.display()
            );
        }
        Ok(())
    }
}

#[derive(PartialEq, Eq)]
enum SavedDacl {
    Null,
    Acl { words: Vec<u32>, byte_len: usize },
}

impl SavedDacl {
    fn as_acl(&self) -> Option<*const ACL> {
        match self {
            Self::Null => None,
            Self::Acl { words, .. } => Some(words.as_ptr().cast()),
        }
    }

    fn pointer(&self) -> *const ACL {
        self.as_acl().unwrap_or(std::ptr::null())
    }
}

fn read_security(handle: &File, path: &Path) -> Result<SecurityState> {
    let mut dacl = std::ptr::null_mut();
    let mut descriptor = std::ptr::null_mut();
    // SAFETY: output pointers are writable; the returned descriptor owns DACL.
    let status = unsafe {
        GetSecurityInfo(
            handle.as_raw_handle(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &raw mut dacl,
            std::ptr::null_mut(),
            &raw mut descriptor,
        )
    };
    win32(
        status,
        &format!("reading workspace DACL for {}", path.display()),
    )?;
    let descriptor = LocalAllocation::new(descriptor);
    if descriptor.raw().is_null() {
        bail!("reading workspace DACL returned a null security descriptor");
    }

    let mut present = 0;
    let mut descriptor_dacl = std::ptr::null_mut();
    let mut defaulted = 0;
    // SAFETY: the LocalAlloc-owned security descriptor remains alive.
    if unsafe {
        GetSecurityDescriptorDacl(
            descriptor.raw(),
            &raw mut present,
            &raw mut descriptor_dacl,
            &raw mut defaulted,
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error()).context("reading DACL presence flag");
    }
    if present == 0 {
        bail!(
            "workspace entry has no DACL; refusing a temporary access mutation: {}",
            path.display()
        );
    }
    debug_assert_eq!(dacl, descriptor_dacl);

    let mut control: SECURITY_DESCRIPTOR_CONTROL = 0;
    let mut revision = 0;
    // SAFETY: the security descriptor and both output pointers are valid.
    if unsafe {
        GetSecurityDescriptorControl(descriptor.raw(), &raw mut control, &raw mut revision)
    } == 0
    {
        return Err(std::io::Error::last_os_error()).context("reading DACL control flags");
    }
    Ok(SecurityState {
        dacl: copy_dacl(descriptor_dacl)?,
        protected: control & SE_DACL_PROTECTED != 0,
    })
}

fn copy_dacl(dacl: *mut ACL) -> Result<SavedDacl> {
    if dacl.is_null() {
        return Ok(SavedDacl::Null);
    }
    // SAFETY: GetSecurityInfo supplied an ACL within its live descriptor.
    if unsafe { IsValidAcl(dacl) } == 0 {
        bail!("Windows returned an invalid workspace DACL");
    }
    // SAFETY: a valid ACL contains its byte length in the fixed header.
    let byte_len = usize::from(unsafe { (*dacl).AclSize });
    if byte_len < size_of::<ACL>() {
        bail!("Windows returned a truncated workspace DACL");
    }
    let mut words = vec![0_u32; byte_len.div_ceil(size_of::<u32>())];
    // SAFETY: both allocations are valid for `byte_len` non-overlapping bytes.
    unsafe {
        std::ptr::copy_nonoverlapping(dacl.cast::<u8>(), words.as_mut_ptr().cast(), byte_len);
    }
    Ok(SavedDacl::Acl { words, byte_len })
}

fn set_dacl(handle: &File, dacl: &SavedDacl, protected: bool, path: &Path) -> Result<()> {
    let inheritance = if protected {
        PROTECTED_DACL_SECURITY_INFORMATION
    } else {
        UNPROTECTED_DACL_SECURITY_INFORMATION
    };
    // SAFETY: the handle has WRITE_DAC and the aligned ACL copy remains alive.
    let status = unsafe {
        SetSecurityInfo(
            handle.as_raw_handle(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | inheritance,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            dacl.pointer(),
            std::ptr::null(),
        )
    };
    win32(
        status,
        &format!("restoring workspace DACL for {}", path.display()),
    )
}

fn win32(status: WIN32_ERROR, operation: &str) -> Result<()> {
    if status == ERROR_SUCCESS {
        return Ok(());
    }
    let raw = i32::try_from(status).unwrap_or(i32::MAX);
    Err(std::io::Error::from_raw_os_error(raw)).context(operation.to_owned())
}
