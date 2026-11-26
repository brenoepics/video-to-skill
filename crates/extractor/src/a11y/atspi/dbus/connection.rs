//! A blocking D-Bus client connection over a Unix socket: `EXTERNAL`
//! authentication, `Hello`, then one method call at a time.
//!
//! Every call is bounded by the caller's timeout, so a bus or an
//! application that stops answering ends the call with an error instead
//! of hanging the verifier.
//!
//! Only the reply to the call in flight is returned, and a reply is
//! exactly this: a `METHOD_RETURN` or `ERROR` whose `REPLY_SERIAL` is the
//! call's and whose `SENDER` — which the bus daemon sets and no peer can
//! forge — is the peer that was called (for a well-known name, its
//! current owner). Anything else is skipped: a bus lets any client send
//! to any other and serials are predictable, so a signal, or another
//! application's message, carrying the right serial must not be taken
//! for the answer. A reply whose signature is not the one the call
//! expects is not decoded at all.

use std::collections::HashMap;
use std::time::Duration;

use anyhow::{bail, Context, Result};

use super::address::UnixTarget;
use super::message::{decode_header, encode_call, Arg, Call, Kind};
use super::transport::Socket;
use super::value::Value;

/// The bus daemon's own name, which it uses as the sender of its replies.
const BUS: &str = "org.freedesktop.DBus";
/// The error name given to a reply with a signature the call did not ask
/// for; callers treat it like any other error reply.
pub const UNEXPECTED_SIGNATURE: &str = "vts.Error.UnexpectedSignature";

/// An error reply: the D-Bus error name and its message, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteError {
    pub name: String,
    pub message: String,
}

pub struct Connection {
    socket: Socket,
    serial: u32,
    /// Well-known names resolved to their unique owners.
    owners: HashMap<String, String>,
}

impl Connection {
    /// Connect to the first of `targets` that accepts, authenticate as
    /// this process's user and register with the bus.
    pub fn open(targets: &[UnixTarget], timeout: Duration) -> Result<Self> {
        let mut connection = Self {
            socket: Socket::open(targets, timeout)?,
            serial: 0,
            owners: HashMap::new(),
        };
        let hello = Call {
            destination: BUS,
            path: "/org/freedesktop/DBus",
            interface: BUS,
            member: "Hello",
            args: Vec::new(),
            reply: "s",
        };
        if let Err(error) = connection.call(&hello)? {
            bail!("the bus refused Hello: {} {}", error.name, error.message);
        }
        Ok(connection)
    }

    /// Make `call` and wait for its reply: the reply's body, or the
    /// error the peer answered with. A transport failure or a timeout is
    /// the outer error.
    pub fn call(&mut self, call: &Call<'_>) -> Result<Result<Vec<Value>, RemoteError>> {
        match self.sender_of(call.destination)? {
            Ok(sender) => self.exchange(call, &sender),
            Err(error) => Ok(Err(error)),
        }
    }

    /// Who answers for `destination`: the bus itself, a unique name, or
    /// the current owner of a well-known name.
    fn sender_of(&mut self, destination: &str) -> Result<Result<String, RemoteError>> {
        if destination == BUS || destination.starts_with(':') {
            return Ok(Ok(destination.to_string()));
        }
        if let Some(owner) = self.owners.get(destination) {
            return Ok(Ok(owner.clone()));
        }
        let lookup = Call {
            destination: BUS,
            path: "/org/freedesktop/DBus",
            interface: BUS,
            member: "GetNameOwner",
            args: vec![Arg::Str(destination)],
            reply: "s",
        };
        Ok(match self.exchange(&lookup, BUS)? {
            Ok(body) => {
                let owner = body
                    .first()
                    .and_then(Value::as_str)
                    .context("GetNameOwner returned no owner")?
                    .to_string();
                self.owners.insert(destination.to_string(), owner.clone());
                Ok(owner)
            }
            Err(error) => Err(error),
        })
    }

    fn exchange(
        &mut self,
        call: &Call<'_>,
        sender: &str,
    ) -> Result<Result<Vec<Value>, RemoteError>> {
        self.serial = self.serial.checked_add(1).unwrap_or(1);
        let serial = self.serial;
        let doing = format!("calling {}.{}", call.interface, call.member);
        let bytes = encode_call(call, serial)?;
        let deadline = self.socket.deadline();
        self.socket.send(&bytes, &doing)?;
        loop {
            let bytes = self.socket.receive(deadline, &doing)?;
            let header = decode_header(&bytes)?;
            let answers = matches!(header.kind, Kind::MethodReturn | Kind::Error)
                && header.reply_serial == Some(serial)
                && header.sender.as_deref() == Some(sender);
            if !answers {
                continue;
            }
            if header.kind == Kind::Error {
                let body = header.body(&bytes).unwrap_or_default();
                return Ok(Err(RemoteError {
                    name: header.error_name.unwrap_or_default(),
                    message: body
                        .first()
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                }));
            }
            if header.signature != call.reply {
                return Ok(Err(RemoteError {
                    name: UNEXPECTED_SIGNATURE.to_string(),
                    message: format!(
                        "{} answered {:?}, not {:?}",
                        call.member, header.signature, call.reply
                    ),
                }));
            }
            return Ok(Ok(header.body(&bytes)?));
        }
    }
}
