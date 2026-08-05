use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use vts_extract::compile::{self, ProcedureIr};
use vts_extract::merge::{merge, PlanAction};

/// Fold a new video's IR into an existing generated skill package.
/// Dry-run prints the plan; otherwise the merged package is compiled to
/// `out` (unverified — re-run verification after a fold).
pub fn run(skill: &str, ir_path: &str, bundle: &str, out: &str, dry_run: bool) -> Result<()> {
    let existing: ProcedureIr = serde_json::from_str(
        &fs::read_to_string(Path::new(skill).join("provenance.json"))
            .context("not a generated skill package (missing provenance.json)")?,
    )
    .context("package provenance.json is not valid")?;
    let mut incoming: ProcedureIr = serde_json::from_str(
        &fs::read_to_string(ir_path).with_context(|| format!("no Procedure IR at {ir_path}"))?,
    )
    .context("incoming Procedure IR is not valid")?;

    // Namespace the incoming frames by fold sequence so two videos'
    // `shot000.jpg` never collide inside one package.
    let prefix = format!("s{}-", existing.history.len() + 2);
    rewrite_frames(&mut incoming, &prefix);

    let result = merge(&existing, &incoming, &today());

    println!(
        "fold plan for {} <- {}:",
        existing.skill_name, incoming.source.original
    );
    for entry in &result.plan {
        let label = match entry.action {
            PlanAction::Confirm => "confirm",
            PlanAction::Variant => "variant",
            PlanAction::Add => "add    ",
            PlanAction::Unconfirmed => "keep   ",
        };
        println!(
            "  {label}  {} {}",
            entry.existing_id.as_deref().unwrap_or("-"),
            entry
                .incoming_id
                .as_deref()
                .map(|id| format!("<- {id}"))
                .unwrap_or_default()
        );
    }
    if dry_run {
        println!("dry run — nothing written.");
        return Ok(());
    }

    let staging = stage_frames(Path::new(skill), Path::new(bundle), &incoming, &prefix)?;
    compile::compile(&result.merged, &staging, Path::new(out))?;
    fs::remove_dir_all(&staging).ok();
    println!(
        "merged package written: {out}/ — re-verify before installing (the fold invalidates any previous badge)"
    );
    Ok(())
}

/// Prefix every frame reference in the incoming IR.
fn rewrite_frames(ir: &mut ProcedureIr, prefix: &str) {
    let rename = |frame: &mut Option<String>| {
        if let Some(f) = frame {
            let name = f.rsplit('/').next().unwrap_or(f).to_string();
            *f = format!("frames/{prefix}{name}");
        }
    };
    for step in &mut ir.steps {
        for evidence in &mut step.evidence {
            rename(&mut evidence.frame);
        }
    }
    for artifact in &mut ir.artifacts {
        rename(&mut artifact.frame);
    }
}

/// Build a staging dir where both videos' frames resolve under `frames/`.
fn stage_frames(
    skill: &Path,
    incoming_bundle: &Path,
    incoming: &ProcedureIr,
    prefix: &str,
) -> Result<std::path::PathBuf> {
    let staging = std::env::temp_dir().join(format!("vts-merge-{}", std::process::id()));
    let frames = staging.join("frames");
    fs::create_dir_all(&frames)?;
    // Existing package frames keep their names.
    let packaged = skill.join("references/frames");
    if packaged.is_dir() {
        for entry in fs::read_dir(&packaged)?.flatten() {
            fs::copy(entry.path(), frames.join(entry.file_name()))?;
        }
    }
    // Incoming frames were rewritten to frames/<prefix><name>; copy them
    // from the incoming bundle under those names.
    for frame in incoming
        .steps
        .iter()
        .flat_map(|s| &s.evidence)
        .filter_map(|e| e.frame.as_ref())
        .chain(incoming.artifacts.iter().filter_map(|a| a.frame.as_ref()))
    {
        let name = frame.rsplit('/').next().unwrap_or(frame);
        let original = name.strip_prefix(prefix).unwrap_or(name);
        let source = incoming_bundle.join("frames").join(original);
        fs::copy(&source, frames.join(name))
            .with_context(|| format!("staging incoming frame {}", source.display()))?;
    }
    Ok(staging)
}

fn today() -> String {
    std::process::Command::new("date")
        .arg("+%F")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown-date".to_string())
}
