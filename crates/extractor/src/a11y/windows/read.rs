//! One application's top-level windows, read into the neutral tree.

use ::windows::core::{Result as WinResult, BOOL, BSTR};
use ::windows::Win32::UI::Accessibility::{
    IUIAutomationElement, IUIAutomationElementArray, IUIAutomationRangeValuePattern,
    IUIAutomationTogglePattern, IUIAutomationValuePattern, UIA_RangeValuePatternId,
    UIA_TogglePatternId, UIA_ValuePatternId,
};
use anyhow::{bail, Context, Result};

use super::com::{failed, Apartment, Client};
use super::mapping::{control_type_name, element_value, exe_stem, sole_pid, PatternReadings};
use super::process::{name_of, pids_named};
use crate::a11y::tree::{Node, Snapshot};

/// Depth and node budgets keep a web view or a huge document from turning
/// one observation into minutes of IPC; the snapshot says when it hit them.
const MAX_DEPTH: usize = 32;
const MAX_NODES: usize = 5_000;

/// The role of the synthesized root, the counterpart of `AXApplication`.
const APPLICATION_ROLE: &str = "Application";

/// Read the windows of the application named `app` (its executable name,
/// `.exe` optional), or of the application owning the focused element
/// when `None`.
pub fn snapshot(app: Option<&str>) -> Result<Snapshot> {
    // Entered first and dropped last: every COM pointer lives in `read`.
    let _apartment = Apartment::enter()?;
    read(app)
}

fn read(app: Option<&str>) -> Result<Snapshot> {
    let client = Client::new()?;
    let (name, pids) = match app {
        Some(name) => (name.to_string(), running(name)?),
        None => focused(&client)?,
    };
    let windows = client
        .desktop()
        .and_then(|desktop| client.children(&desktop))
        .map_err(failed("listing top-level windows"))?;
    let mut root = Node::new(APPLICATION_ROLE).with_title(&name);
    let mut budget = MAX_NODES - 1;
    let mut owners = Vec::new();
    for window in elements(&windows) {
        if budget == 0 {
            break;
        }
        // SAFETY: a cached read on a live element.
        let Ok(pid) = (unsafe { window.CachedProcessId() }) else {
            continue;
        };
        if pids.contains(&pid) {
            owners.push(pid);
            root.children
                .push(read_node(&client, &window, 1, &mut budget));
        }
    }
    if root.children.is_empty() {
        bail!(
            "{name} (pid {}) has no top-level window UI Automation can see — it is windowless, \
             still starting, or runs elevated while this process does not",
            pids.iter()
                .map(i32::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    Ok(Snapshot {
        app: name,
        pid: sole_pid(&owners),
        truncated: budget == 0,
        root,
    })
}

fn running(name: &str) -> Result<Vec<i32>> {
    let pids = pids_named(name)?;
    if pids.is_empty() {
        bail!(
            "no running process named {name:?} — launch the application first, or omit `app` \
             to observe the application that has the focus"
        );
    }
    Ok(pids)
}

fn focused(client: &Client) -> Result<(String, Vec<i32>)> {
    let element = client
        .focused()
        .map_err(failed("reading the focused element"))
        .context("no focused application is reported — name one with `app`")?;
    // SAFETY: a cached read on a live element.
    let pid = unsafe { element.CachedProcessId() }
        .map_err(failed("reading the focused element's process"))?;
    let name = name_of(pid)?.map_or_else(|| format!("pid {pid}"), |exe| exe_stem(&exe).to_string());
    Ok((name, vec![pid]))
}

/// The elements of a found array; one that cannot be fetched is skipped.
fn elements(array: &IUIAutomationElementArray) -> impl Iterator<Item = IUIAutomationElement> + '_ {
    // SAFETY: reads on a live array, indices bounded by its length.
    let length = unsafe { array.Length() }.unwrap_or(0);
    (0..length).filter_map(move |index| unsafe { array.GetElement(index) }.ok())
}

fn text(read: WinResult<BSTR>) -> Option<String> {
    read.ok()
        .map(|text| text.to_string())
        .filter(|text| !text.is_empty())
}

fn readings(element: &IUIAutomationElement) -> PatternReadings {
    // SAFETY: cached pattern reads on a live element. A pattern the
    // element does not support comes back as an error, i.e. no reading.
    unsafe {
        PatternReadings {
            toggle: element
                .GetCachedPatternAs::<IUIAutomationTogglePattern>(UIA_TogglePatternId)
                .and_then(|pattern| pattern.CachedToggleState())
                .ok()
                .map(|state| state.0),
            text: element
                .GetCachedPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
                .and_then(|pattern| pattern.CachedValue())
                .ok()
                .map(|value| value.to_string()),
            range: element
                .GetCachedPatternAs::<IUIAutomationRangeValuePattern>(UIA_RangeValuePatternId)
                .and_then(|pattern| pattern.CachedValue())
                .ok(),
        }
    }
}

/// Read `element` and its subtree. Errors below the top-level windows are
/// treated as missing attributes: a single unresponsive child must not
/// void the whole observation, and the evaluator reports what it did not
/// see.
fn read_node(
    client: &Client,
    element: &IUIAutomationElement,
    depth: usize,
    budget: &mut usize,
) -> Node {
    *budget = budget.saturating_sub(1);
    // SAFETY: cached reads on a live element.
    let mut node = unsafe {
        let role = element
            .CachedControlType()
            .map_or_else(|_| "Unknown".to_string(), |id| control_type_name(id.0));
        let mut node = Node::new(&role);
        node.title = text(element.CachedName());
        node.identifier = text(element.CachedAutomationId());
        node.enabled = element.CachedIsEnabled().ok().map(BOOL::as_bool);
        node
    };
    node.value = element_value(readings(element));
    if depth >= MAX_DEPTH || *budget == 0 {
        return node;
    }
    if let Ok(children) = client.children(element) {
        for child in elements(&children) {
            if *budget == 0 {
                break;
            }
            node.children
                .push(read_node(client, &child, depth + 1, budget));
        }
    }
    node
}
