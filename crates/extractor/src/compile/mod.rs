//! The skill compiler: a versioned Procedure IR (the contract between
//! the analyzing agent and this tool) compiled into an installable
//! agent-skills package. Pure: same IR + bundle → byte-identical output.

mod emit;
pub mod frontmatter;
pub mod graph;
pub mod repairs;
mod safety;
mod validate;

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub use graph::{TaskGraph, Transition};

/// The current Procedure IR schema. v1 IRs still compile (see
/// [`deprecation_note`]); packages are always written at this version.
pub const SCHEMA_VERSION: u32 = 2;

/// The Procedure IR, schema v2 (v1 accepted). Authored by the
/// orchestrating agent (usually as `procedure.json` in the bundle) after
/// analysis; `vts-extract verify` and `merge` update the per-step history.
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
    /// Keysteps and the transitions every folded video took between
    /// them. Empty when authored; the compiler derives the single
    /// video's chain, and each fold adds a walk.
    #[serde(default)]
    pub task_graph: TaskGraph,
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
    /// Step orders this video contradicted, as "`a` before `b` (n vs m
    /// videos)": the lighter direction of a bidirectional edge, which the
    /// merged order does not follow.
    #[serde(default)]
    pub order_variants: Vec<String>,
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
    /// State that must hold before the step runs (v2): "Vim is open on a
    /// modified buffer". The verifier establishes it before the actions.
    #[serde(default)]
    pub preconditions: Vec<String>,
    /// Inputs abstracted out of the video (v2), referenced from `actions`
    /// as `{name}`: the video's `TODO` becomes `{search-term}`.
    #[serde(default)]
    pub parameters: Vec<Parameter>,
    /// A mechanically checkable completion condition (v2) — a command to
    /// run or a state to observe — unlike the prose `success_criteria`.
    #[serde(default)]
    pub postcondition: Option<String>,
    /// Past failures and the corrections learned (v2); `vts-extract
    /// verify` appends unsolved failures.
    #[serde(default)]
    pub failure_notes: Vec<String>,
    /// Executions observed so far (v2): each recorded pass or fail adds one.
    #[serde(default)]
    pub usage_count: u32,
    /// The verifier's latest verdict (v2), written into provenance.json by
    /// `vts-extract verify` and cleared by a fold.
    #[serde(default)]
    pub verdict: Option<Verdict>,
}

/// An input the step abstracts over, so one skill serves many values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Parameter {
    /// Kebab-case; appears in `actions` as `{name}`.
    pub name: String,
    pub description: String,
    /// The value the video used, when it is safe to show.
    #[serde(default)]
    pub example: Option<String>,
}

/// What the verifier last found when it ran the step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Verdict {
    /// `pass` | `fail` | `skipped` | `unverifiable`.
    pub outcome: String,
    pub detail: String,
    /// External signals cited by the repairs that led here.
    #[serde(default)]
    pub repairs: Vec<String>,
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
    let ir = safety::flag(ir)?;
    emit::emit(&ir, bundle_dir, out_dir)
}

/// The note a command prints when it reads an IR written at an older
/// schema: it still compiles, and what v2 adds.
#[must_use]
pub fn deprecation_note(ir: &ProcedureIr) -> Option<String> {
    (ir.schema_version < SCHEMA_VERSION).then(|| {
        format!(
            "note: Procedure IR schema v{} is deprecated — it still compiles, and the package \
             is written as v{SCHEMA_VERSION}, but v2 adds per-step preconditions, \
             parameters, postcondition, failure_notes and usage_count (see \
             references/ir-format.md)",
            ir.schema_version
        )
    })
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
