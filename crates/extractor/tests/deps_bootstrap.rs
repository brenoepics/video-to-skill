//! Bootstrap: download → checksum-verify → atomic install → marker.

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use vts_extract::deps::bootstrap::{install, Archive, Fetcher, ToolSpec};
use vts_extract::deps::{probe, verification_marker, Env, Presence, Tool};

/// Serves canned bytes, or fails N times before succeeding.
struct FakeFetcher {
    payload: Vec<u8>,
    failures_left: AtomicUsize,
}

impl FakeFetcher {
    fn serving(payload: &[u8]) -> FakeFetcher {
        FakeFetcher {
            payload: payload.to_vec(),
            failures_left: AtomicUsize::new(0),
        }
    }

    fn failing_once_then(payload: &[u8]) -> FakeFetcher {
        FakeFetcher {
            payload: payload.to_vec(),
            failures_left: AtomicUsize::new(1),
        }
    }
}

impl Fetcher for FakeFetcher {
    fn fetch(&self, _url: &str, dest: &Path) -> anyhow::Result<()> {
        if self.failures_left.load(Ordering::SeqCst) > 0 {
            self.failures_left.fetch_sub(1, Ordering::SeqCst);
            // Simulate a mid-download interruption: partial bytes then error.
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
// Independently computed: echo -n "pretend this is a static binary" | shasum -a 256
const PAYLOAD_SHA256: &str = "909f08edf03977a5c4bbcad451ccef9f52acb1ec63f5a11f8facf97c00a87e6c";

fn spec_for(tool: Tool, sha256: &str) -> ToolSpec {
    ToolSpec {
        tool,
        url: "https://example.invalid/tool".into(),
        sha256: sha256.into(),
        archive: Archive::None,
    }
}

#[test]
fn verified_install_lands_tool_marker_and_probe_agreement() {
    let data = scratch("ok");
    let spec = spec_for(Tool::Ytdlp, PAYLOAD_SHA256);

    let installed = install(&spec, &FakeFetcher::serving(PAYLOAD), &data).unwrap();

    assert_eq!(installed, Tool::Ytdlp.install_path(&data));
    assert_eq!(fs::read(&installed).unwrap(), PAYLOAD);
    assert!(verification_marker(&installed).is_file());
    let env = Env {
        path_dirs: vec![],
        data_dir: data,
    };
    let status = probe(&env);
    let ytdlp = status.iter().find(|s| s.tool == Tool::Ytdlp).unwrap();
    assert_eq!(ytdlp.presence, Presence::Installed);
}

#[cfg(unix)]
#[test]
fn installed_executables_are_marked_executable() {
    use std::os::unix::fs::PermissionsExt;
    let data = scratch("exec");
    let spec = spec_for(Tool::Ytdlp, PAYLOAD_SHA256);

    let installed = install(&spec, &FakeFetcher::serving(PAYLOAD), &data).unwrap();

    let mode = fs::metadata(&installed).unwrap().permissions().mode();
    assert_ne!(mode & 0o111, 0, "binary should be executable");
}

#[test]
fn checksum_mismatch_installs_nothing() {
    let data = scratch("badsum");
    let spec = spec_for(Tool::Ytdlp, &"0".repeat(64));

    let err = install(&spec, &FakeFetcher::serving(PAYLOAD), &data).unwrap_err();

    assert!(err.to_string().contains("checksum"), "got: {err}");
    let final_path = Tool::Ytdlp.install_path(&data);
    assert!(!final_path.exists(), "corrupt file must not be installed");
    assert!(!verification_marker(&final_path).exists());
}

#[test]
fn zipped_artifact_is_extracted_to_the_bare_tool() {
    let data = scratch("zip");
    // Build a real zip fixture with the system zip (same binary macOS ships).
    let staging = scratch("zip-src");
    fs::write(staging.join("ffmpeg"), PAYLOAD).unwrap();
    let out = std::process::Command::new("zip")
        .arg("-j")
        .arg(staging.join("bundle.zip"))
        .arg(staging.join("ffmpeg"))
        .output()
        .unwrap();
    assert!(out.status.success());
    let zip_bytes = fs::read(staging.join("bundle.zip")).unwrap();
    // Zip bytes vary per run (timestamps), so hash the fixture itself; the
    // checksum mechanism is proven independently by the mismatch test above.
    let zip_sha = vts_extract::hashing::sha256_hex(&zip_bytes);
    let spec = ToolSpec {
        tool: Tool::Ffmpeg,
        url: "https://example.invalid/ffmpeg.zip".into(),
        sha256: zip_sha,
        archive: Archive::Zip {
            member: "ffmpeg".into(),
        },
    };

    let installed = install(&spec, &FakeFetcher::serving(&zip_bytes), &data).unwrap();

    assert_eq!(
        fs::read(&installed).unwrap(),
        PAYLOAD,
        "installed tool must be the extracted member, not the zip"
    );
    assert!(verification_marker(&installed).is_file());
}

#[test]
fn interrupted_download_is_restartable_and_leaves_no_trace_on_failure() {
    let data = scratch("retry");
    let spec = spec_for(Tool::Ytdlp, PAYLOAD_SHA256);
    let fetcher = FakeFetcher::failing_once_then(PAYLOAD);

    let first = install(&spec, &fetcher, &data);
    assert!(first.is_err());
    let final_path = Tool::Ytdlp.install_path(&data);
    assert!(!final_path.exists());
    assert!(!verification_marker(&final_path).exists());

    // Same spec, same data dir: the retry must succeed cleanly.
    let second = install(&spec, &fetcher, &data).unwrap();
    assert_eq!(fs::read(second).unwrap(), PAYLOAD);
}
