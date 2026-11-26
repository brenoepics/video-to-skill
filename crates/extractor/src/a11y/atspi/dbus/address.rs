//! D-Bus server addresses (`unix:path=/run/user/1000/bus;tcp:…`): which
//! Unix sockets an address lists, in the order to try them.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

/// A socket a client can connect to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnixTarget {
    Path(PathBuf),
    /// A Linux abstract-namespace socket name (without the leading NUL).
    Abstract(Vec<u8>),
}

/// The Unix sockets `address` lists. Other transports (`tcp:`,
/// `unixexec:`, listen-only `unix:dir=`) are skipped: this client only
/// talks to the local bus, and never executes anything to reach one.
pub fn unix_targets(address: &str) -> Result<Vec<UnixTarget>> {
    let mut out = Vec::new();
    for entry in address.split(';').filter(|entry| !entry.is_empty()) {
        let Some(params) = entry.strip_prefix("unix:") else {
            continue;
        };
        for param in params.split(',') {
            let Some((key, value)) = param.split_once('=') else {
                bail!("malformed D-Bus address entry {entry:?}");
            };
            let value = unescape(value).with_context(|| format!("in D-Bus address {entry:?}"))?;
            match key {
                "path" => out.push(UnixTarget::Path(PathBuf::from(
                    String::from_utf8(value).context("a socket path is not UTF-8")?,
                ))),
                "abstract" => out.push(UnixTarget::Abstract(value)),
                _ => {}
            }
        }
    }
    if out.is_empty() {
        bail!("the D-Bus address {address:?} lists no Unix socket this client can connect to");
    }
    Ok(out)
}

/// `%xx` escapes decoded; every other byte is taken as is.
fn unescape(value: &str) -> Result<Vec<u8>> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%' {
            let hex = value
                .get(at + 1..at + 3)
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
                .with_context(|| format!("bad %-escape in {value:?}"))?;
            out.push(hex);
            at += 3;
        } else {
            out.push(bytes[at]);
            at += 1;
        }
    }
    Ok(out)
}

/// The session bus: `DBUS_SESSION_BUS_ADDRESS` when set, else the
/// systemd convention `$XDG_RUNTIME_DIR/bus`. `None` when neither exists.
pub fn session_bus(
    env_address: Option<&str>,
    runtime_dir: Option<&Path>,
) -> Result<Option<Vec<UnixTarget>>> {
    if let Some(address) = env_address.filter(|address| !address.trim().is_empty()) {
        return unix_targets(address).map(Some);
    }
    Ok(runtime_dir
        .map(|dir| dir.join("bus"))
        .filter(|socket| socket.exists())
        .map(|socket| vec![UnixTarget::Path(socket)]))
}
