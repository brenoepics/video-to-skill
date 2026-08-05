//! The timeline: the bundle's centerpiece — transcript and frame track
//! merged into ordered, aligned segments the orchestrating agent reads
//! first. Token-lean by design: speech text inline, everything else
//! (word timings, images) referenced by path for on-demand loading.
//!
//! Degrades honestly: with only one track available it still assembles,
//! recording the gap in `notes`.

mod align;

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::bundle::{load_manifest, load_transcript, save_manifest};
use crate::frames::load_frame_track;

pub const TIMELINE_FILE: &str = "timeline.json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Timeline {
    pub schema_version: u32,
    pub duration_secs: f64,
    pub language: Option<String>,
    #[serde(default)]
    pub notes: Vec<String>,
    pub segments: Vec<TimelineSegment>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimelineSegment {
    pub index: usize,
    pub start_secs: f64,
    pub end_secs: f64,
    pub motion_density: Option<f64>,
    pub keyframe: Option<KeyframeRef>,
    pub speech: Vec<SpeechSpan>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeyframeRef {
    /// Bundle-relative path of the agent-readable JPEG.
    pub path: String,
    pub native_path: Option<String>,
    pub timestamp_secs: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpeechSpan {
    pub start_secs: f64,
    pub end_secs: f64,
    pub text: String,
}

pub fn load_timeline(bundle_dir: &Path) -> Result<Timeline> {
    let raw = fs::read_to_string(bundle_dir.join(TIMELINE_FILE))
        .with_context(|| format!("no timeline at {}", bundle_dir.display()))?;
    serde_json::from_str(&raw).context("timeline artifact is not valid")
}

/// Merge whichever tracks exist into `timeline.json`.
pub fn assemble(bundle_dir: &Path) -> Result<Timeline> {
    let mut manifest = load_manifest(bundle_dir)?;
    let duration = manifest.media.duration_secs;

    let transcript = load_transcript(bundle_dir).ok();
    let frame_shots = load_frame_track(bundle_dir).ok().map(|t| t.shots);

    let mut notes = Vec::new();
    if transcript.is_none() {
        notes.push("timeline: transcript unavailable — segments carry no speech".to_string());
    }
    if frame_shots.is_none() {
        notes.push("timeline: frame track unavailable — single whole-video segment".to_string());
    }

    // Segment skeleton: from shots when available, else one whole-video span.
    let segments_base: Vec<TimelineSegment> = match &frame_shots {
        Some(shots) => shots
            .iter()
            .map(|s| TimelineSegment {
                index: s.index,
                start_secs: s.start_secs,
                end_secs: s.end_secs,
                motion_density: Some(s.motion_density),
                keyframe: Some(KeyframeRef {
                    path: s.keyframe.path.clone(),
                    native_path: s.keyframe.native_path.clone(),
                    timestamp_secs: s.keyframe.timestamp_secs,
                }),
                speech: Vec::new(),
            })
            .collect(),
        None => vec![TimelineSegment {
            index: 0,
            start_secs: 0.0,
            end_secs: duration,
            motion_density: None,
            keyframe: None,
            speech: Vec::new(),
        }],
    };

    let mut segments = segments_base;
    if let Some(transcript) = &transcript {
        let ranges: Vec<(f64, f64)> = segments
            .iter()
            .map(|s| (s.start_secs, s.end_secs))
            .collect();
        let assigned = align::assign_speech(&transcript.segments, &ranges);
        for (segment, indices) in segments.iter_mut().zip(assigned) {
            segment.speech = indices
                .into_iter()
                .map(|i| {
                    let s = &transcript.segments[i];
                    SpeechSpan {
                        start_secs: s.start_secs,
                        end_secs: s.end_secs,
                        text: s.text.clone(),
                    }
                })
                .collect();
        }
    }

    let timeline = Timeline {
        schema_version: 1,
        duration_secs: duration,
        language: transcript.as_ref().and_then(|t| t.language.clone()),
        notes,
        segments,
    };
    let tmp = bundle_dir.join(format!("{TIMELINE_FILE}.tmp"));
    fs::write(&tmp, serde_json::to_string_pretty(&timeline)?)?;
    fs::rename(&tmp, bundle_dir.join(TIMELINE_FILE))?;
    manifest.tracks.timeline = Some(TIMELINE_FILE.to_string());
    save_manifest(bundle_dir, &manifest)?;
    Ok(timeline)
}
