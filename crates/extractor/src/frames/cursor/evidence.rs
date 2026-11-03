//! Cursor-centred crops for a state change: where the pointer was just
//! before the change, a native crop of that moment (what was about to
//! be clicked) and one of the settled result around the same spot.
//! Only for changes out of a state the pointer was on screen for — a
//! screen recording, not slides.

use std::path::Path;

use super::super::analyze::State;
use super::super::export::{CropRect, FrameRequest};
use super::super::sample::SampledFrame;
use super::super::{CursorEvidence, RegionCrop, FRAMES_DIR};
use super::CursorTrack;
use crate::bundle::MediaInfo;

/// A state with the pointer on screen for at least this share of its
/// samples is a screen recording (ScreenFilter's gate).
pub const GUI_PRESENCE: f64 = 0.8;
/// The crop is this fraction of the frame each way (640x360 at 1080p),
/// centred on the pointer.
const CROP_SHARE: u32 = 3;
/// How many samples back from the change the pointer may last have
/// been seen.
const LOOKBACK: usize = 3;

/// Plan the crops for `states[index]` (never the first state), adding
/// the export requests, when the previous state had the pointer.
#[must_use]
pub fn plan_evidence(
    track: &CursorTrack,
    states: &[State],
    index: usize,
    sampled: &[SampledFrame],
    media: &MediaInfo,
    bundle_dir: &Path,
    requests: &mut Vec<FrameRequest>,
) -> Option<CursorEvidence> {
    let previous = states.get(index.checked_sub(1)?)?;
    let state = states[index];
    if track.presence(previous.start..state.start) < GUI_PRESENCE {
        return None;
    }
    let (before, (x, y)) = (previous.start.max(state.start.saturating_sub(LOOKBACK))..state.start)
        .rev()
        .find_map(|i| track.native_tip(i).map(|tip| (i, tip)))?;
    let rect = around((x, y), media.width, media.height);
    let mut crop = |moment: &str, timestamp_secs: f64| {
        let rel = format!("{FRAMES_DIR}/shot{index:03}-cursor-{moment}.jpg");
        requests.push(FrameRequest::crop(
            timestamp_secs,
            bundle_dir.join(&rel),
            rect,
        ));
        RegionCrop {
            path: rel,
            timestamp_secs,
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
        }
    };
    let before = crop("before", sampled[before].timestamp_secs);
    let after = crop("after", sampled[state.evidence].timestamp_secs);
    Some(CursorEvidence {
        x,
        y,
        before,
        after,
    })
}

/// The crop rectangle centred on `(x, y)`, even-edged and inside the
/// `width`x`height` frame.
fn around((x, y): (u32, u32), width: u32, height: u32) -> CropRect {
    let span = |centre: u32, extent: u32| {
        let len = (extent / CROP_SHARE) & !1;
        let start = centre.saturating_sub(len / 2).min(extent - len) & !1;
        (start, len)
    };
    let (x, width) = span(x, width);
    let (y, height) = span(y, height);
    CropRect {
        x,
        y,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_crop_is_a_third_of_the_frame_around_the_pointer_and_stays_inside() {
        let mid = around((960, 540), 1920, 1080);
        assert_eq!((mid.width, mid.height), (640, 360));
        assert_eq!((mid.x, mid.y), (640, 360));
        let corner = around((5, 1075), 1920, 1080);
        assert_eq!((corner.x, corner.y), (0, 720));
        let odd = around((1279, 0), 1280, 720);
        assert_eq!((odd.x + odd.width, odd.y), (1280, 0));
        assert!(odd.width.is_multiple_of(2) && odd.height.is_multiple_of(2));
    }
}
