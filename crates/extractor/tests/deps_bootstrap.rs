//! Bootstrap: download -> checksum -> transactional install -> marker.

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use vts_extract::deps::bootstrap::{install, Archive, Fetcher, ToolSpec};
use vts_extract::deps::{verification_marker, Tool};

#[path = "deps_bootstrap/deps_zip.rs"]
mod deps_zip;

/// Serves canned bytes, or fails N times before succeeding.
struct FakeFetcher {
    payload: Vec<u8>,
    failures_left: AtomicUsize,
    calls: AtomicUsize,
}

impl FakeFetcher {
    fn serving(payload: &[u8]) -> FakeFetcher {
        FakeFetcher {
            payload: payload.to_vec(),
            failures_left: AtomicUsize::new(0),
            calls: AtomicUsize::new(0),
        }
    }

    fn failing_once_then(payload: &[u8]) -> FakeFetcher {
        FakeFetcher {
            payload: payload.to_vec(),
            failures_left: AtomicUsize::new(1),
            calls: AtomicUsize::new(0),
        }
    }
}

impl Fetcher for FakeFetcher {
    fn fetch(&self, _url: &str, dest: &Path) -> anyhow::Result<()> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.failures_left.load(Ordering::SeqCst) > 0 {
            self.failures_left.fetch_sub(1, Ordering::SeqCst);
            fs::write(dest, &self.payload[..1])?;
            anyhow::bail!("simulated network drop");
        }
        fs::write(dest, &self.payload)?;
        Ok(())
    }
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("vts-bootstrap-tests")
        .join(format!("{name}-{}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

const PAYLOAD: &[u8] = b"pretend this is a static binary";
const PAYLOAD_SHA256: &str = "909f08edf03977a5c4bbcad451ccef9f52acb1ec63f5a11f8facf97c00a87e6c";
const REPLACEMENT: &[u8] = b"replacement static binary";
const REPLACEMENT_SHA256: &str = "dd07d7f876edbe146f17bc84cf1e7d26c810f08927b542c291402f5a013d2416";

fn spec_for(tool: Tool, sha256: &str) -> ToolSpec {
    ToolSpec {
        tool,
        url: "https://example.invalid/tool".into(),
        sha256: sha256.into(),
        archive: Archive::None,
    }
}

#[test]
fn verified_install_lands_tool_and_matching_marker() {
    let data = scratch("ok");
    let spec = spec_for(Tool::Ytdlp, PAYLOAD_SHA256);

    let installed = install(&spec, &FakeFetcher::serving(PAYLOAD), &data).unwrap();

    assert_eq!(installed, Tool::Ytdlp.install_path(&data));
    assert_eq!(fs::read(&installed).unwrap(), PAYLOAD);
    assert_eq!(
        fs::read_to_string(verification_marker(&installed)).unwrap(),
        format!("sha256:{PAYLOAD_SHA256}\n")
    );
}

#[test]
fn uppercase_checksum_pin_remains_idempotent() {
    let data = scratch("uppercase-pin");
    let spec = spec_for(Tool::Ytdlp, &PAYLOAD_SHA256.to_ascii_uppercase());
    let fetcher = FakeFetcher::serving(PAYLOAD);

    let first = install(&spec, &fetcher, &data).unwrap();
    let second = install(&spec, &fetcher, &data).unwrap();

    assert_eq!(first, second);
    assert_eq!(fetcher.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        fs::read_to_string(verification_marker(&first)).unwrap(),
        format!("sha256:{PAYLOAD_SHA256}\n")
    );
}

#[cfg(unix)]
#[test]
fn installed_executables_are_marked_executable() {
    use std::os::unix::fs::PermissionsExt;
    let data = scratch("exec");

    let installed = install(
        &spec_for(Tool::Ytdlp, PAYLOAD_SHA256),
        &FakeFetcher::serving(PAYLOAD),
        &data,
    )
    .unwrap();

    assert_ne!(
        fs::metadata(installed).unwrap().permissions().mode() & 0o111,
        0
    );
}

#[test]
fn checksum_mismatch_installs_nothing() {
    let data = scratch("badsum");
    let spec = spec_for(Tool::Ytdlp, &"0".repeat(64));

    let error = install(&spec, &FakeFetcher::serving(PAYLOAD), &data).unwrap_err();

    assert!(error.to_string().contains("checksum"), "got: {error}");
    let final_path = Tool::Ytdlp.install_path(&data);
    assert!(!final_path.exists());
    assert!(!verification_marker(&final_path).exists());
}

#[test]
fn interrupted_download_is_restartable_and_leaves_no_trace_on_failure() {
    let data = scratch("retry");
    let spec = spec_for(Tool::Ytdlp, PAYLOAD_SHA256);
    let fetcher = FakeFetcher::failing_once_then(PAYLOAD);

    assert!(install(&spec, &fetcher, &data).is_err());
    let final_path = Tool::Ytdlp.install_path(&data);
    assert!(!final_path.exists());
    assert!(!verification_marker(&final_path).exists());

    let installed = install(&spec, &fetcher, &data).unwrap();
    assert_eq!(fs::read(installed).unwrap(), PAYLOAD);
}

#[test]
fn failed_reinstall_preserves_the_previous_verified_tool() {
    let data = scratch("reinstall-failure");
    let spec = spec_for(Tool::Ytdlp, PAYLOAD_SHA256);
    install(&spec, &FakeFetcher::serving(PAYLOAD), &data).unwrap();

    let replacement = spec_for(Tool::Ytdlp, &"0".repeat(64));
    let error = install(
        &replacement,
        &FakeFetcher::serving(b"corrupt replacement"),
        &data,
    )
    .unwrap_err();

    assert!(error.to_string().contains("checksum"));
    let installed = Tool::Ytdlp.install_path(&data);
    assert_eq!(fs::read(&installed).unwrap(), PAYLOAD);
    assert!(verification_marker(&installed).is_file());
}

#[test]
fn verified_reinstall_atomically_replaces_an_existing_tool() {
    let data = scratch("reinstall-success");
    install(
        &spec_for(Tool::Ytdlp, PAYLOAD_SHA256),
        &FakeFetcher::serving(PAYLOAD),
        &data,
    )
    .unwrap();

    let installed = install(
        &spec_for(Tool::Ytdlp, REPLACEMENT_SHA256),
        &FakeFetcher::serving(REPLACEMENT),
        &data,
    )
    .unwrap();

    assert_eq!(fs::read(&installed).unwrap(), REPLACEMENT);
    assert_eq!(
        fs::read_to_string(verification_marker(&installed)).unwrap(),
        format!("sha256:{REPLACEMENT_SHA256}\n")
    );
}
