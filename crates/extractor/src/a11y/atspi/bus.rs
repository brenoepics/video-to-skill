//! The accessibility bus and the read-only AT-SPI calls made on it.
//!
//! Only getters are called: `GetChildren`, `GetRoleName`, `GetState`,
//! `GetInterfaces`, `GetAttributes`, `Text.GetText` and
//! `Properties.Get`. No `Action`, `EditableText`, `Selection`,
//! `Component` or `Value` setter is ever invoked, and no event listener
//! is registered.

use std::path::PathBuf;

use anyhow::{bail, Context, Result};

use super::dbus::address::{session_bus, unix_targets};
use super::dbus::connection::{Connection, RemoteError};
use super::dbus::message::{Arg, Call};
use super::dbus::value::Value;
use super::mapping::{describe_remote, States};
use super::MESSAGING_TIMEOUT;

const ACCESSIBLE: &str = "org.a11y.atspi.Accessible";
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";
/// The path AT-SPI uses for "no object".
const NULL_PATH: &str = "/org/a11y/atspi/null";

/// One accessible object: the application's bus name and its path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Object {
    pub bus_name: String,
    pub path: String,
}

impl Object {
    /// The desktop: the registry's root, whose children are applications.
    #[must_use]
    pub fn desktop() -> Self {
        Self {
            bus_name: "org.a11y.atspi.Registry".into(),
            path: "/org/a11y/atspi/accessible/root".into(),
        }
    }
}

pub struct Bus {
    connection: Connection,
}

/// Find and connect to the accessibility bus: `AT_SPI_BUS_ADDRESS` when
/// set, else the address the session's `org.a11y.Bus` reports. That is
/// asked with `NO_AUTO_START`, so D-Bus never activates the bus launcher
/// to answer: a session without one is reported. A launcher that is
/// already running but has not yet started its bus does start it when
/// asked — the same request every accessible application makes when it
/// starts — and nothing else is started.
pub fn open() -> Result<Bus> {
    let timeout = MESSAGING_TIMEOUT;
    if let Some(address) = std::env::var("AT_SPI_BUS_ADDRESS")
        .ok()
        .filter(|a| !a.trim().is_empty())
    {
        let connection = unix_targets(&address)
            .and_then(|targets| Connection::open(&targets, timeout))
            .with_context(|| {
                format!("the accessibility bus at AT_SPI_BUS_ADDRESS={address} does not answer")
            })?;
        return Ok(Bus { connection });
    }
    let env_address = std::env::var("DBUS_SESSION_BUS_ADDRESS").ok();
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from);
    let Some(targets) = session_bus(env_address.as_deref(), runtime_dir.as_deref())? else {
        bail!(
            "no D-Bus session bus (DBUS_SESSION_BUS_ADDRESS is unset and there is no \
             $XDG_RUNTIME_DIR/bus) — AT-SPI is only reachable from inside a desktop session, \
             so run observe there; mark the step unverifiable with this reason"
        );
    };
    let mut session =
        Connection::open(&targets, timeout).context("the D-Bus session bus does not answer")?;
    let reply = session.call(&Call {
        destination: "org.a11y.Bus",
        path: "/org/a11y/bus",
        interface: "org.a11y.Bus",
        member: "GetAddress",
        args: Vec::new(),
        reply: "s",
    })?;
    let address = match reply {
        Ok(body) => body
            .first()
            .and_then(Value::as_str)
            .map(str::to_string)
            .context("org.a11y.Bus.GetAddress returned no address")?,
        Err(error) => bail!(
            "no accessibility bus is running in this session ({}) — applications expose \
             nothing to AT-SPI until at-spi2-core's bus launcher is up, and observe never \
             starts it; mark the step unverifiable with this reason",
            error.name
        ),
    };
    let connection = unix_targets(&address)
        .and_then(|targets| Connection::open(&targets, timeout))
        .with_context(|| format!("the accessibility bus at {address} does not answer"))?;
    Ok(Bus { connection })
}

impl Bus {
    /// One call on `object`: its reply, or the error the peer gave.
    pub fn ask(
        &mut self,
        object: &Object,
        interface: &str,
        member: &str,
        args: Vec<Arg<'_>>,
        reply: &str,
    ) -> Result<Result<Vec<Value>, RemoteError>> {
        self.connection.call(&Call {
            destination: &object.bus_name,
            path: &object.path,
            interface,
            member,
            args,
            reply,
        })
    }

    /// The first value of a reply, with an error reply as `None`: a
    /// single element that does not answer must not void the snapshot.
    fn first(
        &mut self,
        object: &Object,
        interface: &str,
        member: &str,
        args: Vec<Arg<'_>>,
        reply: &str,
    ) -> Result<Option<Value>> {
        Ok(self
            .ask(object, interface, member, args, reply)?
            .ok()
            .and_then(|body| body.into_iter().next()))
    }

