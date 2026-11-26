//! Job Object ownership and process-tree shutdown.

use std::mem::size_of_val;
use std::time::{Duration, Instant};

use super::handle::OwnedHandle;
use anyhow::{Context, Result};
use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectBasicAccountingInformation,
    JobObjectExtendedLimitInformation, QueryInformationJobObject, SetInformationJobObject,
    TerminateJobObject, JOBOBJECT_BASIC_ACCOUNTING_INFORMATION,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};

const TERMINATION_EXIT_CODE: u32 = 1;
const DRAIN_TIMEOUT: Duration = Duration::from_secs(10);
const DRAIN_POLL_INTERVAL: Duration = Duration::from_millis(1);

pub(super) struct Job {
    handle: OwnedHandle,
}

impl Job {
    pub(super) fn create() -> Result<Self> {
        // SAFETY: null arguments request an unnamed job with default security.
        let raw = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        let handle = OwnedHandle::from_api(raw, "creating the sandbox Job Object")?;
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let length = u32::try_from(size_of_val(&limits))
            .context("Job Object limits size does not fit in u32")?;
        // SAFETY: `limits` matches JobObjectExtendedLimitInformation.
        let succeeded = unsafe {
            SetInformationJobObject(
                handle.raw(),
                JobObjectExtendedLimitInformation,
                (&raw const limits).cast(),
                length,
            )
        };
        if succeeded == 0 {
            return Err(std::io::Error::last_os_error())
                .context("enabling kill-on-close on the sandbox Job Object");
        }
        Ok(Self { handle })
    }

    pub(super) fn assign(&self, process: HANDLE) -> Result<()> {
        // SAFETY: both handles are open and the process is still suspended.
        let succeeded = unsafe { AssignProcessToJobObject(self.handle.raw(), process) };
        if succeeded == 0 {
            return Err(std::io::Error::last_os_error())
                .context("assigning the suspended process to the sandbox Job Object");
        }
        Ok(())
    }

    pub(super) fn terminate_remaining(&self) -> Result<()> {
        // Descendants may never outlive the command whose exit code is returned.
        // SAFETY: this object owns a Job handle with terminate access.
        let succeeded = unsafe { TerminateJobObject(self.handle.raw(), TERMINATION_EXIT_CODE) };
        if succeeded == 0 {
            return Err(std::io::Error::last_os_error())
                .context("terminating the sandbox process tree");
        }
        let deadline = Instant::now() + DRAIN_TIMEOUT;
        loop {
            let mut accounting = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
            let length = u32::try_from(size_of_val(&accounting))
                .context("Job Object accounting size does not fit in u32")?;
            // SAFETY: the output buffer matches the selected information class.
            let queried = unsafe {
                QueryInformationJobObject(
                    self.handle.raw(),
                    JobObjectBasicAccountingInformation,
                    (&raw mut accounting).cast(),
                    length,
                    std::ptr::null_mut(),
                )
            };
            if queried == 0 {
                return Err(std::io::Error::last_os_error())
                    .context("checking sandbox process-tree termination");
            }
            if accounting.ActiveProcesses == 0 {
                return Ok(());
            }
            if Instant::now() >= deadline {
                anyhow::bail!(
                    "sandbox process tree did not terminate within {} ms",
                    DRAIN_TIMEOUT.as_millis()
                );
            }
            std::thread::sleep(DRAIN_POLL_INTERVAL);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::windows::io::AsRawHandle;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    use super::super::handle::OwnedHandle;
    use super::Job;
    use windows_sys::Win32::Foundation::WAIT_OBJECT_0;
    use windows_sys::Win32::Storage::FileSystem::SYNCHRONIZE;
    use windows_sys::Win32::System::Threading::{OpenProcess, WaitForSingleObject};

    const TEST_TIMEOUT: Duration = Duration::from_secs(5);
    const TERMINATION_TIMEOUT_MS: u32 = 1_000;
    const TEST_DIRECTORY_ENV: &str = "VTS_JOB_TEST_DIRECTORY";

    #[test]
    fn termination_reaps_descendants_of_an_assigned_process() {
        let directory = test_directory();
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();
        let job = Job::create().unwrap();
        let mut parent = Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "sandbox::windows::job::tests::descendant_parent",
                "--test-threads=1",
            ])
            .env(TEST_DIRECTORY_ENV, &directory)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        wait_for_file(&directory.join("parent-ready"));
        if let Err(error) = job.assign(parent.as_raw_handle()) {
            parent.kill().ok();
            parent.wait().ok();
            panic!("assign test process to Job Object: {error:#}");
        }
        fs::write(directory.join("spawn-release"), b"release").unwrap();
        let pid_path = directory.join("descendant-pid");
        wait_for_file(&pid_path);
        let pid = fs::read_to_string(pid_path).unwrap().parse().unwrap();
        let descendant = open_for_synchronization(pid);
        assert!(parent.wait().unwrap().success());

        job.terminate_remaining().unwrap();
        // SAFETY: the owned process handle has SYNCHRONIZE access.
        assert_eq!(
            unsafe { WaitForSingleObject(descendant.raw(), TERMINATION_TIMEOUT_MS) },
            WAIT_OBJECT_0,
            "a descendant survived Job Object termination"
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    #[ignore = "runs only as the assigned parent in the Job Object unit test"]
    #[allow(clippy::zombie_processes)]
    fn descendant_parent() {
        let directory = PathBuf::from(std::env::var_os(TEST_DIRECTORY_ENV).unwrap());
        fs::write(directory.join("parent-ready"), b"ready").unwrap();
        wait_for_file(&directory.join("spawn-release"));
        let _descendant = Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "sandbox::windows::job::tests::long_lived_descendant",
                "--test-threads=1",
            ])
            .env(TEST_DIRECTORY_ENV, directory)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        wait_for_file(&test_directory().join("descendant-pid"));
    }

    #[test]
    #[ignore = "runs only as a descendant in the Job Object unit test"]
    fn long_lived_descendant() {
        fs::write(
            test_directory().join("descendant-pid"),
            std::process::id().to_string(),
        )
        .unwrap();
        loop {
            std::thread::sleep(Duration::from_secs(1));
        }
    }

    fn test_directory() -> PathBuf {
        std::env::var_os(TEST_DIRECTORY_ENV).map_or_else(
            || {
                std::env::temp_dir()
                    .join("vts-job-tests")
                    .join(std::process::id().to_string())
            },
            PathBuf::from,
        )
    }

    fn wait_for_file(path: &Path) {
        let deadline = Instant::now() + TEST_TIMEOUT;
        while !path.is_file() {
            assert!(Instant::now() < deadline, "timed out waiting for {path:?}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn open_for_synchronization(pid: u32) -> OwnedHandle {
        // SAFETY: the requested access is synchronization-only and PID came
        // from the live test descendant.
        let handle = unsafe { OpenProcess(SYNCHRONIZE, 0, pid) };
        OwnedHandle::from_api(handle, "opening Job Object test descendant").unwrap()
    }
}
