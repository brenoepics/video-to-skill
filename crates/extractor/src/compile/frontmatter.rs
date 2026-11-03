//! The SKILL.md frontmatter: exactly the six fields of the Agent Skills
//! specification (<https://agentskills.io/specification>) and nothing
//! else — claude.ai and the API reject packages carrying unknown fields,
//! so everything of ours lives under the namespaced `metadata` map.
//!
//! Every value is a YAML double-quoted scalar except `name`, which is
//! kebab-case and read back bare by the installer.

use std::fmt::Write as _;

use anyhow::{bail, Result};

use super::ProcedureIr;

/// `metadata` key: the `vts-extract` version that compiled the package.
pub const METADATA_VERSION: &str = "video-to-skill/version";
/// `metadata` key: sha256 of the source media, when the bundle knew it.
pub const METADATA_SOURCE_SHA256: &str = "video-to-skill/source-sha256";
/// `metadata` key: `passed/total` steps per the last verification run;
/// `0/total` until `vts-extract verify` records one.
pub const METADATA_VERIFIED_STEPS: &str = "video-to-skill/verified-steps";

/// What `license` says when the IR states none: a reference to the
/// bundled provenance, never an invented licence.
const DEFAULT_LICENSE: &str = "Unspecified; the source video's terms apply (see provenance.json)";

/// Render the frontmatter block, opening and closing `---` included.
#[must_use]
pub fn render(ir: &ProcedureIr) -> String {
    let mut out = String::from("---\n");
    let _ = writeln!(out, "name: {}", ir.skill_name);
    let _ = writeln!(out, "description: {}", quote(&ir.description));
    let _ = writeln!(
        out,
        "license: {}",
        quote(ir.license.as_deref().unwrap_or(DEFAULT_LICENSE))
    );
    if let Some(compatibility) = compatibility(ir) {
        let _ = writeln!(out, "compatibility: {}", quote(&compatibility));
    }
    let _ = writeln!(out, "metadata:");
    let _ = writeln!(
        out,
        "  {METADATA_VERSION}: {}",
        quote(env!("CARGO_PKG_VERSION"))
    );
    if let Some(sha256) = &ir.source.sha256 {
        let _ = writeln!(out, "  {METADATA_SOURCE_SHA256}: {}", quote(sha256));
    }
    let _ = writeln!(
        out,
        "  {METADATA_VERIFIED_STEPS}: {}",
        quote(&format!("0/{}", ir.steps.len()))
    );
    if let Some(tools) = allowed_tools(ir) {
        let _ = writeln!(out, "allowed-tools: {tools}");
    }
    out.push_str("---\n");
    out
}

/// Replace the value of `key` inside the `metadata` map of an existing
/// SKILL.md, leaving everything else byte-identical.
pub fn set_metadata(skill_md: &str, key: &str, value: &str) -> Result<String> {
    let prefix = format!("  {key}:");
    let mut lines: Vec<&str> = skill_md.lines().collect();
    let close = lines
        .iter()
        .skip(1)
        .position(|line| *line == "---")
        .map(|i| i + 1);
    let Some(close) = close.filter(|_| lines.first() == Some(&"---")) else {
        bail!("SKILL.md has no frontmatter block to update");
    };
    let replacement = format!("{prefix} {}", quote(value));
    let Some(index) = lines[..close]
        .iter()
        .position(|line| line.starts_with(&prefix))
    else {
        bail!("SKILL.md frontmatter has no metadata entry '{key}'");
    };
    lines[index] = &replacement;
    let mut out = lines.join("\n");
    if skill_md.ends_with('\n') {
        out.push('\n');
    }
    Ok(out)
}

/// The authored requirements, or the script interpreters as a fallback:
/// a package that ships scripts needs their interpreters wherever it runs.
fn compatibility(ir: &ProcedureIr) -> Option<String> {
    if let Some(authored) = &ir.compatibility {
        return Some(authored.trim().to_string());
    }
    let mut interpreters: Vec<&str> = ir
        .steps
        .iter()
        .flat_map(|s| &s.scripts)
        .filter_map(|script| interpreter(&script.contents))
        .collect();
    interpreters.sort_unstable();
    interpreters.dedup();
    let list = match interpreters.as_slice() {
        [] => return None,
        [one] => (*one).to_string(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    };
    Some(format!("Requires {list} to run scripts/."))
}

/// The program named by a shebang line: `#!/bin/sh` → `sh`,
/// `#!/usr/bin/env python3` → `python3`.
fn interpreter(contents: &str) -> Option<&str> {
    let line = contents.lines().next()?.strip_prefix("#!")?;
    let mut words = line.split_whitespace();
    let program = words.next()?;
    let name = program.rsplit('/').next()?;
    if name == "env" {
        return words.find(|w| !w.starts_with('-'));
    }
    Some(name)
}

/// One `Bash(scripts/<name>:*)` entry per shipped script, in step order,
/// so an agent can pre-approve exactly what the package runs.
fn allowed_tools(ir: &ProcedureIr) -> Option<String> {
    let mut seen = Vec::new();
    for script in ir.steps.iter().flat_map(|s| &s.scripts) {
        if !seen.contains(&script.name.as_str()) {
            seen.push(&script.name);
        }
    }
    if seen.is_empty() {
        return None;
    }
    Some(
        seen.iter()
            .map(|name| format!("Bash(scripts/{name}:*)"))
            .collect::<Vec<_>>()
            .join(" "),
    )
}

/// A YAML double-quoted scalar on one line.
fn quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c.is_control() => out.push(' '),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting_escapes_what_yaml_needs_and_keeps_one_line() {
        assert_eq!(quote(r#"a "b" c\d"#), r#""a \"b\" c\\d""#);
        assert_eq!(quote("two\nlines\t"), r#""two lines ""#);
    }

    #[test]
    fn interpreters_come_from_shebangs_only() {
        assert_eq!(interpreter("#!/bin/sh\necho"), Some("sh"));
        assert_eq!(
            interpreter("#!/usr/bin/env -S python3 -u\n"),
            Some("python3")
        );
        assert_eq!(interpreter("echo no shebang"), None);
    }

    #[test]
    fn set_metadata_replaces_only_the_named_entry() {
        let md = "---\nname: x\nmetadata:\n  k: \"0/1\"\n  j: \"v\"\n---\n\n# x\n  k: body\n";
        let out = set_metadata(md, "k", "1/1").unwrap();
        assert_eq!(
            out,
            "---\nname: x\nmetadata:\n  k: \"1/1\"\n  j: \"v\"\n---\n\n# x\n  k: body\n"
        );
        assert!(set_metadata(md, "missing", "v").is_err());
        assert!(set_metadata("# no frontmatter\n", "k", "v").is_err());
    }
}