    pub fn role_name(&mut self, object: &Object) -> Result<Result<String, RemoteError>> {
        Ok(self
            .ask(object, ACCESSIBLE, "GetRoleName", Vec::new(), "s")?
            .map(|body| {
                body.first()
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string()
            }))
    }

    pub fn property(
        &mut self,
        object: &Object,
        interface: &str,
        name: &str,
    ) -> Result<Option<Value>> {
        self.first(
            object,
            PROPERTIES,
            "Get",
            vec![Arg::Str(interface), Arg::Str(name)],
            "v",
        )
    }

    pub fn string_property(&mut self, object: &Object, name: &str) -> Result<Option<String>> {
        Ok(self
            .property(object, ACCESSIBLE, name)?
            .and_then(|value| value.as_str().map(str::to_string))
            .filter(|text| !text.is_empty()))
    }

    pub fn children(&mut self, object: &Object) -> Result<Vec<Object>> {
        Ok(self
            .first(object, ACCESSIBLE, "GetChildren", Vec::new(), "a(so)")?
            .map(|list| objects(&list))
            .unwrap_or_default())
    }

    /// The applications registered with the registry; a bus without a
    /// registry is said to be one, not read as "no applications".
    pub fn applications(&mut self) -> Result<Vec<Object>> {
        match self.ask(
            &Object::desktop(),
            ACCESSIBLE,
            "GetChildren",
            Vec::new(),
            "a(so)",
        )? {
            Ok(body) => Ok(body.first().map(objects).unwrap_or_default()),
            Err(error) => bail!(
                "the AT-SPI registry (org.a11y.atspi.Registry) does not answer on the \
                 accessibility bus ({}) — at-spi2-core's registry daemon is not running, so no \
                 application can be listed; mark the step unverifiable with this reason",
                error.name
            ),
        }
    }

    pub fn states(&mut self, object: &Object) -> Result<Option<States>> {
        Ok(self
            .first(object, ACCESSIBLE, "GetState", Vec::new(), "au")?
            .and_then(|value| {
                let words: Option<Vec<u32>> = value.items()?.iter().map(Value::as_u32).collect();
                words.map(|words| States::from_words(&words))
            }))
    }

    pub fn interfaces(&mut self, object: &Object) -> Result<Vec<String>> {
        Ok(self
            .first(object, ACCESSIBLE, "GetInterfaces", Vec::new(), "as")?
            .and_then(|value| {
                value.items().map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
            })
            .unwrap_or_default())
    }

    /// The `id` object attribute (web content, some toolkits).
    pub fn id_attribute(&mut self, object: &Object) -> Result<Option<String>> {
        let Some(map) = self.first(object, ACCESSIBLE, "GetAttributes", Vec::new(), "a{ss}")?
        else {
            return Ok(None);
        };
        Ok(map.items().unwrap_or_default().iter().find_map(|entry| {
            let Value::DictEntry(pair) = entry else {
                return None;
            };
            (pair.0.as_str() == Some("id"))
                .then(|| pair.1.as_str().map(str::to_string))
                .flatten()
                .filter(|id| !id.is_empty())
        }))
    }

    /// The first `chars` characters of the element's text.
    pub fn text(&mut self, object: &Object, chars: i32) -> Result<Option<String>> {
        Ok(self
            .first(
                object,
                "org.a11y.atspi.Text",
                "GetText",
                vec![Arg::I32(0), Arg::I32(chars)],
                "s",
            )?
            .and_then(|value| value.as_str().map(str::to_string)))
    }

    /// The pid behind a bus name, as the bus itself reports it.
    pub fn pid(&mut self, bus_name: &str) -> Result<Option<u32>> {
        let bus = Object {
            bus_name: "org.freedesktop.DBus".into(),
            path: "/org/freedesktop/DBus".into(),
        };
        Ok(self
            .first(
                &bus,
                "org.freedesktop.DBus",
                "GetConnectionUnixProcessID",
                vec![Arg::Str(bus_name)],
                "u",
            )?
            .and_then(|value| value.as_u32()))
    }
}

/// The `(so)` references in a `GetChildren` reply, without null ones.
fn objects(list: &Value) -> Vec<Object> {
    list.items()
        .unwrap_or_default()
        .iter()
        .filter_map(|entry| match entry.items()? {
            [bus_name, path] => Some(Object {
                bus_name: bus_name.as_str()?.to_string(),
                path: path.as_str()?.to_string(),
            }),
            _ => None,
        })
        .filter(|child| child.path != NULL_PATH && !child.bus_name.is_empty())
        .collect()
}

/// An error reply as the sentence `observe` prints.
#[must_use]
pub fn explain(doing: &str, error: &RemoteError) -> String {
    describe_remote(doing, &error.name, &error.message)
}
