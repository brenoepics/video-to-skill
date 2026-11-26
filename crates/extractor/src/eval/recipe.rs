//! A gold video's recipe: the timed stills and narration cues that
//! generate it deterministically, which also fix the ground truth —
//! every still transition not marked `continuous` is a visual change,
//! and the `step` labels on stills give each gold step its time span.

use std::collections::BTreeSet;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

use super::metrics::Span;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recipe {
    pub schema_version: u32,
    /// Kebab-case identifier; doubles as the gold IR's `skill_name`.
    pub id: String,
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub duration_secs: f64,
    pub stills: Vec<Still>,
    #[serde(default)]
    pub narration: Vec<Cue>,
}

/// One screen shown from `t` until the next still.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Still {
    pub t: f64,
    /// The gold step this still belongs to; `None` for intro/outro.
    #[serde(default)]
    pub step: Option<String>,
    /// A transition that is not a visual change: scrolling or streaming
    /// output inside one state.
    #[serde(default)]
    pub continuous: bool,
    #[serde(flatten)]
    pub screen: Screen,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Screen {
    Terminal(TerminalOp),
    Dialog(Dialog),
    Slide(Slide),
    Desktop(Desktop),
}

/// Edits applied to the persistent terminal, in field order.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TerminalOp {
    /// Replace the whole screen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lines: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    /// Command typed at the prompt (shown, not yet run).
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "type")]
    pub typed: Option<String>,
    /// Output printed under the command, followed by a fresh prompt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Dialog {
    pub title: String,
    #[serde(default)]
    pub rows: Vec<String>,
    #[serde(default)]
    pub buttons: Vec<String>,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Slide {
    pub title: String,
    #[serde(default)]
    pub bullets: Vec<String>,
    #[serde(default)]
    pub footer: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Desktop {
    #[serde(default)]
    pub status: Option<String>,
}

/// Narration spoken from `t`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cue {
    pub t: f64,
    pub text: String,
}

/// A gold step's time span, from its first still to the next step's.
#[derive(Debug, Clone, PartialEq)]
pub struct StepSpan {
    pub id: String,
    pub span: Span,
}

impl Recipe {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != 1 {
            bail!("recipe {}: unsupported schema_version", self.id);
        }
        crate::safe_path::validate_skill_name(&self.id)?;
        if self.width == 0 || self.height == 0 || self.fps == 0 || self.duration_secs <= 0.0 {
            bail!(
                "recipe {}: size, fps and duration must be positive",
                self.id
            );
        }
        let Some(first) = self.stills.first() else {
            bail!("recipe {}: needs at least one still", self.id);
        };
        if first.t != 0.0 {
            bail!("recipe {}: the first still must start at t=0", self.id);
        }
        for pair in self.stills.windows(2) {
            if pair[1].t <= pair[0].t {
                bail!(
                    "recipe {}: still times must increase (at t={})",
                    self.id,
                    pair[1].t
                );
            }
        }
        if self.stills.iter().any(|s| s.t >= self.duration_secs) {
            bail!("recipe {}: every still must start before the end", self.id);
        }
        self.validate_steps()?;
        for pair in self.narration.windows(2) {
            if pair[1].t <= pair[0].t {
                bail!(
                    "recipe {}: narration cues must increase (at t={})",
                    self.id,
                    pair[1].t
                );
            }
        }
        if self
            .narration
            .iter()
            .any(|c| c.t >= self.duration_secs || c.text.trim().is_empty())
        {
            bail!(
                "recipe {}: narration cues need text and must start before the end",
                self.id
            );
        }
        Ok(())
    }

    /// Steps label contiguous runs of stills, each id used once.
    fn validate_steps(&self) -> Result<()> {
        let mut seen = BTreeSet::new();
        let mut previous: Option<&str> = None;
        for still in &self.stills {
            let current = still.step.as_deref();
            if let Some(id) = current {
                crate::safe_path::validate_step_id(id)?;
                if current != previous && !seen.insert(id) {
                    bail!(
                        "recipe {}: step '{id}' labels two separate runs of stills",
                        self.id
                    );
                }
            }
            previous = current;
        }
        Ok(())
    }

    /// Times at which the screen content changes — every non-continuous
    /// still after the first.
    #[must_use]
    pub fn change_times(&self) -> Vec<f64> {
        self.stills
            .iter()
            .skip(1)
            .filter(|s| !s.continuous)
            .map(|s| s.t)
            .collect()
    }

    /// Gold step spans in video order.
    #[must_use]
    pub fn step_spans(&self) -> Vec<StepSpan> {
        let mut spans: Vec<StepSpan> = Vec::new();
        for (idx, still) in self.stills.iter().enumerate() {
            let end = self
                .stills
                .get(idx + 1)
                .map_or(self.duration_secs, |next| next.t);
            match (&still.step, spans.last_mut()) {
                (Some(id), Some(last)) if last.id == *id => last.span.end_secs = end,
                (Some(id), _) => spans.push(StepSpan {
                    id: id.clone(),
                    span: Span::new(still.t, end),
                }),
                (None, _) => {}
            }
        }
        spans
    }

    /// Still durations in order, the last one running to the end.
    #[must_use]
    pub fn still_durations(&self) -> Vec<f64> {
        self.stills
            .iter()
            .enumerate()
            .map(|(idx, still)| {
                self.stills
                    .get(idx + 1)
                    .map_or(self.duration_secs, |next| next.t)
                    - still.t
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
