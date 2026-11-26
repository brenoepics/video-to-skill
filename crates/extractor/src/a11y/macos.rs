//! macOS backend: a read-only `AXUIElement` snapshot of one application.
//!
//! Only `AXUIElementCopyAttributeValue` is ever called — no action is
//! performed, no attribute is set, no event is posted. Reading another
//! process's tree needs the Accessibility permission (System Settings →
//! Privacy & Security → Accessibility) for the process running
//! `vts-extract`; without it the read is refused with that explanation
//! so the verifier records `unverifiable`, never a false `fail`.
//!
//! Bindings: `objc2-application-services` and `objc2-core-foundation`,
//! pure Rust over the system frameworks, so the build stays a plain
//! `cargo build --locked` (the `axuielement` crate would have added a
//! `swift build` step and a Swift runtime link to every macOS build).

use std::ptr::{self, NonNull};

use anyhow::{bail, Context, Result};
use objc2_application_services::{AXError, AXIsProcessTrustedWithOptions, AXUIElement};
use objc2_core_foundation::{CFArray, CFBoolean, CFNumber, CFRetained, CFString, CFType};

use super::tree::{Node, Scalar, Snapshot};

/// Named in every report so a pass is attributable to this substrate.
pub const SUBSTRATE: &str = "macOS AXUIElement (read-only)";

/// Depth and node budgets keep a web view or a huge document from turning
/// one observation into minutes of IPC; the snapshot says when it hit them.
const MAX_DEPTH: usize = 32;
const MAX_NODES: usize = 5_000;
/// Per-request timeout, so an application that stops answering cannot
/// hang the verifier.
const MESSAGING_TIMEOUT_SECS: f32 = 2.0;

/// Whether this process may read other applications' trees.
#[must_use]
pub fn is_trusted() -> bool {
    // SAFETY: a null options dictionary is documented as "no options".
    unsafe { AXIsProcessTrustedWithOptions(None) }
}

/// Read the tree of the application named `app` (its process name, as
/// `pgrep -x` matches it), or of the frontmost application when `None`.
pub fn snapshot(app: Option<&str>) -> Result<Snapshot> {
    if !is_trusted() {
        bail!(
            "this process is not a trusted Accessibility client — grant Accessibility \
             permission to the terminal or agent host running vts-extract (System Settings → \
             Privacy & Security → Accessibility) and retry"
        );
    }
    let (name, pid, element) = match app {
        Some(name) => {
            let pid = pid_of(name)?;
            // SAFETY: any pid is accepted; a stale one yields an element
            // whose reads fail, which is reported below.
            (name.to_string(), pid, unsafe {
                AXUIElement::new_application(pid)
            })
        }
        None => frontmost()?,
    };
    // SAFETY: plain setter on a valid element.
    unsafe { element.set_messaging_timeout(MESSAGING_TIMEOUT_SECS) };
    if string_attribute(&element, "AXRole")?.is_none() {
        bail!("{name} (pid {pid}) does not answer accessibility requests");
    }
    let mut budget = MAX_NODES;
    let root = read_node(&element, 0, &mut budget);
    Ok(Snapshot {
        app: name,
        pid: Some(pid),
        truncated: budget == 0,
        root,
    })
}

fn pid_of(name: &str) -> Result<i32> {
    let listed = crate::host::command_stdout("pgrep", &["-x", "-n", name]).with_context(|| {
        format!(
            "no running process named {name:?} — launch the application first, or omit \
             `app` to observe the frontmost application"
        )
    })?;
    listed
        .lines()
        .next()
        .and_then(|line| line.trim().parse::<i32>().ok())
        .with_context(|| format!("pgrep returned no pid for {name:?}"))
}

fn frontmost() -> Result<(String, i32, CFRetained<AXUIElement>)> {
    // SAFETY: creating the system-wide element has no preconditions.
    let system = unsafe { AXUIElement::new_system_wide() };
    let element = attribute(&system, "AXFocusedApplication")?
        .and_then(|value| value.downcast::<AXUIElement>().ok())
        .context("no frontmost application is reported")?;
    let mut pid: i32 = 0;
    // SAFETY: `pid` outlives the call and the pointer is non-null.
    let status = unsafe { element.pid(NonNull::from(&mut pid)) };
    if status != AXError::Success {
        bail!(
            "could not read the frontmost application's pid ({})",
            status.0
        );
    }
    let name = string_attribute(&element, "AXTitle")?.unwrap_or_else(|| format!("pid {pid}"));
    Ok((name, pid, element))
}

