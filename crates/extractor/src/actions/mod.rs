//! The action track (opt-in, `extract --actions`): typed GUI events —
//! click, drag, scroll, type, keypress — labelled by VideoAgentTrek's
//! inverse-dynamics models (Video2Action) on the stretches the cursor
//! gate marks as screen recording, written to `actions.json`.
//!
//! Like OCR, an event is a hint for where to look, never a step: the
//! agent reads the frames around it, and a coordinate never reaches a
//! skill (steps stay intent-level). The models are two 16.6 GB
//! Qwen2.5-VL-7B fine-tunes, so inference runs out of process behind
//! [`IdmBackend`]; [`SubprocessRunner`] is the real one.

mod backend;
mod gate;
mod parse;
mod prompts;
mod track;

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub use backend::{Batch, ClipRequest, IdmBackend, Stage, SubprocessRunner};
pub use gate::{screen_recording_windows, MAX_WINDOW_SECS};
pub use prompts::{detector_request_size, dynamic_fps, parameterizer_request_size};
pub use track::{discard_track, extract_action_track, record_unavailable, Models};

pub const ACTIONS_FILE: &str = "actions.json";

/// Everything the two models said about the bundle's screen recording.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionTrack {
    pub schema_version: u32,
    /// `repo@revision` of the stage-1 model.
    pub detector: String,
    /// `repo@revision` of the stage-2 model.
    pub parameterizer: String,
    /// The clips the detector read, on the video's timeline — so an empty
    /// `events` means "no action found here", not "never looked".
    pub windows: Vec<Window>,
    pub events: Vec<ActionEvent>,
    /// What could not be read or typed, e.g. an unknown action label.
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Window {
    pub start_secs: f64,
    pub end_secs: f64,
}

/// One GUI interaction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionEvent {
    pub start_secs: f64,
    pub end_secs: f64,
    #[serde(flatten)]
    pub action: Action,
    /// The published detection F1 for this action type (VideoAgentTrek,
    /// Table 1): the models emit no per-event score, so this is how far
    /// the type is to be trusted, not this event.
    pub confidence: f64,
    /// `repo@revision` of the model that found the event.
    pub detected_by: String,
    /// `repo@revision` of the model that named its parameters, or none
    /// when only the detected type is known.
    #[serde(default)]
    pub parameters_by: Option<String>,
}

/// What was done. Points are fractions of the frame, origin top-left.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Action {
    Click {
        button: Button,
        /// 2 for a double click.
        count: u32,
        #[serde(default)]
        at: Option<Point>,
    },
    Drag {
        /// Where the drag ended; it starts where the pointer was.
        #[serde(default)]
        to: Option<Point>,
    },
    Scroll {
        /// Positive scrolls up, negative down (the model's convention).
        #[serde(default)]
        pixels: Option<f64>,
        #[serde(default)]
        at: Option<Point>,
    },
    Type {
        #[serde(default)]
        text: Option<String>,
    },
    Keypress {
        #[serde(default)]
        keys: Vec<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Button {
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

pub fn load_actions(bundle_dir: &Path) -> Result<ActionTrack> {
    let raw = fs::read_to_string(bundle_dir.join(ACTIONS_FILE))
        .with_context(|| format!("no action track at {}", bundle_dir.display()))?;
    serde_json::from_str(&raw).context("action track artifact is not valid")
}

pub fn save_actions(bundle_dir: &Path, track: &ActionTrack) -> Result<()> {
    fs::create_dir_all(bundle_dir)?;
    let path = bundle_dir.join(ACTIONS_FILE);
    crate::fsx::write_atomic(&path, &serde_json::to_vec_pretty(track)?)?;
    Ok(())
}
