//! Pure evaluation of one check against one snapshot.
//!
//! No platform API is touched here, so every rule is unit-tested against
//! a hand-built tree. A verdict carries what was observed (so a miss can
//! be diagnosed from the report alone) and what a pass does *not* prove,
//! which the verifier copies into the step's detail per the protocol's
//! substrate rules.

use std::path::Path;

use super::check::{Check, Locator};
use super::tree::{Node, Snapshot};

#[cfg(test)]
mod tests;

/// The outcome of one check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    pub passed: bool,
    /// What the substrate actually reported, in one line.
    pub observed: String,
    /// The part of the claim this substrate cannot establish.
    pub not_proved: &'static str,
}

/// How many same-role candidates a miss lists before eliding the rest.
const CANDIDATE_LIMIT: usize = 8;

/// Evaluate `check`. GUI checks need the application's `snapshot`;
/// `file_exists` ignores it.
#[must_use]
pub fn evaluate(check: &Check, snapshot: Option<&Snapshot>) -> Verdict {
    if let Check::FileExists { path } = check {
        return file_exists(Path::new(path));
    }
    let Some(snapshot) = snapshot else {
        return Verdict {
            passed: false,
            observed: "no accessibility snapshot was taken for this check".into(),
            not_proved: NOT_PROVED_NOTHING_OBSERVED,
        };
    };
    match check {
        Check::WindowTitleContains { text, .. } => window_title_contains(snapshot, text),
        Check::ElementExists { .. } => element_exists(snapshot, check),
        Check::ElementValueEquals { value, .. } => element_value_equals(snapshot, check, value),
        Check::CheckboxState { checked, .. } => checkbox_state(snapshot, check, *checked),
        Check::MenuItemEnabled { menu, enabled, .. } => menu_item_enabled(snapshot, menu, *enabled),
        Check::FileExists { .. } => unreachable!("handled above"),
    }
}

const NOT_PROVED_NOTHING_OBSERVED: &str = "anything — nothing was observed";
const NOT_PROVED_WINDOW: &str = "that the step's actions produced the window, or anything about \
     its contents, position or rendering";
const NOT_PROVED_ELEMENT: &str = "that the element is visible, hit-testable or enabled — only \
     that the accessibility tree reports it";
const NOT_PROVED_VALUE: &str = "how the value was entered or how it is rendered — only the \
     value the accessibility tree reports";
const NOT_PROVED_CHECKBOX: &str = "that the step changed the state — only the state reported at \
     observation time";
const NOT_PROVED_MENU: &str = "that activating the item does what the step claims — only the \
     enabled flag the application reports for its (closed) menu";
const NOT_PROVED_FILE: &str = "the file's contents";

fn file_exists(path: &Path) -> Verdict {
    let exists = path.exists();
    Verdict {
        passed: exists,
        observed: format!(
            "{} {}",
            path.display(),
            if exists { "exists" } else { "does not exist" }
        ),
        not_proved: NOT_PROVED_FILE,
    }
}

fn window_title_contains(snapshot: &Snapshot, text: &str) -> Verdict {
    let titles: Vec<&str> = snapshot
        .root
        .descendants()
        .into_iter()
        .filter(|node| node.has_role("window"))
        .filter_map(|node| node.title.as_deref())
        .collect();
    let passed = titles.iter().any(|title| title.contains(text));
    let observed = if titles.is_empty() {
        format!("{}: no window titles reported", snapshot.app)
    } else {
        format!("{}: window titles {titles:?}", snapshot.app)
    };
    Verdict {
        passed,
        observed: truncation_note(snapshot, observed),
        not_proved: NOT_PROVED_WINDOW,
    }
}

/// The first node matching `locator`, plus every node of the same role
/// (so a miss can say what *was* there).
fn locate<'a>(snapshot: &'a Snapshot, locator: Locator<'_>) -> (Option<&'a Node>, Vec<&'a Node>) {
    let same_role: Vec<&Node> = snapshot
        .root
        .descendants()
        .into_iter()
        .filter(|node| node.has_role(locator.role))
        .collect();
    let matched = same_role.iter().copied().find(|node| {
        locator
            .title
            .is_none_or(|title| node.title.as_deref() == Some(title))
            && locator
                .identifier
                .is_none_or(|id| node.identifier.as_deref() == Some(id))
    });
    (matched, same_role)
}

fn describe_miss(snapshot: &Snapshot, locator: Locator<'_>, same_role: &[&Node]) -> String {
    let wanted = match (locator.title, locator.identifier) {
        (Some(title), Some(id)) => format!("{} {title:?} id={id}", locator.role),
        (Some(title), None) => format!("{} {title:?}", locator.role),
        (None, Some(id)) => format!("{} id={id}", locator.role),
        (None, None) => locator.role.to_string(),
    };
    let listed: Vec<String> = same_role
        .iter()
        .take(CANDIDATE_LIMIT)
        .map(|node| node.describe())
        .collect();
    let more = same_role.len().saturating_sub(CANDIDATE_LIMIT);
    let seen = if listed.is_empty() {
        "no elements of that role".to_string()
    } else if more > 0 {
        format!(
            "{} elements of that role: {listed:?} and {more} more",
            same_role.len()
        )
    } else {
        format!("{} elements of that role: {listed:?}", same_role.len())
    };
    truncation_note(snapshot, format!("{}: no {wanted}; {seen}", snapshot.app))
}

