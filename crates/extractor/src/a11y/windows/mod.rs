//! Windows backend: a read-only UI Automation snapshot of one application.
//!
//! Only cached reads are made: `FindAllBuildCache` over an element's
//! children with one cache request for `Name`, `AutomationId`,
//! `ControlType`, `IsEnabled`, `ProcessId` and the `Toggle`, `Value` and
//! `RangeValue` patterns' values, then the `Cached*` getters. No pattern
//! method is ever called (no `Invoke`, `Toggle`, `SetValue`, `Expand`),
//! focus is never set (`AutoSetFocus` is turned off) and no input is
//! sent. Windows asks for no permission: any process may read the trees
//! of applications running at its own integrity level or below.
//!
//! The snapshot mirrors the macOS one: a synthesized `Application` root
//! titled with the application name, whose children are the process's
//! top-level windows (role `Window`, title = the window's `Name`), read
//! in the raw view with the same depth, node and timeout limits. `app`
//! names a process by executable name (`notepad` or `Notepad.exe`, case
//! ignored) through a Toolhelp32 snapshot, and every running process of
//! that name contributes its windows; without `app` the process owning
//! the focused element is read. A packaged app's frame window belongs to
//! `ApplicationFrameHost`, so that is the name to observe it by.
//!
//! What cannot be observed: UI Automation lists a menu's entries only
//! while the menu is open, and opening it is an action, so a
//! `menu_item_enabled` path below the menu bar is refused before anything
//! is read ([`refuse_unobservable`]); a menu bar entry itself (`["File"]`)
//! is read like any element.
//!
//! The pure half ([`mapping`] and the refusal) compiles on every OS so
//! its tests run everywhere; everything that calls COM is `cfg(windows)`.

use anyhow::{bail, Result};

use super::check::Check;

#[cfg(windows)]
mod com;
pub mod mapping;
#[cfg(windows)]
mod process;
#[cfg(windows)]
mod read;
#[cfg(test)]
mod tests;

#[cfg(windows)]
pub use process::pids_named;
#[cfg(windows)]
pub use read::snapshot;

/// Named in every report so a pass is attributable to this substrate.
pub const SUBSTRATE: &str = "Windows UI Automation (read-only)";

/// UI Automation's connection and transaction timeout, so an application
/// that stops answering cannot hang the verifier (the macOS backend's
/// messaging timeout).
pub const MESSAGING_TIMEOUT_MS: u32 = 2_000;

/// Refuse the checks this backend cannot answer without performing an
/// action, before anything is read, so the verifier records
/// `unverifiable` with the reason instead of a false `fail`.
pub fn refuse_unobservable(checks: &[Check]) -> Result<()> {
    for check in checks {
        let Check::MenuItemEnabled { app, menu, .. } = check else {
            continue;
        };
        if let [top, _, ..] = menu.as_slice() {
            bail!(
                "{}: could not observe {}: the menu path {} is not observable on Windows — \
                 UI Automation lists a menu's entries only while the menu is open, and opening \
                 {top:?} is an action observe never performs; only a menu bar entry itself, \
                 such as [{top:?}], can be read. Mark the step unverifiable with this reason",
                check.kind(),
                app.as_deref().unwrap_or("the frontmost application"),
                menu.join(" > "),
            );
        }
    }
    Ok(())
}
