//! IR validation: everything the compiler refuses before writing a byte.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use anyhow::{bail, Result};

use super::{repairs, safety, ProcedureIr, Step, SCHEMA_VERSION};

/// Agent Skills spec limits on frontmatter fields.
const MAX_NAME_LENGTH: usize = 64;
const MAX_DESCRIPTION_LENGTH: usize = 1024;
const MAX_COMPATIBILITY_LENGTH: usize = 500;

pub(super) fn validate(ir: &ProcedureIr, bundle_dir: &Path, out_dir: &Path) -> Result<()> {
    if ir.schema_version == 0 || ir.schema_version > SCHEMA_VERSION {
        bail!(
            "unsupported IR schema_version {} (this build reads v1 and v{SCHEMA_VERSION})",
            ir.schema_version
        );
    }
    validate_frontmatter_fields(ir)?;
    validate_package_directory(ir, out_dir)?;
    if ir.steps.is_empty() {
        bail!("a skill needs at least one step — refuse non-procedural videos instead");
    }
    let mut seen = HashSet::new();
    for step in &ir.steps {
        crate::safe_path::validate_step_id(&step.id)?;
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
        validate_parameters(step)?;
        if let Err(line) = repairs::cited_signals(step.caveats.as_deref().unwrap_or("")) {
            bail!(
                "step '{}': a repair in caveats cites no external signal ('{line}') — write \
                 it as 'Repair N: <change>. Signal: <exit code N | stdout: … | stderr: … | \
                 output diff: … | file: … | state: …>', naming what the environment \
                 reported, never your own judgement (see references/verification-protocol.md)",
                step.id
            );
        }
        for script in &step.scripts {
            crate::safe_path::validate_script_name(&script.name)?;
            if script.name.chars().any(char::is_whitespace) {
                bail!(
                    "step '{}': script name '{}' must not contain whitespace — it is listed \
                     in the frontmatter's space-separated allowed-tools",
                    step.id,
                    script.name
                );
            }
        }
        for evidence in &step.evidence {
            if let Some(frame) = &evidence.frame {
                crate::safe_path::resolve_bundle_file(bundle_dir, frame).map_err(|error| {
                    anyhow::anyhow!("step '{}' cites invalid frame '{frame}': {error}", step.id)
                })?;
            }
        }
    }
    for artifact in &ir.artifacts {
        if let Some(frame) = &artifact.frame {
            crate::safe_path::resolve_bundle_file(bundle_dir, frame)
                .map_err(|error| anyhow::anyhow!("artifact frame '{frame}' is invalid: {error}"))?;
        }
    }
    safety::check(ir)?;
    reject_script_name_collisions(ir)?;
    reject_frame_basename_collisions(ir)
}

/// The fields that become SKILL.md frontmatter, held to the Agent Skills
/// spec's limits so the reference validator accepts every package.
fn validate_frontmatter_fields(ir: &ProcedureIr) -> Result<()> {
    crate::safe_path::validate_skill_name(&ir.skill_name)?;
    if ir.skill_name.len() > MAX_NAME_LENGTH {
        bail!(
            "skill name '{}' exceeds the Agent Skills limit of {MAX_NAME_LENGTH} characters",
            ir.skill_name
        );
    }
    if ir.skill_name.contains("--") {
        bail!(
            "skill name '{}' must not contain consecutive hyphens (Agent Skills spec)",
            ir.skill_name
        );
    }
    if ir.description.trim().is_empty() || ir.description.len() > MAX_DESCRIPTION_LENGTH {
        bail!("description must be non-empty and at most {MAX_DESCRIPTION_LENGTH} characters");
    }
    if ir.license.as_deref().is_some_and(|l| l.trim().is_empty()) {
        bail!("license, when given, must not be empty — omit it to inherit the source's terms");
    }
    if let Some(compatibility) = &ir.compatibility {
        if compatibility.trim().is_empty() || compatibility.len() > MAX_COMPATIBILITY_LENGTH {
            bail!(
                "compatibility must be non-empty and at most {MAX_COMPATIBILITY_LENGTH} \
                 characters (Agent Skills spec)"
            );
        }
    }
    Ok(())
}