fn element_exists(snapshot: &Snapshot, check: &Check) -> Verdict {
    let Some(locator) = check.locator() else {
        unreachable!("element checks always carry a locator");
    };
    let (matched, same_role) = locate(snapshot, locator);
    match matched {
        Some(node) => Verdict {
            passed: true,
            observed: format!("{}: found {}", snapshot.app, node.describe()),
            not_proved: NOT_PROVED_ELEMENT,
        },
        None => Verdict {
            passed: false,
            observed: describe_miss(snapshot, locator, &same_role),
            not_proved: NOT_PROVED_ELEMENT,
        },
    }
}

fn element_value_equals(
    snapshot: &Snapshot,
    check: &Check,
    expected: &super::tree::Scalar,
) -> Verdict {
    let Some(locator) = check.locator() else {
        unreachable!("element checks always carry a locator");
    };
    let (matched, same_role) = locate(snapshot, locator);
    let Some(node) = matched else {
        return Verdict {
            passed: false,
            observed: describe_miss(snapshot, locator, &same_role),
            not_proved: NOT_PROVED_VALUE,
        };
    };
    let passed = node.value.as_ref() == Some(expected);
    Verdict {
        passed,
        observed: format!(
            "{}: {} (expected value={expected})",
            snapshot.app,
            node.describe()
        ),
        not_proved: NOT_PROVED_VALUE,
    }
}

fn checkbox_state(snapshot: &Snapshot, check: &Check, expected: bool) -> Verdict {
    let Some(locator) = check.locator() else {
        unreachable!("checkbox checks always carry a locator");
    };
    let (matched, same_role) = locate(snapshot, locator);
    let Some(node) = matched else {
        return Verdict {
            passed: false,
            observed: describe_miss(snapshot, locator, &same_role),
            not_proved: NOT_PROVED_CHECKBOX,
        };
    };
    let state = node
        .value
        .as_ref()
        .and_then(super::tree::Scalar::as_checked);
    let reading = match state {
        Some(true) => "checked",
        Some(false) => "unchecked",
        None if node.value.is_some() => "mixed or non-boolean",
        None => "no value reported",
    };
    Verdict {
        passed: state == Some(expected),
        observed: format!("{}: {} is {reading}", snapshot.app, node.describe()),
        not_proved: NOT_PROVED_CHECKBOX,
    }
}

/// Menu bar items and menu items on every platform spell their role with
/// one of these suffixes (`AXMenuBarItem`, `AXMenuItem`, UIA `MenuItem`).
fn is_menu_entry(node: &Node) -> bool {
    let role = super::tree::normalize_role(&node.role);
    role.ends_with("menuitem") || role.ends_with("menubaritem")
}

fn menu_item_enabled(snapshot: &Snapshot, menu: &[String], expected: bool) -> Verdict {
    let mut current = &snapshot.root;
    let mut walked: Vec<&str> = Vec::new();
    for segment in menu {
        let next = current
            .descendants()
            .into_iter()
            .skip(1)
            .find(|node| is_menu_entry(node) && node.title.as_deref() == Some(segment));
        let Some(node) = next else {
            let available: Vec<&str> = current
                .descendants()
                .into_iter()
                .skip(1)
                .filter(|node| is_menu_entry(node))
                .filter_map(|node| node.title.as_deref())
                .take(CANDIDATE_LIMIT * 4)
                .collect();
            let at = if walked.is_empty() {
                "the menu bar".to_string()
            } else {
                walked.join(" > ")
            };
            return Verdict {
                passed: false,
                observed: truncation_note(
                    snapshot,
                    format!(
                        "{}: no entry {segment:?} under {at}; entries there: {available:?}",
                        snapshot.app
                    ),
                ),
                not_proved: NOT_PROVED_MENU,
            };
        };
        walked.push(segment);
        current = node;
    }
    let path = walked.join(" > ");
    match current.enabled {
        Some(enabled) => Verdict {
            passed: enabled == expected,
            observed: format!("{}: {path} enabled={enabled}", snapshot.app),
            not_proved: NOT_PROVED_MENU,
        },
        None => Verdict {
            passed: false,
            observed: format!(
                "{}: {path} found but its enabled state is not reported",
                snapshot.app
            ),
            not_proved: NOT_PROVED_MENU,
        },
    }
}

fn truncation_note(snapshot: &Snapshot, observed: String) -> String {
    if snapshot.truncated {
        format!("{observed} (tree truncated at the node budget; unread elements may exist)")
    } else {
        observed
    }
}
