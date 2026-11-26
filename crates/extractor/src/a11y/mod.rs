//! GUI-observed postconditions: read-only checks against a running
//! application's accessibility tree.
//!
//! Most GUI steps are labelled `unverifiable` today although many have an
//! observable postcondition — a window title, a checkbox, an enabled menu
//! item, a file on disk. This module gives the verifier a small
//! vocabulary of such checks ([`check::Check`]), a backend-neutral tree
//! to evaluate them against ([`tree::Snapshot`]), a pure evaluator
//! ([`eval::evaluate`]) and the orchestration behind
//! `vts-extract observe` ([`observe::observe`]).
//!
//! Everything here is read-only: no backend performs an action, posts a
//! keystroke or changes an attribute. That is why `observe` runs outside
//! the sandbox — it executes nothing from the video — and also why it is
//! not a substitute for the sandbox: performing a tutorial's GUI actions
//! on the host is still out of scope (see
//! `references/verification-protocol.md`, "GUI-observed tier").
//!
//! Backends: macOS `AXUIElement` ([`macos`]) and Windows UI Automation
//! ([`windows`]). AT-SPI is designed in issue #37 and not implemented
//! yet; on Linux only `file_exists` can be observed.

pub mod check;
pub mod eval;
#[cfg(target_os = "macos")]
pub mod macos;
pub mod observe;
pub mod tree;
// Its UI Automation half is `cfg(windows)` inside; the pure mapping
// compiles everywhere so its tests run on every OS.
pub mod windows;
