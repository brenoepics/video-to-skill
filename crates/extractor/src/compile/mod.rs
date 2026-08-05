//! The skill compiler: a versioned Procedure IR (the contract between
//! the analyzing agent and this tool) compiled into an installable
//! agent-skills package. Pure: same IR + bundle → byte-identical output.

mod emit;

use std::path::Path;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

/// The Procedure IR, schema v1. Authored by the orchestrating agent
/// (usually as `procedure.json` in the bundle) after analysis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcedureIr {
    pub schema_version: u32,
    /// Installed skill directory name; kebab-case.
    pub skill_name: String,
    pub title: String,
    /// Frontmatter description: what the skill does + when to trigger.
    pub description: String,
    /// Markdown body for the generated SKILL.md's overview section.
    pub overview: String,
    pub genre: String,
    pub source: SourceRef,
    pub steps: Vec<Step>,
    #[serde(default)]
    pub artifacts: Vec<OnScreenArtifact>,
    #[serde(default)]
    pub gaps: Vec<String>,
    /// Multi-source history: one record per folded video (set by merge).
    #[serde(default)]
    pub history: Vec<FoldRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FoldRecord {
    pub source: SourceRef,
    /// ISO date the fold happened.
    pub date: String,
    #[serde(default)]
    pub confirmed: Vec<String>,
    #[serde(default)]
    pub added: Vec<String>,
    #[serde(default)]
    pub variants: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceRef {
    /// URL or path the video came from.
    pub original: String,
    pub title: Option<String>,
    pub duration_secs: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Step {
    /// Kebab-case, ordered: "01-quit-vim".
    pub id: String,
    pub goal: String,
    /// Exact actions/commands, markdown. Verbatim-from-pixels where visual.
    pub actions: String,
    pub success_criteria: String,
    /// "high" | "medium" | "low".
    pub confidence: String,
    pub caveats: Option<String>,
    /// At least one entry is required — every step must trace to the video.
    pub evidence: Vec<Evidence>,
    #[serde(default)]
    pub scripts: Vec<ScriptFile>,
    /// Alternative ways other videos perform this step (set by merge —
    /// conflicting sources become variants, never overwrites).
    #[serde(default)]
    pub variants: Vec<StepVariant>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepVariant {
    pub actions: String,
    /// URL/path of the video this variant came from.
    pub source_original: String,
    #[serde(default)]
    pub evidence: Vec<Evidence>,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Evidence {
    pub timestamp_secs: f64,
    /// Bundle-relative frame path backing this claim.
    pub frame: Option<String>,
    /// Transcript or on-screen text quote.
    pub quote: Option<String>,
    /// Originating video when not the IR's primary source (set by merge).
    #[serde(default)]
    pub source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScriptFile {
    pub name: String,
    pub contents: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OnScreenArtifact {
    pub text: String,
    pub timestamp_secs: f64,
    pub frame: Option<String>,
}

/// Validate `ir` and emit the package into `out_dir`, resolving frame
/// references against `bundle_dir`.
pub fn compile(ir: &ProcedureIr, bundle_dir: &Path, out_dir: &Path) -> Result<()> {
    validate(ir, bundle_dir)?;
    emit::emit(ir, bundle_dir, out_dir)
}

fn validate(ir: &ProcedureIr, bundle_dir: &Path) -> Result<()> {
    if ir.schema_version != 1 {
        bail!("unsupported IR schema_version {}", ir.schema_version);
    }
    if !is_kebab(&ir.skill_name) {
        bail!(
            "skill_name '{}' must be kebab-case (lowercase letters, digits, hyphens)",
            ir.skill_name
        );
    }
    if ir.description.trim().is_empty() || ir.description.len() > 1024 {
        bail!("description must be non-empty and at most 1024 characters");
    }
    if ir.steps.is_empty() {
        bail!("a skill needs at least one step — refuse non-procedural videos instead");
    }
    let mut seen = std::collections::HashSet::new();
    for step in &ir.steps {
        if !is_kebab(&step.id) {
            bail!("step id '{}' must be kebab-case", step.id);
        }
        if !seen.insert(&step.id) {
            bail!("duplicate step id '{}'", step.id);
        }
        if step.evidence.is_empty() {
            bail!(
                "step '{}' has no evidence — every step must cite at least one timestamp",
                step.id
            );
        }
        if !matches!(step.confidence.as_str(), "high" | "medium" | "low") {
            bail!(
                "step '{}': confidence must be high, medium, or low",
                step.id
            );
        }
        for evidence in &step.evidence {
            if let Some(frame) = &evidence.frame {
                if !bundle_dir.join(frame).is_file() {
                    bail!("step '{}' cites missing frame '{frame}'", step.id);
                }
            }
        }
    }
    reject_frame_basename_collisions(ir)
}

/// The package flattens every referenced frame into `references/frames/`
/// by basename, so two different source frames sharing a basename would
/// silently overwrite each other. Refuse to compile instead.
fn reject_frame_basename_collisions(ir: &ProcedureIr) -> Result<()> {
    let referenced = ir
        .steps
        .iter()
        .flat_map(|s| &s.evidence)
        .filter_map(|e| e.frame.as_deref())
        .chain(ir.artifacts.iter().filter_map(|a| a.frame.as_deref()));
    let mut by_basename: std::collections::HashMap<&str, &str> = std::collections::HashMap::new();
    for frame in referenced {
        let name = frame.rsplit('/').next().unwrap_or(frame);
        match by_basename.get(name) {
            // The same source frame cited twice is one copy — fine.
            Some(&first) if first != frame => bail!(
                "frame basename collision: '{name}' is used by both '{first}' and '{frame}' — \
                 frames are flattened into references/frames/ by basename, so one would \
                 overwrite the other"
            ),
            _ => {
                by_basename.insert(name, frame);
            }
        }
    }
    Ok(())
}

fn is_kebab(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !s.starts_with('-')
        && !s.ends_with('-')
}
