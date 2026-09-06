#![cfg(windows)]

use std::fs;
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use vts_extract::sandbox::{self, Mechanism};

#[path = "sandbox_windows/support.rs"]
mod support;

use support::{
    child_test_command, command, dacl, protect_dacl, system_executable, wait_for_process,
    EnvironmentVariable, Junction, Workspace,
};

const DESCENDANT_START_TIMEOUT: Duration = Duration::from_secs(5);
const DESCENDANT_STOP_TIMEOUT: Duration = Duration::from_secs(1);
const MARKER_POLL_INTERVAL: Duration = Duration::from_millis(5);
const SPAWNED_OUTCOME: &str = "spawned";
const CREATION_DENIED_OUTCOME: &str = "creation-denied:5";

#[test]
fn confined_command_writes_inside_and_preserves_exit_code() {
    let workspace = Workspace::new("inside");
    let nested = workspace.0.join("existing");
    let existing = nested.join("before.txt");
    fs::create_dir(&nested).expect("create existing directory");
    fs::write(&existing, "before").expect("create existing file");
    protect_dacl(&nested);
    assert_eq!(sandbox::detect(), Some(Mechanism::WindowsAppContainer));
    let policy = sandbox::policy_text(&workspace.0).expect("render policy");
    for promise in
        "LPAC|network capabilities: none|environment: allowlist|job: kill-on-close".split('|')
    {
        assert!(policy.contains(promise), "policy omitted {promise}");
    }
    let original_dacl = dacl(&workspace.0);
    let original_nested_dacl = dacl(&nested);
    let original_file_dacl = dacl(&existing);
    let write = command(r"echo hello>existing\inside.txt");
    let code = sandbox::run(&workspace.0, &write).expect("run write command");
    assert_eq!(code, 0);
    let output = fs::read_to_string(nested.join("inside.txt")).expect("read output");
    assert_eq!(output, "hello\r\n");
    let code = sandbox::run(&workspace.0, &command("exit /b 37")).expect("run exit");
    assert_eq!(code, 37);
    assert_eq!(dacl(&workspace.0), original_dacl, "DACL was not restored");
    assert_eq!(
        dacl(&nested),
        original_nested_dacl,
        "nested directory DACL was not restored"
    );
    assert_eq!(
        dacl(&existing),
        original_file_dacl,
        "existing file DACL was not restored"
    );
}

#[test]
fn lpac_denies_explicit_reads_and_writes_outside_the_workspace() {
    let workspace = Workspace::new("boundary");
    let outside = Workspace::new("outside");
    let secret = outside.0.join("secret.txt");
    let escaped = outside.0.join("escaped.txt");
    fs::write(&secret, "credential-canary").expect("write secret canary");
    assert_eq!(fs::read_to_string(&secret).unwrap(), "credential-canary");
    fs::write(
        workspace.0.join("probe-path.txt"),
        secret.as_os_str().as_encoded_bytes(),
    )
    .expect("write read probe path");
    let read_code = sandbox::run(&workspace.0, &child_test_command("outside_read_child"))
        .expect("run read probe");
    assert!(workspace.0.join("read-probe-started.txt").is_file());
    assert_ne!(read_code, 0, "LPAC read a host canary outside the grant");

    fs::write(&escaped, "host-can-write").expect("host write precondition");
    fs::remove_file(&escaped).expect("remove host write probe");
    fs::write(
        workspace.0.join("probe-path.txt"),
        escaped.as_os_str().as_encoded_bytes(),
    )
    .expect("write write-probe path");
    let write_code = sandbox::run(&workspace.0, &child_test_command("outside_write_child"))
        .expect("run write probe");
    assert!(workspace.0.join("write-probe-started.txt").is_file());
    assert_ne!(write_code, 0, "LPAC wrote outside its workspace");
    assert!(!escaped.exists(), "outside write canary was created");
}

#[test]
#[ignore = "runs only as the child of the boundary policy test"]
fn outside_read_child() {
    fs::write("read-probe-started.txt", "started").unwrap();
    let path = fs::read_to_string("probe-path.txt").unwrap();
    fs::read(path).expect("LPAC denied the outside read as intended");
}

#[test]
#[ignore = "runs only as the child of the boundary policy test"]
fn outside_write_child() {
    fs::write("write-probe-started.txt", "started").unwrap();
    let path = fs::read_to_string("probe-path.txt").unwrap();
    fs::write(path, "escaped").expect("LPAC denied the outside write as intended");
}

#[test]
fn parent_secrets_are_not_present_in_the_child_environment() {
    let workspace = Workspace::new("environment");
    let _secret = EnvironmentVariable::set("VTS_TEST_SECRET", "must-not-cross-boundary");
    let probe = "if defined VTS_TEST_SECRET (exit /b 71) else (exit /b 0)";
    let code = sandbox::run(&workspace.0, &command(probe)).expect("run environment probe");

    assert_eq!(code, 0, "the child inherited VTS_TEST_SECRET");
}

#[test]
fn loopback_is_denied_after_a_host_connection_proves_the_server() {
    let workspace = Workspace::new("network");
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind loopback listener");
    let address = listener.local_addr().expect("listener address");
    TcpStream::connect(address).expect("host must reach loopback listener");
    listener
        .accept()
        .expect("listener must observe host connection");
    fs::write(workspace.0.join("endpoint.txt"), address.to_string()).unwrap();
    let request = child_test_command("lpac_tcp_child");
    let code = sandbox::run(&workspace.0, &request).expect("run confined TCP probe");
    assert!(workspace.0.join("tcp-probe-started.txt").is_file());
    assert_ne!(code, 0, "LPAC received a network capability");
    listener.set_nonblocking(true).unwrap();
    let error = listener
        .accept()
        .expect_err("LPAC reached loopback listener");
    assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock);
}

