//! Building the action track: gate, cut the detector's clips, detect,
//! cut one clip per detected action, parameterize, and write
//! `actions.json`. The clips stay in the bundle's `actions/` directory
//! (rebuilt on each run) so a runner can be re-run by hand.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};

use super::backend::{Batch, ClipRequest, IdmBackend, Stage};
use super::parse::{self, Detection};
use super::prompts::{
    detector_request_size, dynamic_fps, parameterizer_request_size, DETECTOR_FPS, DETECTOR_PROMPT,
    DETECTOR_SYSTEM, MAX_NEW_TOKENS, PARAMETERIZER_PROMPT, PARAMETERIZER_SYSTEM,
};
use super::{gate, save_actions, Action, ActionEvent, ActionTrack, Window, ACTIONS_FILE};
use crate::bundle::{load_manifest, save_manifest};
use crate::deps::registry::{IDM_DETECTOR, IDM_PARAMETERIZER};
use crate::frames::load_frame_track;
use crate::tools::Toolchain;

const CLIPS_DIR: &str = "actions";
/// Upstream pads an action clip by half a second, so the result of the
/// action is in it, but stops short of the detector clip's last frame.
const RESULT_PAD_SECS: f64 = 0.5;
const END_MARGIN_SECS: f64 = 0.1;

/// The installed model directories, one per stage.
#[derive(Debug, Clone, Copy)]
pub struct Models<'a> {
    pub detector: &'a Path,
    pub parameterizer: &'a Path,
}

/// Where clips come from and go, and the source's geometry.
struct Cutter<'a> {
    tools: &'a Toolchain,
    media: PathBuf,
    clips: PathBuf,
    size: (u32, u32),
    fps: f64,
}

/// One detected action waiting for its parameters.
struct Pending {
    start_secs: f64,
    end_secs: f64,
    label: String,
    request: Option<ClipRequest>,
}

/// Label the bundle's screen recording and write `actions.json`.
pub fn extract_action_track(
    bundle_dir: &Path,
    tools: &Toolchain,
    backend: &dyn IdmBackend,
    models: &Models<'_>,
) -> Result<ActionTrack> {
    let mut manifest = load_manifest(bundle_dir)?;
    let windows = gate::screen_recording_windows(&load_frame_track(bundle_dir)?.shots);
    let cutter = Cutter {
        tools,
        media: PathBuf::from(&manifest.source.media_path),
        clips: bundle_dir.join(CLIPS_DIR),
        size: (manifest.media.width, manifest.media.height),
        fps: manifest.media.fps,
    };
    if cutter.clips.exists() {
        fs::remove_dir_all(&cutter.clips)?;
    }
    fs::create_dir_all(&cutter.clips)?;

    let detect = windows
        .iter()
        .enumerate()
        .map(|(i, window)| cutter.window_clip(i, window))
        .collect::<Result<Vec<_>>>()?;
    let answers = run(backend, Stage::Detect, models.detector, &detect)?;
    let mut notes = Vec::new();
    let mut pending = Vec::new();
    for ((i, window), answer) in windows.iter().enumerate().zip(answers) {
        let Some(text) = answer else {
            notes.push(format!(
                "no detector answer for {:.1}-{:.1}s",
                window.start_secs, window.end_secs
            ));
            continue;
        };
        let found = parse::detections(&text).unwrap_or_default();
        for (j, detection) in found.into_iter().enumerate() {
            pending.push(cutter.action_clip(i, j, window, detection)?);
        }
    }

    let identify: Vec<ClipRequest> = pending.iter().filter_map(|p| p.request.clone()).collect();
    let answers = run(backend, Stage::Identify, models.parameterizer, &identify)?;
    let events = type_events(pending, answers, &mut notes);

    let track = ActionTrack {
        schema_version: 1,
        detector: IDM_DETECTOR.label(),
        parameterizer: IDM_PARAMETERIZER.label(),
        windows,
        events,
        notes,
    };
    save_actions(bundle_dir, &track)?;
    manifest.tracks.actions = Some(ACTIONS_FILE.to_string());
    save_manifest(bundle_dir, &manifest)?;
    Ok(track)
}

/// Record in the manifest why an opted-in bundle has no action track;
/// returns the note. Idempotent.
pub fn record_unavailable(bundle_dir: &Path, reason: &str) -> Result<String> {
    let mut manifest = load_manifest(bundle_dir)?;
    let note = format!("actions: {reason}");
    if !manifest.notes.contains(&note) {
        manifest.notes.push(note.clone());
        save_manifest(bundle_dir, &manifest)?;
    }
    Ok(note)
}