/// Parameters are what make a step reusable: a declared name must be
/// kebab-case and referenced as `{name}` from the actions, a script or
/// the postcondition, and a hyphenated `{place-holder}` in the actions
/// must be declared. Braces in code — `${HOME}`, `{name}` in an f-string,
/// JSON — are left alone: only hyphenated names count as placeholders.
fn validate_parameters(step: &Step) -> Result<()> {
    let texts: Vec<&str> = std::iter::once(step.actions.as_str())
        .chain(step.postcondition.as_deref())
        .chain(step.scripts.iter().map(|s| s.contents.as_str()))
        .collect();
    for parameter in &step.parameters {
        if !is_kebab(&parameter.name) {
            bail!(
                "step '{}': parameter name '{}' must be kebab-case (lowercase letters, \
                 digits, hyphens)",
                step.id,
                parameter.name
            );
        }
        let placeholder = format!("{{{}}}", parameter.name);
        if !texts.iter().any(|text| text.contains(&placeholder)) {
            bail!(
                "step '{}': parameter '{}' is never referenced — write it as '{placeholder}' \
                 in the actions, a script or the postcondition, or drop it",
                step.id,
                parameter.name
            );
        }
    }
    for name in placeholders(&step.actions) {
        if !step.parameters.iter().any(|p| p.name == name) {
            bail!(
                "step '{}': '{{{name}}}' in actions is not declared in parameters — declare \
                 it with a description and an example, or write the literal value",
                step.id
            );
        }
    }
    Ok(())
}

/// Hyphenated `{kebab-case}` placeholders in `text`, skipping shell
/// expansions (`${…}`).
fn placeholders(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    for (open, _) in text.match_indices('{') {
        if text[..open].ends_with('$') {
            continue;
        }
        let Some(close) = text[open..].find('}') else {
            break;
        };
        let name = &text[open + 1..open + close];
        if name.contains('-') && is_kebab(name) {
            found.push(name.to_string());
        }
    }
    found
}

fn is_kebab(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !value.starts_with('-')
        && !value.ends_with('-')
}

/// The spec requires `name` to equal the directory name; agents resolve a
/// skill by its directory, so a mismatch is a package that cannot be found.
fn validate_package_directory(ir: &ProcedureIr, out_dir: &Path) -> Result<()> {
    let dir_name = out_dir.file_name().and_then(|n| n.to_str());
    if dir_name != Some(ir.skill_name.as_str()) {
        bail!(
            "skill name '{}' must match the package directory name '{}' (Agent Skills \
             spec: name equals the directory) — write the package to a directory named \
             '{}'",
            ir.skill_name,
            dir_name.unwrap_or(""),
            ir.skill_name
        );
    }
    Ok(())
}

/// Scripts from every step share one output directory. Reject names that
/// would address the same file on a case-insensitive filesystem, as well as
/// duplicate names whose contents disagree.
fn reject_script_name_collisions(ir: &ProcedureIr) -> Result<()> {
    let mut by_name: HashMap<String, (&str, &str)> = HashMap::new();
    for script in ir.steps.iter().flat_map(|step| &step.scripts) {
        let key = script.name.to_lowercase();
        if let Some(&(first_name, first_contents)) = by_name.get(&key) {
            if first_name == script.name && first_contents == script.contents {
                continue;
            }
            if first_name == script.name {
                bail!(
                    "script name collision: '{}' is declared with different contents",
                    script.name
                );
            }
            bail!(
                "script name collision: '{first_name}' and '{}' are the same on a \
                 case-insensitive filesystem",
                script.name
            );
        }
        by_name.insert(key, (&script.name, &script.contents));
    }
    Ok(())
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
    let mut by_basename: HashMap<String, &str> = HashMap::new();
    for frame in referenced {
        let name = crate::safe_path::portable_basename(frame)?;
        let key = name.to_lowercase();
        match by_basename.get(&key) {
            // The same source frame cited twice is one copy — fine.
            Some(&first) if first != frame => bail!(
                "case-insensitive frame basename collision: '{first}' and '{frame}' both map \
                 to '{name}' in references/frames/, so one would overwrite the other"
            ),
            _ => {
                by_basename.insert(key, frame);
            }
        }
    }
    Ok(())
}
