//! The real (network) fetcher. Everything else in `deps` is offline.

use std::io::{Read, Write};
use std::path::Path;

use anyhow::{Context, Result};

use super::bootstrap::Fetcher;

/// Downloads over HTTPS with a coarse progress line per ~10%.
pub struct HttpFetcher;

impl Fetcher for HttpFetcher {
    fn fetch(&self, url: &str, dest: &Path) -> Result<()> {
        let response = ureq::get(url)
            .call()
            .with_context(|| format!("requesting {url}"))?;
        let total: Option<u64> = response
            .headers()
            .get("content-length")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok());

        let mut reader = response.into_body().into_reader();
        let mut file = std::fs::File::create(dest)?;
        let mut buf = vec![0u8; 1 << 16];
        let mut done: u64 = 0;
        let mut next_report: u64 = 0;
        loop {
            let n = reader.read(&mut buf)?;
            if n == 0 {
                break;
            }
            file.write_all(&buf[..n])?;
            done += n as u64;
            if let Some(total) = total {
                if done >= next_report {
                    let pct = done * 100 / total.max(1);
                    println!("  … {pct}% ({} / {} MB)", done >> 20, total >> 20);
                    next_report = done + total / 10;
                }
            }
        }
        file.flush()?;
        Ok(())
    }
}
