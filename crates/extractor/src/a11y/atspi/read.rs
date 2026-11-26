//! Reading one application's tree from the accessibility bus into the
//! neutral snapshot.

use std::time::Instant;

use anyhow::{bail, Context, Result};

use super::bus::{self, explain, Bus, Object};
use super::mapping::{
    self, is_app_named, reads_text, state, text_value, toggle_value, MAX_TEXT_CHARS,
};
use super::{MAX_DEPTH, MAX_NODES, WALK_LIMIT};
use crate::a11y::tree::{Node, Snapshot};

/// One application registered on the bus, with the names it answers to.
struct App {
    root: Object,
    pid: Option<u32>,
    /// The accessible name (`GLib.set_prgname`, `QCoreApplication`, …).
    name: Option<String>,
    /// The kernel's process name and the basename of `argv[0]`.
    process_names: Vec<String>,
}

impl App {
    fn label(&self) -> String {
        let name = self
            .name
            .clone()
            .or_else(|| self.process_names.first().cloned())
            .unwrap_or_else(|| self.root.bus_name.clone());
        match self.pid {
            Some(pid) => format!("{name} (pid {pid})"),
            None => name,
        }
    }
}

fn process_names(pid: u32) -> Vec<String> {
    let mut names = Vec::new();
    if let Ok(comm) = std::fs::read_to_string(format!("/proc/{pid}/comm")) {
        names.push(comm.trim_end().to_string());
    }
    if let Ok(cmdline) = std::fs::read(format!("/proc/{pid}/cmdline")) {
        let argv0 = cmdline.split(|&b| b == 0).next().unwrap_or_default();
        let argv0 = String::from_utf8_lossy(argv0);
        if let Some(base) = argv0.rsplit('/').next().filter(|b| !b.is_empty()) {
            names.push(base.to_string());
        }
    }
    names
}

fn applications(bus: &mut Bus) -> Result<Vec<App>> {
    let mut apps = Vec::new();
    for root in bus.applications()? {
        // The bus daemon answers for the pid, so a failure here is the
        // connection's. The name comes from the application: one that hangs
        // must not stop every other application from being observed.
        let pid = bus.pid(&root.bus_name)?;
        let name = bus.string_property(&root, "Name").ok().flatten();
        apps.push(App {
            pid,
            name,
            process_names: pid.map(process_names).unwrap_or_default(),
            root,
        });
    }
    Ok(apps)
}

/// Whether any of the application's top-level windows is `ACTIVE`; an
/// application that does not answer is not the active one.
fn is_active(bus: &mut Bus, app: &App) -> bool {
    bus.children(&app.root)
        .unwrap_or_default()
        .iter()
        .any(|window| {
            bus.states(window)
                .ok()
                .flatten()
                .is_some_and(|s| s.has(state::ACTIVE))
        })
}

/// Every application `wanted` names, or why there is none.
fn named(apps: Vec<App>, wanted: &str) -> Result<Vec<App>> {
    let (chosen, others): (Vec<App>, Vec<App>) = apps.into_iter().partition(|candidate| {
        let mut names: Vec<&str> = candidate.process_names.iter().map(String::as_str).collect();
        names.extend(candidate.name.as_deref());
        is_app_named(wanted, &names)
    });
    if chosen.is_empty() {
        let registered: Vec<String> = others.iter().map(App::label).collect();
        bail!(
            "no application named {wanted:?} is registered on the accessibility bus \
             (registered: {registered:?}) — launch it first; Qt and Chromium-based \
             applications register only while desktop accessibility is enabled"
        );
    }
    Ok(chosen)
}

/// The first application with an `ACTIVE` window.
fn frontmost(bus: &mut Bus, apps: Vec<App>) -> Result<App> {
    if let Some(active) = apps.into_iter().find(|candidate| is_active(bus, candidate)) {
        return Ok(active);
    }
    bail!("no application reports an active window — name the application with `app`")
}

