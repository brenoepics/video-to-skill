//! A step's structured checks: the read-only vocabulary of
//! `vts-extract observe` ([`Check`]) carried in the Procedure IR.
//!
//! The compiler holds each check to the rules [`Check::parse`] applies,
//! and to one of its own: a check is literal text, except for
//! `{workspace}`, which may only begin a `file_exists` path. The verifier
//! replaces it with its throwaway workspace (`vts-extract observe
//! --workspace <dir>`, the directory it gave `sandbox`). A file a step
//! writes can only be checked there — the sandbox grants nothing else —
//! and any other path would observe the verifier's own machine instead of
//! the step's result.
//!
//! Every check also renders as one "Checkable:" line under the step's
//! success criteria: a sentence for a human, then the exact JSON the
//! verifier passes to `observe --check`.

use std::path::Path;

use anyhow::{anyhow, bail, Context, Result};

use super::{validate::placeholders, Step};
use crate::a11y::check::{Check, Locator, Target};
use crate::a11y::tree::Scalar;

#[cfg(test)]
mod tests;

/// The one placeholder a check may use, at the start of a `file_exists`
/// path.
pub const WORKSPACE_PLACEHOLDER: &str = "{workspace}";

/// Refuse a step whose checks `observe` would refuse, or that rely on a
/// substitution nothing performs.
pub(super) fn validate(step: &Step) -> Result<()> {
    for (index, check) in step.checks.iter().enumerate() {
        validate_one(check)
            .map_err(|error| anyhow!("step '{}' check {}: {error:#}", step.id, index + 1))?;
    }
    Ok(())
}

fn validate_one(check: &Check) -> Result<()> {
    check.validate()?;
    if let Check::FileExists { path } = check {
        return validate_workspace_path(path);
    }
    for (name, text) in strings(check) {
        let placeholder = if text.contains(WORKSPACE_PLACEHOLDER) {
            Some(WORKSPACE_PLACEHOLDER.to_string())
        } else {
            placeholders(text).first().map(|p| format!("{{{p}}}"))
        };
        if let Some(placeholder) = placeholder {
            bail!(
                "{name} uses '{placeholder}', but a check is literal text — only \
                 '{WORKSPACE_PLACEHOLDER}' is substituted, at the start of a file_exists \
                 path, and parameters never are; write the value the video shows"
            );
        }
    }
    Ok(())
}

fn validate_workspace_path(path: &str) -> Result<()> {
    let relative = match path.strip_prefix(WORKSPACE_PLACEHOLDER) {
        Some("" | "/") => bail!(
            "a file_exists path must name a file inside the workspace, as \
             '{WORKSPACE_PLACEHOLDER}/<relative path>'"
        ),
        Some(rest) => rest.strip_prefix('/'),
        None => None,
    };
    let Some(relative) = relative else {
        bail!(
            "file_exists path must start with '{WORKSPACE_PLACEHOLDER}/' — the verifier \
             can only check files the step wrote into its throwaway workspace, so a path \
             anywhere else would observe the verifier's machine, not the step's result"
        );
    };
    if relative.contains('\\') {
        bail!("a file_exists path must use '/' separators");
    }
    if relative.split('/').any(str::is_empty) {
        bail!("a file_exists path must not have an empty component");
    }
    if relative.split('/').any(|c| c == "." || c == "..") {
        bail!("a file_exists path must not use '.' or '..' — it stays inside the workspace");
    }
    if let Some(open) = relative.find('{') {
        let token = relative[open..]
            .find('}')
            .map_or(&relative[open..], |close| &relative[open..=open + close]);
        bail!(
            "'{WORKSPACE_PLACEHOLDER}' is the only placeholder a file_exists path may use, \
             once, at its start — '{token}' is never substituted"
        );
    }
    if !crate::safe_path::is_portable_relative(relative) {
        bail!(
            "a file_exists path must be portable: no ':', '*', '?', '\"', '<', '>', \
             '|', control characters, reserved Windows names, or names ending in '.' or ' '"
        );
    }
    Ok(())
}

