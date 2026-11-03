//! Turn screen states into shots: which frames to export, where they
//! go, and the `frames.json` records that describe them. Pure
//! planning — the caller exports the requests in one batch.

use std::path::Path;

use crate::bundle::MediaInfo;

use super::analyze::{self, State};
use super::cursor::{self, CursorTrack};
use super::export::FrameRequest;
use super::sample::SampledFrame;
use super::{hash, subdivide, ChangedRegion, FrameConfig, Keyframe, Shot, FRAMES_DIR};

/// Everything the plan is made from.
pub struct Inputs<'a> {
    pub states: &'a [State],
    pub sampled: &'a [SampledFrame],
    pub cursor: &'a CursorTrack,
    pub media: &'a MediaInfo,
    pub bundle_dir: &'a Path,
    pub config: &'a FrameConfig,
}

/// One shot per state. The keyframe (and its native still) sit at the
/// state's evidence frame — the settled result of the change that
/// started it — with a native crop of the region that changed and, in
/// screen recordings, crops around the pointer before and after the
/// change; long states gain periodic interior keyframes.
pub fn plan_shots(inputs: &Inputs) -> (Vec<Shot>, Vec<FrameRequest>) {
    let Inputs {
        states,
        sampled,
        cursor,
        media,
        bundle_dir,
        config,
    } = *inputs;
    let mut shots = Vec::with_capacity(states.len());
    let mut requests = Vec::new();
    for (index, state) in states.iter().enumerate() {
        let end = states
            .get(index + 1)
            .map_or(sampled.len(), |next| next.start);
        let start_secs = sampled[state.start].timestamp_secs;
        let end_secs = sampled
            .get(end)
            .map_or(media.duration_secs, |f| f.timestamp_secs);
        let evidence = &sampled[state.evidence];
        let changed_region = state.region.map(|region| {
            let rect = region.to_native(media.width, media.height);
            let rel = format!("{FRAMES_DIR}/shot{index:03}-changed.jpg");
            requests.push(FrameRequest::crop(
                evidence.timestamp_secs,
                bundle_dir.join(&rel),
                rect,
            ));
            ChangedRegion {
                path: rel,
                x: rect.x,
                y: rect.y,
                width: rect.width,
                height: rect.height,
            }
        });
        let kf_rel = format!("{FRAMES_DIR}/shot{index:03}.jpg");
        let native_rel = format!("{FRAMES_DIR}/shot{index:03}-native.jpg");
        requests.push(FrameRequest::bounded(
            evidence.timestamp_secs,
            bundle_dir.join(&kf_rel),
            config.max_keyframe_width,
        ));
        requests.push(FrameRequest::native(
            evidence.timestamp_secs,
            bundle_dir.join(&native_rel),
        ));
        let interior_keyframes = subdivide::plan_interior_keyframes(
            &subdivide::ShotSpan {
                index,
                start_secs,
                end_secs,
                main_keyframe_secs: evidence.timestamp_secs,
                samples: &sampled[state.start..end],
            },
            config,
        );
        requests.extend(interior_keyframes.iter().map(|kf| {
            FrameRequest::bounded(
                kf.timestamp_secs,
                bundle_dir.join(&kf.path),
                config.max_keyframe_width,
            )
        }));
        shots.push(Shot {
            index,
            start_secs,
            end_secs,
            motion_density: analyze::motion_density(&sampled[state.start..end]),
            keyframe: Keyframe {
                timestamp_secs: evidence.timestamp_secs,
                path: kf_rel,
                native_path: Some(native_rel),
                hash: format!("{:016x}", hash::thumbnail_hash(evidence.luma())),
            },
            interior_keyframes,
            changed_region,
            cursor_presence: Some(cursor.presence(state.start..end)),
            cursor: cursor::evidence::plan_evidence(
                cursor,
                states,
                index,
                sampled,
                media,
                bundle_dir,
                &mut requests,
            ),
        });
    }
    (shots, requests)
}
