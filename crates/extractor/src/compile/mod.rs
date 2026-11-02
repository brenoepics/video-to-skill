//! The skill compiler: a versioned Procedure IR (the contract between
//! the analyzing agent and this tool) compiled into an installable
//! agent-skills package. Pure: same IR + bundle → byte-identical output.

mod emit;
pub mod frontmatter;
mod safety;
mod validate;

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
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
    /// Frontmatter `license`: a licence name, or a reference to a file
    /// bundled in the package. Unset means the source video's terms
    /// apply, and the frontmatter says so instead of inventing one.
    #[serde(default)]
    pub license: Option<String>,
    /// Frontmatter `compatibility`: environment requirements — the tool
    /// the skill drives, its version, OS, required binaries. At most
    /// 500 characters. Unset: derived from script interpreters, if any.
    #[serde(default)]
    pub compatibility: Option<String>,
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
    /// sha256 of the media file, copied from the bundle manifest by
    /// [`load_ir`] when the IR does not state it.
    #[serde(default)]
    pub sha256: Option<String>,
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
/// references against `bundle_dir`. `out_dir` must be named after the
/// skill: the Agent Skills spec requires `name` to equal the directory.
pub fn compile(ir: &ProcedureIr, bundle_dir: &Path, out_dir: &Path) -> Result<()> {
    validate::validate(ir, bundle_dir, out_dir)?;
    emit::emit(ir, bundle_dir, out_dir)
}

/// Read a Procedure IR from `path`. When the IR does not state the source
/// media's sha256, take it from the bundle manifest at `bundle_dir` — the
/// manifest is where ingest recorded it, so the package can name the exact
/// bytes it was learned from. A bundle without a manifest (hand-built
/// fixtures) leaves it unset.
pub fn load_ir(path: &Path, bundle_dir: &Path) -> Result<ProcedureIr> {
    let raw = fs::read_to_string(path).with_context(|| {
        format!(
            "no Procedure IR at {} — write it first (see references/ir-format.md)",
            path.display()
        )
    })?;
    let mut ir: ProcedureIr =
        serde_json::from_str(&raw).context("Procedure IR is not valid JSON for schema v1")?;
    if ir.source.sha256.is_none() && bundle_dir.join(crate::bundle::MANIFEST_FILE).is_file() {
        let manifest = crate::bundle::load_manifest(bundle_dir)?;
        if !manifest.source.sha256.is_empty() {
            ir.source.sha256 = Some(manifest.source.sha256);
        }
    }
    Ok(ir)
}
