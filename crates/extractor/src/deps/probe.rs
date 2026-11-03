//! The PATH compatibility probe: a found executable only counts when it
//! accepts the options the pipeline relies on, and spawning it is the only
//! way to know.

use std::io;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const EXECUTABLE_PROBE_TIMEOUT: Duration = Duration::from_secs(5);
const EXECUTABLE_PROBE_POLL_INTERVAL: Duration = Duration::from_millis(10);
/// On Linux, exec of a file that any process still holds open for writing
/// fails with `ETXTBSY`, and a child forked by another thread keeps every
/// open descriptor until its own exec. That window is microseconds long
/// and closes by itself, so a busy executable is retried briefly instead
/// of being reported unusable.
const EXECUTABLE_BUSY_RETRIES: u32 = 8;
const EXECUTABLE_BUSY_BACKOFF: Duration = Duration::from_millis(25);

/// True when `path args` runs and exits successfully within the probe
/// timeout. Any spawn failure other than a transient "text file busy"
/// means the executable is unusable.
pub(super) fn command_succeeds_within(path: &Path, args: &[&str]) -> bool {
    let Ok(mut child) = retry_while_busy(|| spawn_quiet(path, args)) else {
        return false;
    };
    let deadline = Instant::now() + EXECUTABLE_PROBE_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(EXECUTABLE_PROBE_POLL_INTERVAL);
            }
            Ok(None) | Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
    }
}

fn spawn_quiet(path: &Path, args: &[&str]) -> io::Result<Child> {
    Command::new(path)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
}

/// Run `attempt`, retrying a bounded number of times while it fails with
/// "text file busy"; every other error is returned at once.
fn retry_while_busy<T>(mut attempt: impl FnMut() -> io::Result<T>) -> io::Result<T> {
    let mut retries_left = EXECUTABLE_BUSY_RETRIES;
    loop {
        match attempt() {
            Err(error) if is_transient(&error) && retries_left > 0 => {
                retries_left -= 1;
                std::thread::sleep(EXECUTABLE_BUSY_BACKOFF);
            }
            outcome => return outcome,
        }
    }
}

/// Only "text file busy" is worth waiting out: not found, permission
/// denied and a bad executable image are all final.
fn is_transient(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::ExecutableFileBusy
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_busy_executable_is_retried_until_it_frees_up() {
        let mut attempts = 0;

        let outcome = retry_while_busy(|| {
            attempts += 1;
            if attempts < 3 {
                Err(io::Error::from(io::ErrorKind::ExecutableFileBusy))
            } else {
                Ok(attempts)
            }
        });

        assert_eq!(outcome.ok(), Some(3));
    }

    #[test]
    fn other_spawn_errors_are_final() {
        let mut attempts = 0;

        let outcome = retry_while_busy(|| {
            attempts += 1;
            Err::<(), _>(io::Error::from(io::ErrorKind::NotFound))
        });

        assert_eq!(outcome.unwrap_err().kind(), io::ErrorKind::NotFound);
        assert_eq!(attempts, 1, "a missing executable is not retried");
    }

    #[test]
    fn an_executable_busy_for_too_long_is_given_up_on() {
        let mut attempts = 0;

        let outcome = retry_while_busy(|| {
            attempts += 1;
            Err::<(), _>(io::Error::from(io::ErrorKind::ExecutableFileBusy))
        });

        assert_eq!(
            outcome.unwrap_err().kind(),
            io::ErrorKind::ExecutableFileBusy
        );
        assert_eq!(attempts, EXECUTABLE_BUSY_RETRIES + 1);
    }

    #[cfg(unix)]
    #[test]
    fn text_file_busy_is_the_only_transient_spawn_error() {
        // ETXTBSY is 26 on Linux and macOS alike.
        assert!(is_transient(&io::Error::from_raw_os_error(26)));
        // ENOENT and EACCES: the tool is genuinely unusable.
        assert!(!is_transient(&io::Error::from_raw_os_error(2)));
        assert!(!is_transient(&io::Error::from_raw_os_error(13)));
    }
}
