//! Turn screen states into shots: which frames to export, where they
//! go, and the `frames.json` records that describe them. Pure
//! planning — the caller exports the requests in one batch.

use std::path::Path;

use super::analyze::{self, State};
use super::export::FrameRequest;
use super::sample::SampledFrame;
use super::{subdivide, FrameConfig, Keyframe, Shot, FRAMES_DIR};

/// One shot per state. The keyframe (and its native still) sit at the
/// state's evidence frame — the settled result of the change that
/// started it — and long states gain periodic interior keyframes.
pub fn plan_shots(
    states: &[State],
    sampled: &[SampledFrame],
    duration_secs: f64,
    bundle_dir: &Path,
    config: &FrameConfig,
) -> (Vec<Shot>, Vec<FrameRequest>) {
    let mut shots = Vec::with_capacity(states.len());
    let mut requests = Vec::new();
    for (index, state) in states.iter().enumerate() {
        let end = states
            .get(index + 1)
            .map_or(sampled.len(), |next| next.start);
        let start_secs = sampled[state.start].timestamp_secs;
        let end_secs = sampled.get(end).map_or(duration_secs, |f| f.timestamp_secs);
        let evidence = &sampled[state.evidence];
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
                hash: format!("{:016x}", analyze::thumbnail_hash(evidence.luma())),
            },
            interior_keyframes,
        });
    }
    (shots, requests)
}