/// Every string a check carries, named as the safety walk and the
/// validation errors refer to it.
pub(super) fn strings(check: &Check) -> Vec<(&'static str, &str)> {
    let mut out = Vec::new();
    if let Target::Gui(Some(app)) = check.target() {
        out.push(("check app", app));
    }
    if let Some(locator) = check.locator() {
        out.push(("check role", locator.role));
        out.extend(locator.title.map(|title| ("check title", title)));
        out.extend(locator.identifier.map(|id| ("check identifier", id)));
    }
    match check {
        Check::WindowTitleContains { text, .. } => out.push(("check text", text.as_str())),
        Check::ElementValueEquals {
            value: Scalar::Text(text),
            ..
        } => out.push(("check value", text.as_str())),
        Check::FileExists { path } => out.push(("check path", path.as_str())),
        Check::MenuItemEnabled { menu, .. } => {
            out.extend(menu.iter().map(|entry| ("check menu", entry.as_str())));
        }
        _ => {}
    }
    out
}

/// `check` with `{workspace}` in a `file_exists` path replaced by
/// `workspace`; every other check is returned unchanged.
pub fn substitute_workspace(check: &Check, workspace: &Path) -> Result<Check> {
    let Check::FileExists { path } = check else {
        return Ok(check.clone());
    };
    let Some(relative) = path.strip_prefix(WORKSPACE_PLACEHOLDER) else {
        return Ok(check.clone());
    };
    let joined = workspace.join(relative.trim_start_matches('/'));
    let path = joined
        .to_str()
        .with_context(|| format!("workspace {} is not valid UTF-8", workspace.display()))?
        .to_string();
    Ok(Check::FileExists { path })
}

/// Whether `check` still names the placeholder, i.e. needs a workspace
/// before it can be observed.
#[must_use]
pub fn needs_workspace(check: &Check) -> bool {
    matches!(check, Check::FileExists { path } if path.contains(WORKSPACE_PLACEHOLDER))
}

/// The "Checkable:" line for `check`: what holds, in words, then the
/// JSON `observe --check` takes.
pub(super) fn checkable_line(check: &Check) -> Result<String> {
    let json = serde_json::to_string(check)?;
    Ok(format!(
        "Checkable: {} — {}",
        sentence(check),
        code_span(&json)
    ))
}

fn sentence(check: &Check) -> String {
    let app = match check.target() {
        Target::Gui(Some(app)) => format!("the application {app:?}"),
        _ => "the frontmost application".to_string(),
    };
    // A checkbox_state locator's role is implied by the sentence.
    let with_role = !matches!(check, Check::CheckboxState { .. });
    let element = check
        .locator()
        .map_or_else(String::new, |locator| located(locator, with_role));
    match check {
        Check::WindowTitleContains { text, .. } => {
            format!("{app} has a window whose title contains {text:?}")
        }
        Check::ElementExists { .. } => format!("an element {element} exists in {app}"),
        Check::ElementValueEquals { value, .. } => {
            format!("the element {element} in {app} has the value {value}")
        }
        Check::CheckboxState { checked, .. } => {
            let state = if *checked { "checked" } else { "unchecked" };
            format!("the checkbox {element} in {app} is {state}")
        }
        Check::FileExists { path } => format!("a file exists at {path:?}"),
        Check::MenuItemEnabled { menu, enabled, .. } => {
            let item: Vec<String> = menu.iter().map(|entry| format!("{entry:?}")).collect();
            let state = if *enabled { "enabled" } else { "disabled" };
            format!("the menu item {} in {app} is {state}", item.join(" > "))
        }
    }
}

/// `with role "button" and title "Save"`.
fn located(locator: Locator<'_>, with_role: bool) -> String {
    let mut parts = Vec::new();
    if with_role {
        parts.push(format!("role {:?}", locator.role));
    }
    parts.extend(locator.title.map(|title| format!("title {title:?}")));
    parts.extend(locator.identifier.map(|id| format!("identifier {id:?}")));
    match parts.split_last() {
        Some((last, [])) => format!("with {last}"),
        Some((last, rest)) => format!("with {} and {last}", rest.join(", ")),
        None => String::new(),
    }
}

/// `text` as one Markdown code span, fenced with more backticks than any
/// run inside it so a backtick in a title cannot end the span early.
fn code_span(text: &str) -> String {
    let longest = text.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    let fence = "`".repeat(longest + 1);
    let pad = if text.starts_with('`') || text.ends_with('`') {
        " "
    } else {
        ""
    };
    format!("{fence}{pad}{text}{pad}{fence}")
}
