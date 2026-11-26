//! The socket under a D-Bus connection: connecting, `EXTERNAL`
//! authentication, and reading whole frames against a deadline.
//!
//! A deadline bounds a whole exchange, not each read: a peer that
//! trickles a reply one byte at a time cannot keep a call alive past its
//! timeout.

use std::fmt::Write as _;
use std::io::{ErrorKind, Read, Write};
use std::os::linux::net::SocketAddrExt;
use std::os::unix::fs::MetadataExt;
use std::os::unix::net::{SocketAddr, UnixStream};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};

use super::address::UnixTarget;
use super::message::{frame_length, FIXED_HEADER_BYTES};

/// The longest authentication line a server may send.
const MAX_AUTH_LINE: usize = 512;

pub(super) struct Socket {
    stream: UnixStream,
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

impl Socket {
    /// Connect to the first of `targets` that accepts and authenticate as
    /// this process's user.
    pub(super) fn open(targets: &[UnixTarget], timeout: Duration) -> Result<Self> {
        let mut failures = Vec::new();
        for target in targets {
            match connect(target) {
                Ok(stream) => {
                    stream.set_write_timeout(Some(timeout))?;
                    let mut socket = Self { stream, timeout };
                    socket.authenticate()?;
                    return Ok(socket);
                }
                Err(error) => failures.push(format!("{}: {error}", describe(target))),
            }
        }
        bail!("could not connect ({})", failures.join("; "))
    }

    pub(super) fn deadline(&self) -> Instant {
        Instant::now() + self.timeout
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
        let deadline = self.deadline();
        self.send(
            format!("\0AUTH EXTERNAL {hex}\r\n").as_bytes(),
            "authenticating",
        )?;
        let mut line = Vec::new();
        let mut byte = [0u8; 1];
        while !line.ends_with(b"\r\n") {
            if line.len() >= MAX_AUTH_LINE {
                bail!("the bus sent an overlong authentication line");
            }
            self.read_until(deadline, &mut byte, "authenticating")?;
            line.push(byte[0]);
        }
        let line = String::from_utf8_lossy(&line).trim_end().to_string();
        if !line.starts_with("OK ") {
            bail!("the bus refused authentication: {line}");
        }
        self.send(b"BEGIN\r\n", "authenticating")
    }

    fn no_answer(&self, doing: &str) -> anyhow::Error {
        anyhow::anyhow!("{doing}: no answer within {} s", self.timeout.as_secs_f32())
    }

    pub(super) fn send(&mut self, bytes: &[u8], doing: &str) -> Result<()> {
        self.stream.write_all(bytes).map_err(|error| {
            if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) {
                self.no_answer(doing)
            } else {
                anyhow::anyhow!("{doing}: {error}")
            }
        })
    }

    /// The next whole message, as raw bytes.
    pub(super) fn receive(&mut self, deadline: Instant, doing: &str) -> Result<Vec<u8>> {
        let mut bytes = vec![0u8; FIXED_HEADER_BYTES];
        self.read_until(deadline, &mut bytes, doing)?;
        let total = frame_length(&bytes)?;
        bytes.resize(total, 0);
        self.read_until(deadline, &mut bytes[FIXED_HEADER_BYTES..], doing)?;
        Ok(bytes)
    }

    /// Fill `buf`, giving up at `deadline` however the bytes arrive.
    fn read_until(&mut self, deadline: Instant, buf: &mut [u8], doing: &str) -> Result<()> {
        let mut filled = 0;
        while filled < buf.len() {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(self.no_answer(doing));
            }
            self.stream.set_read_timeout(Some(remaining))?;
            match self.stream.read(&mut buf[filled..]) {
                Ok(0) => bail!("{doing}: the bus closed the connection"),
                Ok(read) => filled += read,
                Err(error) if error.kind() == ErrorKind::Interrupted => {}
                Err(error)
                    if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) =>
                {
                    return Err(self.no_answer(doing));
                }
                Err(error) => bail!("{doing}: {error}"),
            }
        }
        Ok(())
    }
}
