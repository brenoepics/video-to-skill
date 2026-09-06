use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{CloseHandle, FALSE, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows_sys::Win32::System::Threading::{
    OpenProcess, TerminateProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
};

const PROCESS_CLEANUP_TIMEOUT_MS: u32 = 5_000;

pub(super) struct Workspace(pub(super) PathBuf);

impl Workspace {
    pub(super) fn new(tag: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("vts-windows-sandbox-{tag}-{}", std::process::id()));
        fs::create_dir_all(&path)
            .unwrap_or_else(|error| panic!("create workspace {}: {error}", path.display()));
        Self(path)
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).ok();
    }
}

pub(super) fn command(script: &str) -> Vec<String> {
    let cmd = system_executable("cmd.exe").display().to_string();
    vec![cmd, "/d".into(), "/s".into(), "/c".into(), script.into()]
}

pub(super) fn child_test_command(test: &str) -> Vec<String> {
    let current_exe =
        std::env::current_exe().unwrap_or_else(|error| panic!("current test executable: {error}"));
    vec![
        current_exe.display().to_string(),
        "--ignored".into(),
        "--exact".into(),
        test.into(),
        "--test-threads=1".into(),
    ]
}

pub(super) fn system_executable(name: &str) -> PathBuf {
    PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| panic!("SystemRoot is unset")))
        .join("System32")
        .join(name)
}

pub(super) fn dacl(path: &Path) -> Vec<u8> {
    let output = Command::new(system_executable("icacls.exe"))
        .arg(path)
        .output()
        .unwrap_or_else(|error| panic!("inspect workspace DACL: {error}"));
    assert!(output.status.success(), "icacls failed");
    output.stdout
}

pub(super) fn protect_dacl(path: &Path) {
    let output = Command::new(system_executable("icacls.exe"))
        .arg(path)
        .arg("/inheritance:d")
        .output()
        .unwrap_or_else(|error| panic!("protect workspace DACL: {error}"));
    assert!(output.status.success(), "icacls failed: {output:?}");
}

pub(super) fn wait_for_process(path: &Path, timeout: Duration) -> io::Result<ProcessProbe> {
    let deadline = Instant::now() + timeout;
    loop {
        match fs::read_to_string(path) {
            Ok(value) => {
                if let Ok(pid) = value.trim().parse() {
                    return ProcessProbe::open(pid);
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!("timed out waiting for descendant PID in {}", path.display()),
            ));
        }
        thread::sleep(Duration::from_millis(5));
    }
}

pub(super) struct ProcessProbe(HANDLE);

impl ProcessProbe {
    fn open(pid: u32) -> io::Result<Self> {
        // SAFETY: PID comes from the live child and no handle is inherited.
        let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE | PROCESS_TERMINATE, FALSE, pid) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(handle))
    }

    pub(super) fn terminated_within(&self, timeout: Duration) -> io::Result<bool> {
        let millis = u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX - 1);
        // SAFETY: this object owns a live process handle with synchronize access.
        match unsafe { WaitForSingleObject(self.0, millis) } {
            WAIT_OBJECT_0 => Ok(true),
            WAIT_TIMEOUT => Ok(false),
            _ => Err(io::Error::last_os_error()),
        }
    }
}

impl Drop for ProcessProbe {
    fn drop(&mut self) {
        // Keep a failed Job Object assertion from leaking its probe process.
        // SAFETY: this object owns a process handle with synchronize access.
        if unsafe { WaitForSingleObject(self.0, 0) } == WAIT_TIMEOUT {
            // SAFETY: the handle has terminate access and remains open while
            // termination completes.
            unsafe {
                TerminateProcess(self.0, 1);
                WaitForSingleObject(self.0, PROCESS_CLEANUP_TIMEOUT_MS);
            }
        }
        // SAFETY: this is the final use of the owned process handle.
        unsafe {
            CloseHandle(self.0);
        }
    }
}

pub(super) struct EnvironmentVariable {
    name: &'static str,
    previous: Option<std::ffi::OsString>,
}

impl EnvironmentVariable {
    pub(super) fn set(name: &'static str, value: &str) -> Self {
        let previous = std::env::var_os(name);
        std::env::set_var(name, value);
        Self { name, previous }
    }
}

impl Drop for EnvironmentVariable {
    fn drop(&mut self) {
        if let Some(value) = &self.previous {
            std::env::set_var(self.name, value);
        } else {
            std::env::remove_var(self.name);
        }
    }
}

pub(super) struct Junction(pub(super) PathBuf);

impl Drop for Junction {
    fn drop(&mut self) {
        fs::remove_dir(&self.0).ok();
    }
}
