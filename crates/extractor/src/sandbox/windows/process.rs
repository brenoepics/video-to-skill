//! Suspended LPAC process creation and complete Job Object lifetime control.

use std::ffi::c_void;
use std::mem::{size_of, size_of_val};
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use anyhow::{bail, Context, Result};
use windows_sys::Win32::Foundation::{TRUE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows_sys::Win32::Security::{PSID, SECURITY_CAPABILITIES};
use windows_sys::Win32::System::Threading::{
    CreateProcessW, DeleteProcThreadAttributeList, GetExitCodeProcess,
    InitializeProcThreadAttributeList, ResumeThread, TerminateProcess, UpdateProcThreadAttribute,
    WaitForSingleObject, CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT,
    EXTENDED_STARTUPINFO_PRESENT, LPPROC_THREAD_ATTRIBUTE_LIST, PROCESS_INFORMATION,
    PROC_THREAD_ATTRIBUTE_ALL_APPLICATION_PACKAGES_POLICY, PROC_THREAD_ATTRIBUTE_HANDLE_LIST,
    PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES, STARTF_USESTDHANDLES, STARTUPINFOEXW,
};
use windows_sys::Win32::System::WindowsProgramming::PROCESS_CREATION_ALL_APPLICATION_PACKAGES_OPT_OUT;

use super::command::ResolvedCommand;
use super::environment::EnvironmentBlock;
use super::handle::OwnedHandle;
use super::job::Job;
use super::stdio::StandardHandles;

const JOB_TERMINATION_EXIT_CODE: u32 = 1;
const COMMAND_TIMEOUT_MS: u32 = 5 * 60 * 1_000;
const PROCESS_TERMINATION_TIMEOUT_MS: u32 = 10_000;

pub(super) fn run(workspace: &Path, sid: PSID, mut command: ResolvedCommand) -> Result<i32> {
    let environment = EnvironmentBlock::build(workspace, command.executable())?;
    let current_directory = wide_path(workspace)?;
    let job = Job::create()?;
    let mut process =
        create_suspended(sid, &mut command, &environment, current_directory.as_ptr())?;
    job.assign(process.process.raw())?;

    let execution = process.resume_and_wait();
    let tree_cleanup = job.terminate_remaining();
    match (execution, tree_cleanup) {
        (Ok(code), Ok(())) => Ok(code),
        (Err(error), Ok(())) | (Ok(_), Err(error)) => Err(error),
        (Err(error), Err(cleanup)) => Err(error.context(format!(
            "additionally failed to terminate the sandbox process tree: {cleanup:#}"
        ))),
    }
}

fn create_suspended(
    sid: PSID,
    command: &mut ResolvedCommand,
    environment: &EnvironmentBlock,
    current_directory: *const u16,
) -> Result<SuspendedProcess> {
    let standard = StandardHandles::inherit()?;
    let inherited = standard.as_array();
    let capabilities = SECURITY_CAPABILITIES {
        AppContainerSid: sid,
        Capabilities: std::ptr::null_mut(),
        CapabilityCount: 0,
        Reserved: 0,
    };
    let lpac_policy = PROCESS_CREATION_ALL_APPLICATION_PACKAGES_OPT_OUT;
    let mut attributes = AttributeList::create(3)?;
    attributes.update(
        PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES,
        (&raw const capabilities).cast(),
        size_of::<SECURITY_CAPABILITIES>(),
    )?;
    attributes.update(
        PROC_THREAD_ATTRIBUTE_ALL_APPLICATION_PACKAGES_POLICY,
        (&raw const lpac_policy).cast(),
        size_of::<u32>(),
    )?;
    attributes.update(
        PROC_THREAD_ATTRIBUTE_HANDLE_LIST,
        inherited.as_ptr().cast(),
        size_of_val(&inherited),
    )?;

    let mut startup = STARTUPINFOEXW::default();
    startup.StartupInfo.cb = u32::try_from(size_of::<STARTUPINFOEXW>())
        .context("STARTUPINFOEXW size does not fit in u32")?;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = inherited[0];
    startup.StartupInfo.hStdOutput = inherited[1];
    startup.StartupInfo.hStdError = inherited[2];
    startup.lpAttributeList = attributes.raw();

    let mut information = PROCESS_INFORMATION::default();
    let flags = CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT | EXTENDED_STARTUPINFO_PRESENT;
    // SAFETY: all pointers reference initialized storage that outlives the
    // call. Only the three handles in the attribute list are inheritable.
    let succeeded = unsafe {
        CreateProcessW(
            command.application_name(),
            command.command_line_mut(),
            std::ptr::null(),
            std::ptr::null(),
            TRUE,
            flags,
            environment.as_ptr().cast(),
            current_directory,
            &raw const startup.StartupInfo,
            &raw mut information,
        )
    };
    if succeeded == 0 {
        return Err(std::io::Error::last_os_error()).context("creating the suspended LPAC process");
    }
    SuspendedProcess::from_information(information)
}

struct AttributeList {
    _storage: Vec<usize>,
    pointer: LPPROC_THREAD_ATTRIBUTE_LIST,
}

impl AttributeList {
    fn create(count: u32) -> Result<Self> {
        let mut bytes = 0;
        // SAFETY: a null first call is the documented size query.
        unsafe {
            InitializeProcThreadAttributeList(std::ptr::null_mut(), count, 0, &raw mut bytes);
        }
        if bytes == 0 {
            return Err(std::io::Error::last_os_error())
                .context("sizing the process attribute list");
        }
        let words = bytes.div_ceil(size_of::<usize>());
        let mut storage = vec![0_usize; words];
        let pointer = storage.as_mut_ptr().cast();
        // SAFETY: `storage` has the exact requested byte capacity and alignment.
        let succeeded =
            unsafe { InitializeProcThreadAttributeList(pointer, count, 0, &raw mut bytes) };
        if succeeded == 0 {
            return Err(std::io::Error::last_os_error())
                .context("initializing the process attribute list");
        }
        Ok(Self {
            _storage: storage,
            pointer,
        })
    }

    fn update(&mut self, attribute: u32, value: *const c_void, bytes: usize) -> Result<()> {
        let attribute = usize::try_from(attribute).context("attribute id does not fit usize")?;
        // SAFETY: the list is initialized and each supplied value remains live
        // until after CreateProcessW returns and this list is deleted.
        let succeeded = unsafe {
            UpdateProcThreadAttribute(
                self.pointer,
                0,
                attribute,
                value,
                bytes,
                std::ptr::null_mut(),
                std::ptr::null(),
            )
        };
        if succeeded == 0 {
            return Err(std::io::Error::last_os_error())
                .context("setting a process security attribute");
        }
        Ok(())
    }

    fn raw(&self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        self.pointer
    }
}

impl Drop for AttributeList {
    fn drop(&mut self) {
        // SAFETY: the second initialization call succeeded for this pointer.
        unsafe {
            DeleteProcThreadAttributeList(self.pointer);
        }
    }
}

struct SuspendedProcess {
    process: OwnedHandle,
    thread: OwnedHandle,
    completed: bool,
}

impl SuspendedProcess {
    fn from_information(information: PROCESS_INFORMATION) -> Result<Self> {
        let process = OwnedHandle::from_api(
            information.hProcess,
            "receiving the suspended process handle",
        )?;
        let thread = match OwnedHandle::from_api(
            information.hThread,
            "receiving the suspended thread handle",
        ) {
            Ok(thread) => thread,
            Err(error) => {
                let cleanup = terminate_and_wait(process.raw());
                return match cleanup {
                    Ok(()) => Err(error),
                    Err(cleanup) => Err(error.context(format!(
                        "also failed to stop the suspended process: {cleanup:#}"
                    ))),
                };
            }
        };
        Ok(Self {
            process,
            thread,
            completed: false,
        })
    }

    fn resume_and_wait(&mut self) -> Result<i32> {
        // SAFETY: this is the primary thread returned suspended by CreateProcessW.
        let previous = unsafe { ResumeThread(self.thread.raw()) };
        if previous == u32::MAX {
            return Err(std::io::Error::last_os_error()).context("resuming the sandbox process");
        }
        // SAFETY: the process handle has synchronization access.
        let wait = unsafe { WaitForSingleObject(self.process.raw(), COMMAND_TIMEOUT_MS) };
        if wait == WAIT_TIMEOUT {
            bail!("sandbox command timed out after {COMMAND_TIMEOUT_MS} ms");
        }
        if wait != WAIT_OBJECT_0 {
            return Err(std::io::Error::last_os_error()).context("waiting for the sandbox process");
        }
        self.completed = true;
        let mut exit_code = 0;
        // SAFETY: the process is signaled and the output pointer is writable.
        let succeeded = unsafe { GetExitCodeProcess(self.process.raw(), &raw mut exit_code) };
        if succeeded == 0 {
            return Err(std::io::Error::last_os_error())
                .context("reading the sandbox process exit code");
        }
        Ok(i32::from_ne_bytes(exit_code.to_ne_bytes()))
    }
}

impl Drop for SuspendedProcess {
    fn drop(&mut self) {
        if self.completed {
            return;
        }
        // The explicit execution path reports cleanup errors. Drop remains a
        // bounded backstop, followed by the Job's kill-on-close guarantee.
        let _ = terminate_and_wait(self.process.raw());
    }
}

fn terminate_and_wait(process: windows_sys::Win32::Foundation::HANDLE) -> Result<()> {
    // SAFETY: the caller keeps the process handle open through this function.
    let terminated = unsafe { TerminateProcess(process, JOB_TERMINATION_EXIT_CODE) };
    if terminated == 0 {
        // A process that exited between checks already satisfies cleanup.
        // SAFETY: the handle has synchronization access.
        if unsafe { WaitForSingleObject(process, 0) } == WAIT_OBJECT_0 {
            return Ok(());
        }
        return Err(std::io::Error::last_os_error()).context("terminating the sandbox process");
    }
    // SAFETY: the process handle has synchronization access.
    match unsafe { WaitForSingleObject(process, PROCESS_TERMINATION_TIMEOUT_MS) } {
        WAIT_OBJECT_0 => Ok(()),
        WAIT_TIMEOUT => {
            bail!("sandbox process did not terminate within {PROCESS_TERMINATION_TIMEOUT_MS} ms")
        }
        _ => Err(std::io::Error::last_os_error()).context("waiting for process termination"),
    }
}

fn wide_path(path: &Path) -> Result<Vec<u16>> {
    let mut encoded: Vec<_> = path.as_os_str().encode_wide().collect();
    if encoded.contains(&0) {
        bail!(
            "workspace path contains a nul character: {}",
            path.display()
        );
    }
    encoded.push(0);
    Ok(encoded)
}
