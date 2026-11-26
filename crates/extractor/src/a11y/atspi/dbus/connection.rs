//! A blocking D-Bus client connection over a Unix socket: `EXTERNAL`
//! authentication, `Hello`, then one method call at a time.
//!
//! Every read and write carries the caller's timeout, so a bus or an
//! application that stops answering ends the call with an error instead
//! of hanging the verifier. Signals and replies to other calls are
//! skipped; only the reply to the call in flight is returned.

use std::fmt::Write as _;
use std::io::{ErrorKind, Read, Write};
use std::os::linux::net::SocketAddrExt;
use std::os::unix::fs::MetadataExt;
use std::os::unix::net::{SocketAddr, UnixStream};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};

use super::address::UnixTarget;
use super::message::{decode, encode_call, frame_length, Call, Kind, FIXED_HEADER_BYTES};
use super::value::Value;

/// The longest authentication line a server may send.
const MAX_AUTH_LINE: usize = 512;

/// An error reply: the D-Bus error name and its message, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteError {
    pub name: String,
    pub message: String,
}

pub struct Connection {
    stream: UnixStream,
    serial: u32,
    timeout: Duration,
}

fn connect(target: &UnixTarget) -> std::io::Result<UnixStream> {
    match target {
        UnixTarget::Path(path) => UnixStream::connect(path),
        UnixTarget::Abstract(name) => {
            UnixStream::connect_addr(&SocketAddr::from_abstract_name(name)?)
        }
    }
}

fn describe(target: &UnixTarget) -> String {
    match target {
        UnixTarget::Path(path) => path.display().to_string(),
        UnixTarget::Abstract(name) => format!("@{}", String::from_utf8_lossy(name)),
    }
}

impl Connection {
    /// Connect to the first of `targets` that accepts, authenticate as
    /// this process's user and register with the bus.
    pub fn open(targets: &[UnixTarget], timeout: Duration) -> Result<Self> {
        let mut failures = Vec::new();
        let mut stream = None;
        for target in targets {
            match connect(target) {
                Ok(connected) => {
                    stream = Some(connected);
                    break;
                }
                Err(error) => failures.push(format!("{}: {error}", describe(target))),
            }
        }
        let Some(stream) = stream else {
            bail!("could not connect ({})", failures.join("; "));
        };
        stream.set_read_timeout(Some(timeout))?;
        stream.set_write_timeout(Some(timeout))?;
        let mut connection = Self {
            stream,
            serial: 0,
            timeout,
        };
        connection.authenticate()?;
        let hello = Call {
            destination: "org.freedesktop.DBus",
            path: "/org/freedesktop/DBus",
            interface: "org.freedesktop.DBus",
            member: "Hello",
            args: Vec::new(),
        };
        if let Err(error) = connection.call(&hello)? {
            bail!("the bus refused Hello: {} {}", error.name, error.message);
        }
        Ok(connection)
    }

    fn authenticate(&mut self) -> Result<()> {
        // The uid that owns this process, without a libc binding.
        let uid = std::fs::metadata("/proc/self")
            .context("reading this process's uid from /proc/self")?
            .uid();
        let hex = uid.to_string().bytes().fold(String::new(), |mut out, b| {
            // Writing into a String cannot fail.
            let _ = write!(out, "{b:02x}");
            out
        });
        self.stream
            .write_all(format!("\0AUTH EXTERNAL {hex}\r\n").as_bytes())
            .context("sending authentication")?;
        let line = self.auth_line()?;
        if !line.starts_with("OK ") {
            bail!("the bus refused authentication: {line}");
        }
        self.stream
            .write_all(b"BEGIN\r\n")
            .context("sending authentication")?;
        Ok(())
    }

    fn auth_line(&mut self) -> Result<String> {
        let mut line = Vec::new();
        let mut byte = [0u8; 1];
        while !line.ends_with(b"\r\n") {
            if line.len() >= MAX_AUTH_LINE {
                bail!("the bus sent an overlong authentication line");
            }
            self.stream
                .read_exact(&mut byte)
                .map_err(|error| self.io_error(&error, "authenticating"))?;
            line.push(byte[0]);
        }
        Ok(String::from_utf8_lossy(&line).trim_end().to_string())
    }

    fn io_error(&self, error: &std::io::Error, doing: &str) -> anyhow::Error {
        if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) {
            anyhow::anyhow!("{doing}: no answer within {} s", self.timeout.as_secs_f32())
        } else {
            anyhow::anyhow!("{doing}: {error}")
        }
    }

    /// Make `call` and wait for its reply: the reply's body, or the
    /// error the peer answered with. A transport failure or a timeout is
    /// the outer error.
    pub fn call(&mut self, call: &Call<'_>) -> Result<Result<Vec<Value>, RemoteError>> {
        self.serial = self.serial.checked_add(1).unwrap_or(1);
        let serial = self.serial;
        let doing = format!("calling {}.{}", call.interface, call.member);
        let bytes = encode_call(call, serial)?;
        self.stream
            .write_all(&bytes)
            .map_err(|error| self.io_error(&error, &doing))?;
        let deadline = Instant::now() + self.timeout;
        loop {
            let message = self.receive(deadline, &doing)?;
            if message.reply_serial != Some(serial) {
                continue;
            }
            return Ok(match message.kind {
                Kind::Error => Err(RemoteError {
                    name: message.error_name.unwrap_or_default(),
                    message: message
                        .body
                        .first()
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                }),
                _ => Ok(message.body),
            });
        }
    }

    fn receive(&mut self, deadline: Instant, doing: &str) -> Result<super::message::Message> {
        let mut bytes = vec![0u8; FIXED_HEADER_BYTES];
        self.read_until(deadline, &mut bytes, doing)?;
        let total = frame_length(&bytes)?;
        bytes.resize(total, 0);
        self.read_until(deadline, &mut bytes[FIXED_HEADER_BYTES..], doing)?;
        decode(&bytes)
    }

    fn read_until(&mut self, deadline: Instant, buf: &mut [u8], doing: &str) -> Result<()> {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            bail!("{doing}: no answer within {} s", self.timeout.as_secs_f32());
        }
        self.stream.set_read_timeout(Some(remaining))?;
        self.stream
            .read_exact(buf)
            .map_err(|error| self.io_error(&error, doing))
    }
}