#[test]
#[ignore = "runs only as the child of the loopback policy test"]
fn lpac_tcp_child() {
    fs::write("tcp-probe-started.txt", "started").expect("mark TCP child start");
    let endpoint = fs::read_to_string("endpoint.txt").expect("read endpoint");
    TcpStream::connect(endpoint.trim()).expect("LPAC denied loopback as intended");
}

#[test]
fn a_hard_link_inside_the_workspace_is_refused() {
    let workspace = Workspace::new("hard-link");
    let outside = Workspace::new("hard-link-target");
    let target = outside.0.join("target.txt");
    fs::write(&target, "host data").expect("write hard-link target");
    fs::hard_link(&target, workspace.0.join("alias.txt")).expect("create hard link");

    let error =
        sandbox::run(&workspace.0, &command("exit /b 0")).expect_err("hard link must fail closed");
    assert!(error.to_string().contains("hard link"));
}

#[test]
fn a_reparse_point_inside_the_workspace_is_refused() {
    let workspace = Workspace::new("reparse");
    let outside = Workspace::new("reparse-target");
    let junction = workspace.0.join("escape");
    let output = Command::new(system_executable("cmd.exe"))
        .args(["/d", "/c", "mklink", "/J"])
        .arg(&junction)
        .arg(&outside.0)
        .output()
        .expect("create junction");
    assert!(output.status.success(), "mklink failed: {output:?}");
    let _junction = Junction(junction);

    let error = sandbox::run(&workspace.0, &command("exit /b 0"))
        .expect_err("reparse point must fail closed");
    assert!(error.to_string().contains("reparse point"));
}

#[test]
fn descendants_are_killed_or_the_lpac_host_denies_their_creation() {
    let workspace = Workspace::new("tree");
    let mut request = child_test_command("job_parent_child");
    let executable = workspace.0.join("sandbox-test.exe");
    let executable_bytes = fs::read(&request[0]).expect("read descendant probe executable");
    fs::write(&executable, executable_bytes).expect("create descendant probe inside workspace");
    request[0] = executable.display().to_string();

    let workspace_path = workspace.0.clone();
    let runner = thread::spawn(move || sandbox::run(&workspace_path, &request));
    let outcome_path = workspace.0.join("spawn-outcome.txt");
    let outcome = wait_for_spawn_outcome(&outcome_path);
    let descendant = if outcome == SPAWNED_OUTCOME {
        let process = wait_for_process(&workspace.0.join("started.txt"), DESCENDANT_START_TIMEOUT)
            .expect("open the live descendant process");
        fs::write(workspace.0.join("parent-release.txt"), "release")
            .expect("release the root process");
        Some(process)
    } else {
        assert_eq!(outcome, CREATION_DENIED_OUTCOME);
        None
    };

    let code = runner
        .join()
        .expect("sandbox runner thread panicked")
        .expect("run process-tree probe");
    assert_eq!(code, 0, "the LPAC parent probe failed");
    if let Some(descendant) = descendant {
        assert!(
            descendant
                .terminated_within(DESCENDANT_STOP_TIMEOUT)
                .expect("wait for descendant termination"),
            "descendant survived sandbox::run"
        );
    }
}

#[test]
#[ignore = "runs only as the root process of the Job Object policy test"]
#[allow(clippy::zombie_processes)] // The outer backend must reap this Job Object descendant.
fn job_parent_child() {
    let executable = std::env::current_exe().expect("current test executable");
    let descendant = Command::new(executable)
        .args(["--ignored", "--exact", "job_grandchild", "--test-threads=1"])
        .spawn();
    match descendant {
        Ok(_descendant) => {
            fs::write("spawn-outcome.txt", SPAWNED_OUTCOME).expect("publish successful spawn");
            wait_for_marker(Path::new("started.txt"), "Job Object descendant start");
            wait_for_marker(Path::new("parent-release.txt"), "parent release");
        }
        Err(error) if error.raw_os_error() == Some(5) => {
            fs::write("spawn-outcome.txt", CREATION_DENIED_OUTCOME)
                .expect("publish LPAC process-creation denial");
        }
        Err(error) => panic!("unexpected descendant spawn failure: {error}"),
    }
}

#[test]
#[ignore = "runs only as the descendant of the Job Object policy test"]
fn job_grandchild() {
    fs::write("started.txt", std::process::id().to_string()).expect("publish descendant PID");
    loop {
        thread::sleep(Duration::from_secs(1));
    }
}

fn wait_for_marker(path: &Path, operation: &str) {
    let deadline = Instant::now() + DESCENDANT_START_TIMEOUT;
    while !path.is_file() {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {operation} marker {}",
            path.display()
        );
        thread::sleep(MARKER_POLL_INTERVAL);
    }
}

fn wait_for_spawn_outcome(path: &Path) -> String {
    let deadline = Instant::now() + DESCENDANT_START_TIMEOUT;
    loop {
        match fs::read_to_string(path) {
            Ok(value) if value == SPAWNED_OUTCOME || value == CREATION_DENIED_OUTCOME => {
                return value;
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => panic!("read descendant spawn outcome: {error}"),
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for descendant spawn outcome"
        );
        thread::sleep(MARKER_POLL_INTERVAL);
    }
}
