//! A backend-neutral snapshot of an application's accessibility tree.
//!
//! Every backend reduces its platform tree to this shape, so the
//! evaluator — and its tests — never see a platform API.

use std::fmt::{self, Write as _};

use serde::{Deserialize, Serialize};

/// Checkbox values arrive as `CFNumber`s (0, 1, 2 for mixed); anything
/// closer than this to an integer is that integer.
const INTEGRAL_TOLERANCE: f64 = 1e-9;

/// An element's accessibility value, with its type preserved.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Scalar {
    Bool(bool),
    Number(f64),
    Text(String),
}

impl Scalar {
    /// A checkbox reading: `false`/`0` unchecked, `true`/`1` checked;
    /// anything else (a mixed `2`, text) is `None`.
    #[must_use]
    pub fn as_checked(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            Self::Number(n) if n.abs() < INTEGRAL_TOLERANCE => Some(false),
            Self::Number(n) if (n - 1.0).abs() < INTEGRAL_TOLERANCE => Some(true),
            _ => None,
        }
    }
}

impl fmt::Display for Scalar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bool(b) => write!(f, "{b}"),
            Self::Number(n) => write!(f, "{n}"),
            Self::Text(t) => write!(f, "{t:?}"),
        }
    }
}

/// One element of the tree. Only the attributes the checks read are kept.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Node {
    /// The platform's role name (`AXButton`, `Button`, …).
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identifier: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Scalar>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<Node>,
}

impl Node {
    #[must_use]
    pub fn new(role: &str) -> Self {
        Self {
            role: role.to_string(),
            title: None,
            identifier: None,
            value: None,
            enabled: None,
            children: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_title(mut self, title: &str) -> Self {
        self.title = Some(title.to_string());
        self
    }

    #[must_use]
    pub fn with_identifier(mut self, identifier: &str) -> Self {
        self.identifier = Some(identifier.to_string());
        self
    }

    #[must_use]
    pub fn with_value(mut self, value: Scalar) -> Self {
        self.value = Some(value);
        self
    }

    #[must_use]
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = Some(enabled);
        self
    }

    #[must_use]
    pub fn with_child(mut self, child: Node) -> Self {
        self.children.push(child);
        self
    }

    /// This node and every node below it, in document order.
    #[must_use]
    pub fn descendants(&self) -> Vec<&Node> {
        let mut out = Vec::new();
        self.collect(&mut out);
        out
    }

    fn collect<'a>(&'a self, out: &mut Vec<&'a Node>) {
        out.push(self);
        for child in &self.children {
            child.collect(out);
        }
    }

    /// Role equality without the platform prefix and ignoring case.
    #[must_use]
    pub fn has_role(&self, role: &str) -> bool {
        normalize_role(&self.role) == normalize_role(role)
    }

    /// A compact one-line rendering for observation reports.
    #[must_use]
    pub fn describe(&self) -> String {
        // Writing into a String cannot fail, so the results are dropped.
        let mut out = self.role.clone();
        if let Some(title) = &self.title {
            let _ = write!(out, " {title:?}");
        }
        if let Some(identifier) = &self.identifier {
            let _ = write!(out, " id={identifier}");
        }
        if let Some(value) = &self.value {
            let _ = write!(out, " value={value}");
        }
        if let Some(enabled) = self.enabled {
            let _ = write!(out, " enabled={enabled}");
        }
        out
    }
}

/// Lower-case, without the macOS `AX` prefix and without separators, so
/// `AXCheckBox`, `CheckBox` and `check_box` all become `checkbox`.
#[must_use]
pub fn normalize_role(role: &str) -> String {
    let trimmed = role.trim();
    let stripped = trimmed.strip_prefix("AX").unwrap_or(trimmed);
    stripped
        .chars()
        .filter(|c| !matches!(c, '_' | '-' | ' '))
        .flat_map(char::to_lowercase)
        .collect()
}

/// The tree read from one application.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    /// The application as the backend identifies it.
    pub app: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<i32>,
    /// True when the backend stopped reading at its node budget, so an
    /// absent element may simply be unread.
    #[serde(default)]
    pub truncated: bool,
    pub root: Node,
}

#[cfg(test)]
mod tests {
    use super::{normalize_role, Node, Scalar};

    #[test]
    fn roles_normalise_across_platform_spellings() {
        for spelling in [
            "AXCheckBox",
            "CheckBox",
            "check_box",
            "check box",
            " checkbox ",
        ] {
            assert_eq!(normalize_role(spelling), "checkbox", "{spelling}");
        }
        assert_eq!(normalize_role("AXMenuBarItem"), "menubaritem");
        assert!(Node::new("AXButton").has_role("button"));
        assert!(!Node::new("AXButton").has_role("menu_button"));
    }

    #[test]
    fn checkbox_readings_cover_bool_and_number_values() {
        assert_eq!(Scalar::Bool(true).as_checked(), Some(true));
        assert_eq!(Scalar::Number(0.0).as_checked(), Some(false));
        assert_eq!(Scalar::Number(1.0).as_checked(), Some(true));
        assert_eq!(Scalar::Number(2.0).as_checked(), None);
        assert_eq!(Scalar::Text("1".into()).as_checked(), None);
    }

    #[test]
    fn descendants_are_in_document_order_including_self() {
        let tree = Node::new("a")
            .with_child(Node::new("b").with_child(Node::new("c")))
            .with_child(Node::new("d"));
        let roles: Vec<&str> = tree.descendants().iter().map(|n| n.role.as_str()).collect();
        assert_eq!(roles, ["a", "b", "c", "d"]);
    }

    #[test]
    fn describe_is_one_compact_line() {
        let node = Node::new("AXCheckBox")
            .with_title("Wrap lines")
            .with_identifier("wrap")
            .with_value(Scalar::Number(1.0))
            .with_enabled(true);
        assert_eq!(
            node.describe(),
            "AXCheckBox \"Wrap lines\" id=wrap value=1 enabled=true"
        );
    }
}
