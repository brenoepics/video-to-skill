//! The mid-resolution decode the pointer is tracked on: the same
//! sampling as the thumbnails (one `fps` filter, non-reference frames
//! skipped), scaled to [`MID_W`]x[`MID_H`] gray and handed over one
//! plane at a time so an hour of video never sits in memory at once.

use std::path::Path;

use anyhow::Result;

use super::super::sample::stream_raw;
use super::{MID_H, MID_W};
use crate::tools::Toolchain;

/// Decode `media` once and hand each sampled gray plane to `on_frame`.
pub fn stream_gray(
    tools: &Toolchain,
    media: &Path,
    fps: f64,
    on_frame: &mut dyn FnMut(Vec<u8>),
) -> Result<()> {
    let filter = format!("fps={fps},scale={MID_W}x{MID_H},format=gray");
    stream_raw(tools, media, &filter, "gray", MID_W * MID_H, on_frame)
}
