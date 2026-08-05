//! Package rendering: IR → files. Deterministic; no timestamps or
//! randomness may enter the output.

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use super::{ProcedureIr, Step};

pub fn emit(ir: &ProcedureIr, bundle_dir: &Path, out_dir: &Path) -> Result<()> {
    fs::create_dir_all(out_dir.join("steps"))?;
    write(out_dir, "SKILL.md", &render_skill_md(ir))?;
    for step in &ir.steps {
        write(
            out_dir,
            &format!("steps/{}.md", step.id),
            &render_step(step),
        )?;
    }
    emit_scripts(ir, out_dir)?;
    copy_frames(ir, bundle_dir, out_dir)?;
    write(
        out_dir,
        "provenance.json",
        &serde_json::to_string_pretty(ir)?,
    )?;
    Ok(())
}

fn render_skill_md(ir: &ProcedureIr) -> String {
    let mut md = String::new();
    let _ = writeln!(md, "---");
    let _ = writeln!(md, "name: {}", ir.skill_name);
    let _ = writeln!(md, "description: {}", ir.description.replace('\n', " "));
    let _ = writeln!(md, "---");
    let _ = writeln!(md, "\n# {}\n", ir.title);
    let _ = writeln!(md, "{}\n", ir.overview.trim());
    let source_title = ir.source.title.as_deref().unwrap_or("(untitled)");
    let _ = writeln!(
        md,
        "Learned from: \"{source_title}\" ({}, {:.0}s, genre: {}).\n",
        ir.source.original, ir.source.duration_secs, ir.genre
    );
    let _ = writeln!(md, "## Steps\n");
    let _ = writeln!(
        md,
        "Load a step file from `steps/` when performing it; each cites its video evidence.\n"
    );
    for step in &ir.steps {
        let flag = if step.confidence == "low" {
            " — ⚠ LOW CONFIDENCE"
        } else {
            ""
        };
        let _ = writeln!(
            md,
            "- [{}](steps/{}.md): {}{flag}",
            step.id, step.id, step.goal
        );
    }
    if !ir.artifacts.is_empty() {
        let _ = writeln!(md, "\n## On-screen artifacts\n");
        for artifact in &ir.artifacts {
            let _ = writeln!(
                md,
                "- `{}` [t={}]",
                artifact.text,
                mmss(artifact.timestamp_secs)
            );
        }
    }
    if !ir.history.is_empty() {
        let _ = writeln!(md, "\n## Sources\n");
        let _ = writeln!(
            md,
            "- \"{}\" ({}) — original",
            ir.source.title.as_deref().unwrap_or("(untitled)"),
            ir.source.original
        );
        for record in &ir.history {
            let _ = writeln!(
                md,
                "- \"{}\" ({}) — folded {} ({} confirmed, {} added, {} variants)",
                record.source.title.as_deref().unwrap_or("(untitled)"),
                record.source.original,
                record.date,
                record.confirmed.len(),
                record.added.len(),
                record.variants.len()
            );
        }
    }
    if !ir.gaps.is_empty() {
        let _ = writeln!(md, "\n## Known gaps\n");
        for gap in &ir.gaps {
            let _ = writeln!(md, "- {gap}");
        }
    }
    md
}

fn render_step(step: &Step) -> String {
    let mut md = String::new();
    let _ = writeln!(md, "# {} — {}\n", step.id, step.goal);
    if step.confidence == "low" {
        let _ = writeln!(
            md,
            "> ⚠ **LOW CONFIDENCE** — verify before relying on this step.\n"
        );
    }
    let _ = writeln!(md, "## Actions\n\n{}\n", step.actions.trim());
    let _ = writeln!(
        md,
        "## Success criteria\n\n{}\n",
        step.success_criteria.trim()
    );
    if let Some(caveats) = &step.caveats {
        let _ = writeln!(md, "## Caveats\n\n{}\n", caveats.trim());
    }
    let _ = writeln!(md, "## Evidence\n");
    for evidence in &step.evidence {
        let mut line = format!("- [t={}]", mmss(evidence.timestamp_secs));
        if let Some(frame) = &evidence.frame {
            let name = frame.rsplit('/').next().unwrap_or(frame);
            let _ = write!(line, " ![frame](../references/frames/{name})");
        }
        if let Some(quote) = &evidence.quote {
            let _ = write!(line, " — \"{quote}\"");
        }
        let _ = writeln!(md, "{line}");
    }
    if !step.variants.is_empty() {
        let _ = writeln!(md, "\n## Variants (from other videos)\n");
        for variant in &step.variants {
            let _ = writeln!(
                md,
                "- {} — source: {}",
                variant.actions.trim(),
                variant.source_original
            );
            for evidence in &variant.evidence {
                let quote = evidence
                    .quote
                    .as_deref()
                    .map(|q| format!(" — \"{q}\""))
                    .unwrap_or_default();
                let _ = writeln!(md, "  - [t={}]{quote}", mmss(evidence.timestamp_secs));
            }
            if let Some(note) = &variant.note {
                let _ = writeln!(md, "  - note: {note}");
            }
        }
    }
    if !step.scripts.is_empty() {
        let _ = writeln!(md, "\n## Scripts\n");
        for script in &step.scripts {
            let _ = writeln!(md, "- `scripts/{}`", script.name);
        }
    }
    md
}

fn emit_scripts(ir: &ProcedureIr, out_dir: &Path) -> Result<()> {
    let scripts: Vec<_> = ir.steps.iter().flat_map(|s| &s.scripts).collect();
    if scripts.is_empty() {
        return Ok(());
    }
    fs::create_dir_all(out_dir.join("scripts"))?;
    for script in scripts {
        let path = out_dir.join("scripts").join(&script.name);
        fs::write(&path, &script.contents)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
        }
    }
    Ok(())
}

fn copy_frames(ir: &ProcedureIr, bundle_dir: &Path, out_dir: &Path) -> Result<()> {
    let frames: Vec<&String> = ir
        .steps
        .iter()
        .flat_map(|s| &s.evidence)
        .filter_map(|e| e.frame.as_ref())
        .chain(ir.artifacts.iter().filter_map(|a| a.frame.as_ref()))
        .collect();
    if frames.is_empty() {
        return Ok(());
    }
    let dest_dir = out_dir.join("references/frames");
    fs::create_dir_all(&dest_dir)?;
    for frame in frames {
        let name = frame.rsplit('/').next().unwrap_or(frame);
        fs::copy(bundle_dir.join(frame), dest_dir.join(name))
            .with_context(|| format!("copying evidence frame {frame}"))?;
    }
    Ok(())
}

fn write(out_dir: &Path, rel: &str, contents: &str) -> Result<()> {
    fs::write(out_dir.join(rel), contents).with_context(|| format!("writing {rel}"))
}

/// Seconds → "M:SS" (or "H:MM:SS" past an hour).
fn mmss(secs: f64) -> String {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let total = secs.max(0.0).round() as u64;
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}