/// Read the tree of the application named `app` (its process name or
/// its accessible name), or of the one with the active window when
/// `None`. Every registered application of that name contributes.
pub fn snapshot(app: Option<&str>) -> Result<Snapshot> {
    let mut bus = bus::open()?;
    let apps =
        applications(&mut bus).context("listing the applications on the accessibility bus")?;
    let (label, chosen) = if let Some(wanted) = app {
        (wanted.to_string(), named(apps, wanted)?)
    } else {
        let active = frontmost(&mut bus, apps)?;
        (active.label(), vec![active])
    };
    let mut walk = Walk {
        budget: MAX_NODES,
        started: Instant::now(),
        cut: false,
        timed_out: false,
    };
    let mut roots = Vec::new();
    for candidate in &chosen {
        if walk.out_of_time() {
            break;
        }
        if let Err(error) = bus.role_name(&candidate.root)? {
            bail!(
                "{}",
                explain(&format!("reading {}", candidate.label()), &error)
            );
        }
        roots.push(read_node(&mut bus, &candidate.root, None, 0, &mut walk)?);
    }
    let pids: Vec<u32> = chosen.iter().filter_map(|c| c.pid).collect();
    let pid = match pids.as_slice() {
        [first, rest @ ..] if rest.iter().all(|p| p == first) && pids.len() == chosen.len() => {
            i32::try_from(*first).ok()
        }
        _ => None,
    };
    let root = if roots.len() == 1 {
        roots.remove(0)
    } else {
        let mut merged = Node::new("Application").with_title(&label);
        merged.children = roots.into_iter().flat_map(|r| r.children).collect();
        merged
    };
    Ok(Snapshot {
        app: label,
        pid,
        truncated: walk.cut || walk.timed_out,
        timed_out: walk.timed_out,
        root,
    })
}

struct Walk {
    budget: usize,
    started: Instant,
    /// Set when the node budget stopped the walk.
    cut: bool,
    /// Set when the time limit stopped the walk.
    timed_out: bool,
}

impl Walk {
    fn out_of_time(&mut self) -> bool {
        if self.started.elapsed() > WALK_LIMIT {
            self.timed_out = true;
        }
        self.timed_out
    }

    fn may_descend(&mut self, depth: usize) -> bool {
        if self.out_of_time() {
            return false;
        }
        if self.budget == 0 {
            self.cut = true;
            return false;
        }
        depth < MAX_DEPTH
    }
}

/// Read `object` and its subtree. An attribute the element does not
/// answer for is missing, not an error; a transport failure (the bus
/// went away, the application stopped answering) ends the snapshot.
fn read_node(
    bus: &mut Bus,
    object: &Object,
    parent: Option<&str>,
    depth: usize,
    walk: &mut Walk,
) -> Result<Node> {
    walk.budget = walk.budget.saturating_sub(1);
    // `None` when the element did not say: then its text is never read.
    let read_role = bus.role_name(object)?.ok();
    let atspi_role = read_role.clone().unwrap_or_default();
    let states = bus.states(object)?;
    let mut node = Node::new(&mapping::role(
        &atspi_role,
        parent,
        states.unwrap_or_default(),
    ));
    node.title = match bus.string_property(object, "Name")? {
        Some(name) => Some(name),
        None => bus.string_property(object, "Description")?,
    };
    node.identifier = match bus.string_property(object, "AccessibleId")? {
        Some(id) => Some(id),
        None => bus.id_attribute(object)?,
    };
    node.enabled = states.and_then(mapping::States::enabled_reading);
    if walk.out_of_time() {
        return Ok(node);
    }
    node.value = match states.and_then(|s| toggle_value(&atspi_role, s)) {
        Some(toggle) => Some(toggle),
        None => value(bus, object, read_role.as_deref())?,
    };
    if walk.may_descend(depth) {
        for child in bus.children(object)? {
            if !walk.may_descend(depth) {
                break;
            }
            node.children
                .push(read_node(bus, &child, Some(&atspi_role), depth + 1, walk)?);
        }
    }
    Ok(node)
}

/// A slider's or spinner's number, else an element's text — never a
/// password field's, nor one whose role could not be read.
fn value(
    bus: &mut Bus,
    object: &Object,
    atspi_role: Option<&str>,
) -> Result<Option<crate::a11y::tree::Scalar>> {
    let interfaces = bus.interfaces(object)?;
    let has = |name: &str| interfaces.iter().any(|i| i == name);
    if has("org.a11y.atspi.Value") {
        if let Some(number) = bus
            .property(object, "org.a11y.atspi.Value", "CurrentValue")?
            .and_then(|v| v.as_f64())
            .filter(|n| n.is_finite())
        {
            return Ok(Some(crate::a11y::tree::Scalar::Number(number)));
        }
    }
    if has("org.a11y.atspi.Text") && reads_text(atspi_role) {
        let limit = i32::try_from(MAX_TEXT_CHARS + 1).unwrap_or(i32::MAX);
        return Ok(bus.text(object, limit)?.and_then(text_value));
    }
    Ok(None)
}
