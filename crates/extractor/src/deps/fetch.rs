//! The real (network) fetcher. Everything else in `deps` is offline.

use std::io::{Read, Write};
use std::path::Path;
use std::time::Duration;

use anyhow::{bail, Context, Result};

use super::bootstrap::Fetcher;

const MAX_DOWNLOAD_BYTES: u64 = 1 << 30;
const COPY_BUFFER_BYTES: usize = 1 << 16;
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// Downloads over HTTPS with a coarse progress line per ~10%, refusing
/// anything over 1 GiB — every default dependency is far smaller.
pub struct HttpFetcher;

impl Fetcher for HttpFetcher {
    fn fetch(&self, url: &str, dest: &Path) -> Result<()> {
        download(url, dest, MAX_DOWNLOAD_BYTES, DOWNLOAD_TIMEOUT)
    }
}

/// [`HttpFetcher`] for the opt-in model shards (3.9 GB each), with a
/// longer timeout: a sized fetch is capped at the file's pinned size and
/// refused up front when the server announces any other length. The
/// checksum still decides what is kept.
pub struct LargeFileFetcher {
    pub timeout: Duration,
}

impl Fetcher for LargeFileFetcher {
    fn fetch(&self, url: &str, dest: &Path) -> Result<()> {
        download(url, dest, MAX_DOWNLOAD_BYTES, self.timeout)
    }

    fn fetch_sized(&self, url: &str, dest: &Path, size: u64) -> Result<()> {
        download_exact(url, dest, Some(size), size, self.timeout)
    }
}

fn download(url: &str, dest: &Path, limit: u64, timeout: Duration) -> Result<()> {
    download_exact(url, dest, None, limit, timeout)
}

fn download_exact(
    url: &str,
    dest: &Path,
    exact: Option<u64>,
    limit: u64,
    timeout: Duration,
) -> Result<()> {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .build();
    let agent: ureq::Agent = config.into();
    let response = agent
        .get(url)
        .call()
        .with_context(|| format!("requesting {url}"))?;
    let total: Option<u64> = response
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok());
    check_announced_length(url, total, exact, limit)?;

    let mut reader = response.into_body().into_reader();
    let mut file = std::fs::File::create(dest)?;
    copy_bounded(&mut reader, &mut file, total, limit)?;
    file.flush()?;
    file.sync_all()?;
    Ok(())
}

/// Refuse a download before reading its body when the announced length
/// is over the limit, or differs from the pinned size.
fn check_announced_length(
    url: &str,
    total: Option<u64>,
    exact: Option<u64>,
    limit: u64,
) -> Result<()> {
    match (total, exact) {
        (Some(bytes), Some(size)) if bytes != size => {
            bail!("download from {url} announces {bytes} bytes; {size} are pinned")
        }
        (Some(bytes), _) if bytes > limit => {
            bail!("download from {url} exceeds the {limit}-byte safety limit")
        }
        _ => Ok(()),
    }
}

fn copy_bounded(
    reader: &mut impl Read,
    writer: &mut impl Write,
    total: Option<u64>,
    limit: u64,
) -> Result<()> {
    let mut buf = vec![0u8; COPY_BUFFER_BYTES];
    let mut done: u64 = 0;
    let mut next_report: u64 = 0;
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        done = done
            .checked_add(u64::try_from(n).context("download byte count overflow")?)
            .context("download byte count overflow")?;
        if done > limit {
            bail!("download exceeded the {limit}-byte safety limit");
        }
        writer.write_all(&buf[..n])?;
        if let Some(total) = total {
            if done >= next_report {
                let pct = done * 100 / total.max(1);
                println!("  … {pct}% ({} / {} MB)", done >> 20, total >> 20);
                next_report = done + total / 10;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::{check_announced_length, copy_bounded};

    #[test]
    fn a_pinned_size_refuses_any_other_announced_length_up_front() {
        let url = "https://huggingface.co/x/resolve/0/model.safetensors";

        let error = check_announced_length(url, Some(10), Some(7), 100).unwrap_err();
        assert!(error.to_string().contains("7 are pinned"), "{error}");
        assert!(check_announced_length(url, Some(7), Some(7), 7).is_ok());
        assert!(check_announced_length(url, None, Some(7), 7).is_ok());
        let error = check_announced_length(url, Some(200), None, 100).unwrap_err();
        assert!(error.to_string().contains("safety limit"), "{error}");
    }

    #[test]
    fn a_stream_over_the_limit_is_rejected_before_being_written() {
        let mut source = Cursor::new(b"oversized".to_vec());
        let mut destination = Vec::new();

        let error = copy_bounded(&mut source, &mut destination, None, 4).unwrap_err();

        assert!(error.to_string().contains("safety limit"));
        assert!(
            destination.is_empty(),
            "the limit is enforced before anything is written"
        );
    }
}
