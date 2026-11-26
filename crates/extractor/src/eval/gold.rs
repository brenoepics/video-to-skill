//! Loading the gold set: one directory per video holding `recipe.json`
//! (how to generate it, and the truth that falls out of that) and
//! `procedure.json` (the Procedure IR an ideal analysis would write).

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use super::metrics::{backtick_commands, Span};
use super::recipe::Recipe;
use crate::compile::{ProcedureIr, SCHEMA_VERSION};

pub const RECIPE_FILE: &str = "recipe.json";
pub const PROCEDURE_FILE: &str = "procedure.json";

/// A gold step: its IR id, its span in the video, and the verbatim
/// commands (backtick spans of the IR's `actions`).
#[derive(Debug, Clone, PartialEq)]
pub struct GoldStep {
    pub id: String,
    pub span: Span,
    pub commands: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GoldVideo {
    pub id: String,
    pub dir: PathBuf,
    pub recipe: Recipe,
    pub procedure: ProcedureIr,
    pub steps: Vec<GoldStep>,
}

impl GoldVideo {
    /// Every gold command across the steps.
    #[must_use]
    pub fn command_count(&self) -> usize {
        self.steps.iter().map(|s| s.commands.len()).sum()
    }
}

/// Load every gold video under `root` (each subdirectory with a
/// recipe), sorted by id.
pub fn load_gold_set(root: &Path) -> Result<Vec<GoldVideo>> {
    let entries = fs::read_dir(root)
        .with_context(|| format!("cannot read the gold set at {}", root.display()))?;
    let mut dirs: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.join(RECIPE_FILE).is_file())
        .collect();
    dirs.sort();
    if dirs.is_empty() {
        bail!(
            "no gold videos under {} (expected <id>/{RECIPE_FILE})",
            root.display()
        );
    }
    dirs.iter().map(|dir| load_gold_video(dir)).collect()
}

/// Load and cross-check one gold video directory.
pub fn load_gold_video(dir: &Path) -> Result<GoldVideo> {
    let recipe: Recipe = read_json(&dir.join(RECIPE_FILE))?;
    recipe.validate()?;
    let dir_name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if dir_name != recipe.id {
        bail!(
            "gold directory '{dir_name}' must be named after its recipe id '{}'",
            recipe.id
        );
    }
    let procedure: ProcedureIr = read_json(&dir.join(PROCEDURE_FILE))?;
    let steps = cross_check(&recipe, &procedure)?;
    Ok(GoldVideo {
        id: recipe.id.clone(),
        dir: dir.to_path_buf(),
        recipe,
        procedure,
        steps,
    })
}

fn cross_check(recipe: &Recipe, ir: &ProcedureIr) -> Result<Vec<GoldStep>> {
    let id = &recipe.id;
    if ir.schema_version != SCHEMA_VERSION {
        bail!("gold {id}: procedure.json must be IR schema {SCHEMA_VERSION}");
    }
    if ir.skill_name != *id {
        bail!(
            "gold {id}: skill_name '{}' must equal the recipe id",
            ir.skill_name
        );
    }
    if (ir.source.duration_secs - recipe.duration_secs).abs() > 1e-6 {
        bail!("gold {id}: source.duration_secs must equal the recipe duration");
    }
    let spans = recipe.step_spans();
    let labelled: Vec<&str> = spans.iter().map(|s| s.id.as_str()).collect();
    let declared: Vec<&str> = ir.steps.iter().map(|s| s.id.as_str()).collect();
    if labelled != declared {
        bail!(
            "gold {id}: recipe labels steps {labelled:?} but procedure.json declares {declared:?}"
        );
    }
    ir.steps
        .iter()
        .zip(spans)
        .map(|(step, labelled)| {
            if step.evidence.is_empty() {
                bail!("gold {id}: step '{}' needs evidence", step.id);
            }
            if !matches!(step.confidence.as_str(), "high" | "medium" | "low") {
                bail!("gold {id}: step '{}' has an unknown confidence", step.id);
            }
            for evidence in &step.evidence {
                let t = evidence.timestamp_secs;
                if t < labelled.span.start_secs || t > labelled.span.end_secs {
                    bail!(
                        "gold {id}: step '{}' cites {t}s outside its span {}–{}s",
                        step.id,
                        labelled.span.start_secs,
                        labelled.span.end_secs
                    );
                }
            }
            Ok(GoldStep {
                id: step.id.clone(),
                span: labelled.span,
                commands: backtick_commands(&step.actions),
            })
        })
        .collect()
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let raw =
        fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    serde_json::from_str(&raw).with_context(|| format!("{} is not valid", path.display()))
}
