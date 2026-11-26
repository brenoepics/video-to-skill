//! Ephemeral `AppContainer` identity ownership.

use std::os::windows::ffi::OsStrExt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Context, Result};
use windows_sys::Win32::Security::Isolation::{
    CreateAppContainerProfile, DeleteAppContainerProfile,
};
use windows_sys::Win32::Security::{FreeSid, PSID};

static PROFILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(super) struct AppContainerProfile {
    name: Vec<u16>,
    sid: PSID,
    deleted: bool,
}

impl AppContainerProfile {
    pub(super) fn create() -> Result<Self> {
        let name = unique_name()?;
        let name_wide = wide(&name);
        let display_name = wide("video-to-skill verification sandbox");
        let description = wide("Ephemeral LPAC for untrusted video-derived commands");
        let mut sid = std::ptr::null_mut();
        // SAFETY: all strings are terminated and live for the call; zero
        // capabilities deliberately denies network capability grants.
        let result = unsafe {
            CreateAppContainerProfile(
                name_wide.as_ptr(),
                display_name.as_ptr(),
                description.as_ptr(),
                std::ptr::null(),
                0,
                &raw mut sid,
            )
        };
        if result < 0 {
            if !sid.is_null() {
                // SAFETY: the failed API nevertheless returned ownership.
                unsafe {
                    FreeSid(sid);
                }
            }
            return Err(hresult(
                "creating the ephemeral AppContainer profile",
                result,
            ));
        }
        if sid.is_null() {
            // SAFETY: creation succeeded, so the profile name is registered.
            unsafe {
                DeleteAppContainerProfile(name_wide.as_ptr());
            }
            bail!("creating the AppContainer profile returned a null SID");
        }
        Ok(Self {
            name: name_wide,
            sid,
            deleted: false,
        })
    }

    pub(super) fn sid(&self) -> PSID {
        self.sid
    }

    pub(super) fn delete(&mut self) -> Result<()> {
        if self.deleted {
            return Ok(());
        }
        // SAFETY: the nul-terminated name remains stable for this object's life.
        let result = unsafe { DeleteAppContainerProfile(self.name.as_ptr()) };
        if result < 0 {
            return Err(hresult(
                "deleting the ephemeral AppContainer profile",
                result,
            ));
        }
        self.deleted = true;
        Ok(())
    }
}

impl Drop for AppContainerProfile {
    fn drop(&mut self) {
        if !self.sid.is_null() {
            // SAFETY: CreateAppContainerProfile transferred this SID to us.
            unsafe {
                FreeSid(self.sid);
            }
            self.sid = std::ptr::null_mut();
        }
        if !self.deleted {
            // Best-effort backstop. The explicit cleanup path reports failures.
            unsafe {
                DeleteAppContainerProfile(self.name.as_ptr());
            }
        }
    }
}

fn unique_name() -> Result<String> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock predates the Unix epoch")?;
    let sequence = PROFILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    Ok(format!(
        "vts.sbx.{:x}.{:x}.{sequence:x}",
        std::process::id(),
        elapsed.as_nanos()
    ))
}

fn wide(value: &str) -> Vec<u16> {
    std::ffi::OsStr::new(value)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn hresult(operation: &str, result: i32) -> anyhow::Error {
    let code = u32::from_ne_bytes(result.to_ne_bytes());
    anyhow!("{operation} failed with HRESULT 0x{code:08X}")
}
