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

/// [`HttpFetcher`] with a caller-chosen ceiling, for the opt-in model
/// shards (3.9 GB each). The checksum still decides what is kept; the
/// ceiling only stops a runaway stream early.
pub struct LargeFileFetcher {
    pub max_bytes: u64,
    pub timeout: Duration,
}

impl Fetcher for LargeFileFetcher {
    fn fetch(&self, url: &str, dest: &Path) -> Result<()> {
        download(url, dest, self.max_bytes, self.timeout)
    }
}

fn download(url: &str, dest: &Path, limit: u64, timeout: Duration) -> Result<()> {
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
    if total.is_some_and(|bytes| bytes > limit) {
        bail!("download from {url} exceeds the {limit}-byte safety limit");
    }

    let mut reader = response.into_body().into_reader();
    let mut file = std::fs::File::create(dest)?;
    copy_bounded(&mut reader, &mut file, total, limit)?;
    file.flush()?;
    file.sync_all()?;
    Ok(())
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

    use super::copy_bounded;

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