/// One attribute, following the Copy rule: the returned value is owned.
/// Missing or unsupported attributes are `None`; a refused API is an error.
fn attribute(element: &AXUIElement, name: &str) -> Result<Option<CFRetained<CFType>>> {
    let key = CFString::from_str(name);
    let mut raw: *const CFType = ptr::null();
    // SAFETY: `raw` is a valid out-pointer for the duration of the call.
    let status = unsafe { element.copy_attribute_value(&key, NonNull::from(&mut raw)) };
    match status {
        AXError::Success => {
            // SAFETY: on success the callee stored a +1 reference (Copy rule)
            // or null, and ownership passes to the caller.
            Ok(NonNull::new(raw.cast_mut()).map(|ptr| unsafe { CFRetained::from_raw(ptr) }))
        }
        AXError::NoValue | AXError::AttributeUnsupported => Ok(None),
        AXError::APIDisabled => bail!(
            "the accessibility API is disabled for this process — grant Accessibility \
             permission (System Settings → Privacy & Security → Accessibility)"
        ),
        AXError::InvalidUIElement => {
            bail!("reading {name}: the element is gone (did the application quit?)")
        }
        AXError::CannotComplete => bail!(
            "reading {name}: the application did not answer within {MESSAGING_TIMEOUT_SECS} s \
             or does not support accessibility"
        ),
        other => bail!("reading {name} failed with AXError {}", other.0),
    }
}

fn string_attribute(element: &AXUIElement, name: &str) -> Result<Option<String>> {
    Ok(attribute(element, name)?
        .and_then(|value| value.downcast_ref::<CFString>().map(ToString::to_string)))
}

fn scalar(value: &CFType) -> Option<Scalar> {
    if let Some(text) = value.downcast_ref::<CFString>() {
        return Some(Scalar::Text(text.to_string()));
    }
    if let Some(flag) = value.downcast_ref::<CFBoolean>() {
        return Some(Scalar::Bool(flag.as_bool()));
    }
    value
        .downcast_ref::<CFNumber>()?
        .as_f64()
        .map(Scalar::Number)
}

/// Read `element` and its subtree. Errors below the root are treated as
/// missing attributes: a single unresponsive child must not void the
/// whole observation, and the evaluator reports what it did not see.
fn read_node(element: &AXUIElement, depth: usize, budget: &mut usize) -> Node {
    *budget = budget.saturating_sub(1);
    let get = |name: &str| attribute(element, name).ok().flatten();
    let text =
        |name: &str| get(name).and_then(|v| v.downcast_ref::<CFString>().map(ToString::to_string));
    let mut node = Node::new(&text("AXRole").unwrap_or_else(|| "AXUnknown".to_string()));
    // Toolbar buttons and icons carry their label in AXDescription instead.
    node.title = text("AXTitle")
        .filter(|t| !t.is_empty())
        .or_else(|| text("AXDescription").filter(|t| !t.is_empty()));
    node.identifier = text("AXIdentifier").filter(|t| !t.is_empty());
    node.value = get("AXValue").and_then(|v| scalar(&v));
    node.enabled =
        get("AXEnabled").and_then(|v| v.downcast_ref::<CFBoolean>().map(CFBoolean::as_bool));
    if depth >= MAX_DEPTH || *budget == 0 {
        return node;
    }
    if let Some(children) = get("AXChildren").and_then(|v| v.downcast::<CFArray>().ok()) {
        // SAFETY: AXChildren is documented as an array of AXUIElementRef,
        // i.e. CoreFoundation objects, so viewing it as CFArray<CFType> is
        // sound; each element is still checked by downcast below.
        let children = unsafe { CFRetained::cast_unchecked::<CFArray<CFType>>(children) };
        for child in children.iter() {
            if *budget == 0 {
                break;
            }
            if let Some(child) = child.downcast_ref::<AXUIElement>() {
                node.children.push(read_node(child, depth + 1, budget));
            }
        }
    }
    node
}
