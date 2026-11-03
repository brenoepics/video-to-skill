//! Thumbnail sampling: one ffmpeg pass emits raw yuv420p 160x90
//! frames for the whole video — cheap enough to analyze hours of
//! footage, yet fine enough that a new terminal line or a dialog is
//! several pixels, not a fraction of one.
//!
//! The decoder skips non-reference frames (`-skip_frame noref`): nothing
//! depends on them, so output stays artifact-free, and at one sample per
//! second the at-most-few-frames shift in which picture lands on a tick
//! is invisible to state detection. It cuts the full-decode pass by
//! 1.2–1.6x on B-frame-heavy h264 and is ignored by decoders without it.
//!
//! Thumbnail-derived values (keyframe hashes, motion density, state
//! timestamps) stay identical across runs of one version, but on
//! B-frame content they can differ slightly from bundles made before
//! this flag — a different nearby picture may have landed on a tick.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};

use crate::tools::Toolchain;

/// Luma plane size of a sampled thumbnail.
pub const THUMB_W: usize = 160;
pub const THUMB_H: usize = 90;
/// Chroma planes are subsampled 2x2 (yuv420p).
pub const CHROMA_W: usize = THUMB_W / 2;
pub const CHROMA_H: usize = THUMB_H / 2;
const LUMA_LEN: usize = THUMB_W * THUMB_H;
const CHROMA_LEN: usize = CHROMA_W * CHROMA_H;
/// Bytes per sampled frame: Y, then U, then V.
pub const FRAME_LEN: usize = LUMA_LEN + 2 * CHROMA_LEN;

pub struct SampledFrame {
    pub timestamp_secs: f64,
    planes: Vec<u8>,
}

impl SampledFrame {
    /// The 160x90 luma plane, row-major.
    #[must_use]
    pub fn luma(&self) -> &[u8] {
        &self.planes[..LUMA_LEN]
    }

    /// The two 80x45 chroma planes (U, V), row-major.
    #[must_use]
    pub fn chroma(&self) -> (&[u8], &[u8]) {
        let (u, v) = self.planes[LUMA_LEN..].split_at(CHROMA_LEN);
        (u, v)
    }

    /// A frame with the given luma and neutral chroma — for tests.
    #[cfg(test)]
    pub(super) fn from_luma(timestamp_secs: f64, luma: &[u8]) -> SampledFrame {
        assert_eq!(
            luma.len(),
            LUMA_LEN,
            "luma plane must be {THUMB_W}x{THUMB_H}"
        );
        let mut planes = luma.to_vec();
        planes.resize(FRAME_LEN, 128);
        SampledFrame {
            timestamp_secs,
            planes,
        }
    }

    /// Fill both chroma planes with flat values — for tests.
    #[cfg(test)]
    pub(super) fn set_chroma_for_test(&mut self, u: u8, v: u8) {
        self.planes[LUMA_LEN..LUMA_LEN + CHROMA_LEN].fill(u);
        self.planes[LUMA_LEN + CHROMA_LEN..].fill(v);
    }
}

/// Decode `media` once and return one thumbnail per `1/fps` seconds.
pub fn sample_thumbnails(tools: &Toolchain, media: &Path, fps: f64) -> Result<Vec<SampledFrame>> {
    let filter = format!("fps={fps},scale={THUMB_W}x{THUMB_H},format=yuv420p");
    let mut child = Command::new(&tools.ffmpeg)
        .args(["-v", "error", "-skip_frame", "noref", "-i"])
        .arg(media)
        .args(["-vf", &filter, "-f", "rawvideo", "-pix_fmt", "yuv420p", "-"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("running ffmpeg frame sampler")?;
    let mut stdout = child
        .stdout
        .take()
        .context("ffmpeg frame sampler has no stdout")?;
    let mut stderr = child
        .stderr
        .take()
        .context("ffmpeg frame sampler has no stderr")?;
    // Drain stderr concurrently so a chatty failure can never block the
    // sampler on a full pipe while we wait for frames.
    let diagnostics = std::thread::spawn(move || {
        let mut text = Vec::new();
        let _ = stderr.read_to_end(&mut text);
        String::from_utf8_lossy(&text).into_owned()
    });
    let mut frames = Vec::new();
    let mut truncated = false;
    loop {
        let mut planes = vec![0u8; FRAME_LEN];
        let got = read_frame(&mut stdout, &mut planes).context("reading sampled frames")?;
        if got == 0 {
            break;
        }
        if got < FRAME_LEN {
            truncated = true;
            break;
        }
        #[allow(clippy::cast_precision_loss)] // frame counts are small
        let timestamp_secs = frames.len() as f64 / fps;
        frames.push(SampledFrame {
            timestamp_secs,
            planes,
        });
    }
    let status = child.wait().context("waiting for ffmpeg frame sampler")?;
    let diagnostics = diagnostics.join().unwrap_or_default();
    if !status.success() {
        bail!("frame sampling failed: {diagnostics}");
    }
    if truncated {
        bail!("frame sampler produced a truncated stream");
    }
    Ok(frames)
}

/// Fill `buf` from `reader`, returning how many bytes arrived before
/// EOF (a short count means the stream was cut mid-frame).
fn read_frame(reader: &mut impl Read, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        match reader.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => {}
            Err(err) => return Err(err),
        }
    }
    Ok(filled)
}
