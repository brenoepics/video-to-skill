//! The postcondition language: read-only checks a verifier may request.
//!
//! Each check names an observable fact about a running application's
//! accessibility tree, or about the filesystem, and nothing else: there is
//! no action, click or keystroke in this vocabulary, by design. The JSON
//! form is an object tagged by `"check"`:
//!
//! ```json
//! {"check": "window_title_contains", "app": "TextEdit", "text": "Untitled"}
//! {"check": "element_exists", "role": "AXButton", "title": "Save"}
//! {"check": "element_value_equals", "role": "AXTextArea", "identifier": "body", "value": "hello"}
//! {"check": "checkbox_state", "title": "Wrap lines", "checked": true}
//! {"check": "file_exists", "path": "/tmp/work/out.txt"}
//! {"check": "menu_item_enabled", "app": "TextEdit", "menu": ["File", "Save"]}
//! ```
//!
//! `app` names a running process (its executable name, e.g. `TextEdit`);
//! when omitted the frontmost application is observed. Roles are the
//! platform's own names, compared without the `AX` prefix and ignoring
//! case, so `button` matches `AXButton`.

use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use super::tree::Scalar;

#[cfg(test)]
mod tests;

/// A read-only postcondition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "check", rename_all = "snake_case", deny_unknown_fields)]
pub enum Check {
    /// Some window of the application has a title containing `text`.
    WindowTitleContains {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        app: Option<String>,
        text: String,
    },
    /// An element with `role` and the given `title` or `identifier` exists.
    ElementExists {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        app: Option<String>,
        role: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        identifier: Option<String>,
    },
    /// The located element's accessibility value equals `value`
    /// (same JSON type: a text `"1"` is not the number `1`).
    ElementValueEquals {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        app: Option<String>,
        role: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        identifier: Option<String>,
        value: Scalar,
    },
    /// The located checkbox is checked (or unchecked); a mixed state
    /// satisfies neither.
    CheckboxState {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        app: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        identifier: Option<String>,
        checked: bool,
    },
    /// A file exists at `path`. Already scriptable; listed so one report
    /// can carry every postcondition in one vocabulary.
    FileExists { path: String },
    /// The menu item reached by `menu` (e.g. `["File", "Save"]`) is
    /// enabled — or disabled, with `"enabled": false`.
    MenuItemEnabled {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        app: Option<String>,
        menu: Vec<String>,
        #[serde(default = "enabled_default")]
        enabled: bool,
    },
}

fn enabled_default() -> bool {
    true
}

/// Where a check looks: a path on disk, or the accessibility tree of a
/// named application (`None` = the frontmost one).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target<'a> {
    File(&'a Path),
    Gui(Option<&'a str>),
}

/// How an element is found: its role plus a title and/or an identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Locator<'a> {
    pub role: &'a str,
    pub title: Option<&'a str>,
    pub identifier: Option<&'a str>,
}

/// The role every `checkbox_state` check locates.
pub const CHECKBOX_ROLE: &str = "checkbox";

impl Check {
    /// Parse one JSON check object and validate it.
    pub fn parse(json: &str) -> Result<Self> {
        let check: Self = serde_json::from_str(json).context("not a valid check")?;
        check.validate()?;
        Ok(check)
    }

    /// Parse a JSON check object, or a non-empty array of them.
    pub fn parse_many(json: &str) -> Result<Vec<Self>> {
        let value: serde_json::Value = serde_json::from_str(json).context("not valid JSON")?;
        let items = match value {
            serde_json::Value::Array(items) if items.is_empty() => {
                bail!("an empty list checks nothing")
            }
            serde_json::Value::Array(items) => items,
            object @ serde_json::Value::Object(_) => vec![object],
            other => bail!("expected a check object or an array of them, got {other}"),
        };
        items
            .into_iter()
            .map(|item| {
                let check: Self = serde_json::from_value(item).context("not a valid check")?;
                check.validate()?;
                Ok(check)
            })
            .collect()
    }

    /// The `"check"` tag this variant serialises as.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::WindowTitleContains { .. } => "window_title_contains",
            Self::ElementExists { .. } => "element_exists",
            Self::ElementValueEquals { .. } => "element_value_equals",
            Self::CheckboxState { .. } => "checkbox_state",
            Self::FileExists { .. } => "file_exists",
            Self::MenuItemEnabled { .. } => "menu_item_enabled",
        }
    }

    #[must_use]
    pub fn target(&self) -> Target<'_> {
        match self {
            Self::FileExists { path } => Target::File(Path::new(path)),
            Self::WindowTitleContains { app, .. }
            | Self::ElementExists { app, .. }
            | Self::ElementValueEquals { app, .. }
            | Self::CheckboxState { app, .. }
            | Self::MenuItemEnabled { app, .. } => Target::Gui(app.as_deref()),
        }
    }

    /// The element locator, for the element-level checks.
    #[must_use]
    pub fn locator(&self) -> Option<Locator<'_>> {
        match self {
            Self::ElementExists {
                role,
                title,
                identifier,
                ..
            }
            | Self::ElementValueEquals {
                role,
                title,
                identifier,
                ..
            } => Some(Locator {
                role,
                title: title.as_deref(),
                identifier: identifier.as_deref(),
            }),
            Self::CheckboxState {
                title, identifier, ..
            } => Some(Locator {
                role: CHECKBOX_ROLE,
                title: title.as_deref(),
                identifier: identifier.as_deref(),
            }),
            _ => None,
        }
    }

    fn validate(&self) -> Result<()> {
        let kind = self.kind();
        if let Some(locator) = self.locator() {
            if locator.role.trim().is_empty() {
                bail!("{kind}: role must not be empty");
            }
            if locator.title.is_none_or(|t| t.trim().is_empty())
                && locator.identifier.is_none_or(|i| i.trim().is_empty())
            {
                bail!("{kind}: an element needs a title or an identifier to locate it");
            }
        }
        match self {
            Self::WindowTitleContains { text, .. } if text.trim().is_empty() => {
                bail!("{kind}: text must not be empty")
            }
            Self::FileExists { path } if path.trim().is_empty() => {
                bail!("{kind}: path must not be empty")
            }
            Self::MenuItemEnabled { menu, .. } if menu.is_empty() => {
                bail!("{kind}: menu path must name at least one entry")
            }
            Self::MenuItemEnabled { menu, .. } if menu.iter().any(|m| m.trim().is_empty()) => {
                bail!("{kind}: menu path entries must not be empty")
            }
            _ => Ok(()),
        }
    }
}
