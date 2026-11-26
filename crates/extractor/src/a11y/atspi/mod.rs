//! Linux backend: a read-only AT-SPI snapshot of one application.
//!
//! AT-SPI is a D-Bus protocol on a bus of its own, the accessibility bus,
//! whose address the session bus's `org.a11y.Bus` reports (or
//! `AT_SPI_BUS_ADDRESS` names). The registry's root lists one object per
//! application; each application answers for its own tree. GTK, Qt,
//! `LibreOffice`, Firefox and Chromium all expose their trees there —
//! Qt and Chromium only while desktop accessibility is enabled, which
//! `observe` reads as "not registered" and never switches on.
//!
//! Only getters are called (the `bus` module lists them), and every call
//! carries `NO_AUTO_START`, so D-Bus never activates a service to answer
//! one: a session without the accessibility bus launcher is exit 2 with
//! that reason. A launcher that is already running but has not started
//! its bus yet starts it when asked for the address — the request every
//! accessible application makes at startup; `observe` starts no process
//! itself. A reply counts only from the peer that was called (its
//! `SENDER`, set by the bus daemon), so another application cannot answer
//! in its place. Reading needs no permission beyond being in the user's
//! session, since the bus only admits its own user.
//!
//! The snapshot mirrors the other backends: the application's root
//! (role `Application`, titled with its accessible name) whose children
//! are its top-level windows, read with the same depth, node and timeout
//! limits. Roles are mapped onto the spellings macOS and UI Automation
//! use where one exists (`frame` → `Window`, `push button` → `Button`;
//! [`mapping::role`]), toggles report 0/1/2 like theirs, and a password
//! field's text is never read. `app` names an application by its process
//! name, the basename of its `argv[0]` or its accessible name, case
//! ignored; without `app` the application with an `ACTIVE` window is
//! read. Unlike UI Automation, toolkits list a closed menu's entries, so
//! `menu_item_enabled` reads submenu paths such as `["File", "Save"]`.
//!
//! The client is a small D-Bus implementation over `std`'s Unix socket
//! ([`dbus`]): the read-only calls above need method calls with string
//! and integer arguments and a decoder, which is less code to audit than
//! an async D-Bus stack and its dependency tree. The pure halves
//! ([`dbus`] encoding and decoding, [`mapping`]) compile on every OS so
//! their tests run everywhere; the socket and the bus are Linux-only.

#[cfg(target_os = "linux")]
pub mod bus;
pub mod dbus;
pub mod mapping;
#[cfg(target_os = "linux")]
mod read;

use std::time::Duration;

#[cfg(target_os = "linux")]
pub use read::snapshot;

/// Named in every report so a pass is attributable to this substrate.
pub const SUBSTRATE: &str = "Linux AT-SPI (read-only)";

/// Per-call timeout, so an application that stops answering cannot hang
/// the verifier (the other backends' messaging timeout).
pub const MESSAGING_TIMEOUT: Duration = Duration::from_secs(2);
/// Depth and node budgets, as on the other backends; the snapshot says
/// when it hit them.
pub const MAX_DEPTH: usize = 32;
pub const MAX_NODES: usize = 5_000;
/// A whole walk stops here even if every call answers in time.
pub const WALK_LIMIT: Duration = Duration::from_secs(30);