impl Cutter<'_> {
    fn window_clip(&self, index: usize, window: &Window) -> Result<ClipRequest> {
        let id = format!("w{index:03}");
        let clip = self.clips.join(format!("{id}.mp4"));
        cut(
            self.tools,
            &self.media,
            window.start_secs,
            window.end_secs,
            &clip,
        )?;
        let size = detector_request_size(self.size.0, self.size.1);
        Ok(request(id, clip, DETECTOR_FPS, size, Stage::Detect))
    }

    /// Place a clip-relative detection on the video's timeline (a time
    /// outside the clip is pulled in) and cut its action clip, when the
    /// padded span is not empty.
    fn action_clip(
        &self,
        window_index: usize,
        index: usize,
        window: &Window,
        found: Detection,
    ) -> Result<Pending> {
        let span = window.end_secs - window.start_secs;
        let start = found.start_secs.clamp(0.0, span);
        let start_secs = window.start_secs + start;
        let end_secs = window.start_secs + found.end_secs.clamp(start, span);
        let clip_end = (end_secs + RESULT_PAD_SECS).min(window.end_secs - END_MARGIN_SECS);
        let mut request_for_clip = None;
        if clip_end > start_secs {
            let id = format!("w{window_index:03}-a{index:02}");
            let clip = self.clips.join(format!("{id}.mp4"));
            cut(self.tools, &self.media, start_secs, clip_end, &clip)?;
            let secs = clip_end - start_secs;
            let size = parameterizer_request_size(self.size.0, self.size.1, secs, self.fps);
            let fps = dynamic_fps(secs, self.fps);
            request_for_clip = Some(request(id, clip, fps, size, Stage::Identify));
        }
        Ok(Pending {
            start_secs,
            end_secs,
            label: found.label,
            request: request_for_clip,
        })
    }
}

/// Typed events in time order: stage 2's calls when it named any, else
/// the bare type stage 1 detected, else a note.
fn type_events(
    pending: Vec<Pending>,
    answers: Vec<Option<String>>,
    notes: &mut Vec<String>,
) -> Vec<ActionEvent> {
    let mut answers = answers.into_iter();
    let mut events = Vec::new();
    for item in pending {
        let typed = item
            .request
            .as_ref()
            .and_then(|request| {
                let text = answers.next().flatten()?;
                Some(parse::calls(
                    &text,
                    (request.resized_width, request.resized_height),
                ))
            })
            .unwrap_or_default();
        let (actions, parameters_by) = if typed.is_empty() {
            let Some(action) = parse::from_label(&item.label) else {
                notes.push(format!(
                    "skipped an action of unknown type '{}' at {:.1}s",
                    item.label, item.start_secs
                ));
                continue;
            };
            (vec![action], None)
        } else {
            (typed, Some(IDM_PARAMETERIZER.label()))
        };
        events.extend(
            actions
                .into_iter()
                .map(|action| event(&item, action, parameters_by.clone())),
        );
    }
    events.sort_by(|a, b| a.start_secs.total_cmp(&b.start_secs));
    events
}

fn event(item: &Pending, action: Action, parameters_by: Option<String>) -> ActionEvent {
    ActionEvent {
        start_secs: round_ms(item.start_secs),
        end_secs: round_ms(item.end_secs),
        confidence: parse::published_f1(&action),
        action,
        detected_by: IDM_DETECTOR.label(),
        parameters_by,
    }
}

fn round_ms(secs: f64) -> f64 {
    (secs * 1000.0).round() / 1000.0
}

fn request(id: String, clip: PathBuf, fps: f64, size: (u32, u32), stage: Stage) -> ClipRequest {
    let (system, prompt) = match stage {
        Stage::Detect => (DETECTOR_SYSTEM, DETECTOR_PROMPT),
        Stage::Identify => (PARAMETERIZER_SYSTEM, PARAMETERIZER_PROMPT),
    };
    ClipRequest {
        id,
        clip,
        fps,
        resized_width: size.0,
        resized_height: size.1,
        system,
        prompt,
        max_new_tokens: MAX_NEW_TOKENS,
    }
}

/// One batch per stage; an empty stage runs nothing.
fn run(
    backend: &dyn IdmBackend,
    stage: Stage,
    model_dir: &Path,
    requests: &[ClipRequest],
) -> Result<Vec<Option<String>>> {
    if requests.is_empty() {
        return Ok(Vec::new());
    }
    let answers = backend.run(&Batch {
        stage,
        model_dir,
        requests,
    })?;
    if answers.len() != requests.len() {
        bail!(
            "the {stage:?} backend answered {} of {} clips",
            answers.len(),
            requests.len()
        );
    }
    Ok(answers)
}

/// Cut `start..end` of `media` into `dest`, re-encoded with ffmpeg's
/// built-in MPEG-4 encoder so every build can write it.
fn cut(tools: &Toolchain, media: &Path, start: f64, end: f64, dest: &Path) -> Result<()> {
    let output = Command::new(&tools.ffmpeg)
        .args(["-nostdin", "-y", "-loglevel", "error", "-ss"])
        .arg(format!("{start:.3}"))
        .arg("-i")
        .arg(media)
        .arg("-t")
        .arg(format!("{:.3}", end - start))
        .args(["-an", "-c:v", "mpeg4", "-q:v", "2"])
        .arg(dest)
        .output()
        .context("running ffmpeg to cut an action clip")?;
    if !output.status.success() || !dest.is_file() {
        bail!(
            "cutting {start:.1}-{end:.1}s failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}
