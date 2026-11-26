//! The inference seam. The models are 16.6 GB Qwen2.5-VL checkpoints
//! that only run well on a GPU under `transformers` (or a server built
//! on it), so the extractor never links an inference engine: it cuts the
//! clips, sizes them, writes the prompts, and hands one batch per stage
//! to a backend. [`SubprocessRunner`] is the real backend — any program
//! that speaks the JSON protocol below — and tests script their own.
//!
//! Protocol (version 1): the runner gets one JSON object on stdin,
//! `{"protocol": 1, "stage": "detect" | "identify", "model_dir": ...,
//! "requests": [{"id", "clip", "fps", "resized_width", "resized_height",
//! "system", "prompt", "max_new_tokens"}]}`, loads the model in
//! `model_dir` once, and prints `{"responses": [{"id", "text"}]}` on
//! stdout: each clip's decoded answer, verbatim. A request it could not
//! answer is left out. Its stderr is the user's progress display.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    /// Stage 1: which actions happen in a clip, and when.
    Detect,
    /// Stage 2: one action's parameters.
    Identify,
}

/// One clip for one model, sized and prompted as upstream does.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ClipRequest {
    pub id: String,
    pub clip: PathBuf,
    pub fps: f64,
    pub resized_width: u32,
    pub resized_height: u32,
    pub system: &'static str,
    pub prompt: &'static str,
    pub max_new_tokens: u32,
}

/// Every request of one stage: one model load answers them all.
#[derive(Debug, Clone, Serialize)]
pub struct Batch<'a> {
    pub stage: Stage,
    pub model_dir: &'a Path,
    pub requests: &'a [ClipRequest],
}

/// Runs a model over a batch; one answer (or none) per request, in
/// request order.
pub trait IdmBackend {
    fn run(&self, batch: &Batch<'_>) -> Result<Vec<Option<String>>>;
}

/// The real backend: a program the user installed beside their model
/// runtime (`scripts/idm-runner.py` is the reference one).
#[derive(Debug, Clone)]
pub struct SubprocessRunner {
    pub program: PathBuf,
}

#[derive(Serialize)]
struct Envelope<'a> {
    protocol: u32,
    #[serde(flatten)]
    batch: &'a Batch<'a>,
}

#[derive(Deserialize)]
struct Answers {
    responses: Vec<Answer>,
}

#[derive(Deserialize)]
struct Answer {
    id: String,
    text: String,
}

impl IdmBackend for SubprocessRunner {
    fn run(&self, batch: &Batch<'_>) -> Result<Vec<Option<String>>> {
        let input = serde_json::to_vec(&Envelope {
            protocol: PROTOCOL_VERSION,
            batch,
        })?;
        let mut child = crate::deps::retry_while_busy(|| {
            Command::new(&self.program)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()
        })
        .with_context(|| format!("starting the action runner {}", self.program.display()))?;
        let mut stdin = child.stdin.take().context("action runner has no stdin")?;
        let writer = std::thread::spawn(move || stdin.write_all(&input));
        let output = child
            .wait_with_output()
            .context("waiting for the action runner")?;
        let written = writer.join();
        if !output.status.success() {
            bail!(
                "action runner {} failed ({})",
                self.program.display(),
                output.status
            );
        }
        if !matches!(written, Ok(Ok(()))) {
            bail!("action runner closed its input before reading the batch");
        }
        let answers: Answers = serde_json::from_slice(&output.stdout)
            .context("action runner printed no valid `responses` JSON")?;
        Ok(batch
            .requests
            .iter()
            .map(|request| {
                answers
                    .responses
                    .iter()
                    .find(|answer| answer.id == request.id)
                    .map(|answer| answer.text.clone())
            })
            .collect())
    }
}
