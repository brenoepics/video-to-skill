#![cfg(target_os = "linux")]

//! The D-Bus connection against a scripted fake bus: what it accepts as
//! the reply to a call. The accessibility bus lets any client send to any
//! other and serials are predictable, so only a `METHOD_RETURN` or `ERROR`
//! from the peer that was called may answer — never a signal, and never
//! another application — and a reply must arrive whole within the call's
//! timeout, however slowly it trickles.

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used)]

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use vts_extract::a11y::atspi::dbus::address::UnixTarget;
use vts_extract::a11y::atspi::dbus::connection::Connection;
use vts_extract::a11y::atspi::dbus::message::{frame_length, Call};
use vts_extract::a11y::atspi::dbus::value::Value;

const BUS: &str = "org.freedesktop.DBus";
const TIMEOUT: Duration = Duration::from_millis(400);

/// A `s` body holding `text`.
fn text(text: &str) -> Vec<u8> {
    let mut body = u32::try_from(text.len()).unwrap().to_le_bytes().to_vec();
    body.extend(text.as_bytes());
    body.push(0);
    body
}

/// One message: type `kind`, `REPLY_SERIAL`, an optional `SENDER`, a body.
fn message(
    kind: u8,
    reply_serial: u32,
    sender: Option<&str>,
    signature: &str,
    body: &[u8],
) -> Vec<u8> {
    let pad = |bytes: &mut Vec<u8>| bytes.resize(bytes.len().next_multiple_of(8), 0);
    let len = |n: usize| u32::try_from(n).unwrap().to_le_bytes();
    let mut fields = vec![5, 1, b'u', 0];
    fields.extend(reply_serial.to_le_bytes());
    if let Some(sender) = sender {
        fields.extend([7, 1, b's', 0]);
        fields.extend(len(sender.len()));
        fields.extend(sender.as_bytes());
        fields.push(0);
        pad(&mut fields);
    }
    fields.extend([8, 1, b'g', 0, u8::try_from(signature.len()).unwrap()]);
    fields.extend(signature.as_bytes());
    fields.push(0);
    let mut out = vec![b'l', kind, 0, 1];
    out.extend(len(body.len()));
    out.extend(len(77));
    out.extend(len(fields.len()));
    out.extend(&fields);
    pad(&mut out);
    out.extend(body);
    out
}

fn read_message(stream: &mut impl Read) -> Vec<u8> {
    let mut bytes = vec![0u8; 16];
    stream.read_exact(&mut bytes).unwrap();
    bytes.resize(frame_length(&bytes).unwrap(), 0);
    stream.read_exact(&mut bytes[16..]).unwrap();
    bytes
}

/// A bus that authenticates, answers Hello, reads one call and then
/// lets `script` answer it.
fn fake_bus(name: &str, script: fn(&mut UnixStream)) -> (PathBuf, JoinHandle<()>) {
    let dir = std::env::temp_dir()
        .join("vts-atspi-dbus-tests")
        .join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("bus");
    let listener = UnixListener::bind(&path).unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut line = Vec::new();
        reader.read_until(b'\n', &mut line).unwrap();
        stream
            .write_all(b"OK 0123456789abcdef0123456789abcdef\r\n")
            .unwrap();
        line.clear();
        reader.read_until(b'\n', &mut line).unwrap();
        read_message(&mut reader);
        stream
            .write_all(&message(2, 1, Some(BUS), "s", &text(":1.9")))
            .unwrap();
        read_message(&mut reader);
        script(&mut stream);
        // Hold the connection open until the client has given up.
        std::thread::sleep(TIMEOUT * 3);
    });
    (path, server)
}

fn call_app(path: PathBuf) -> anyhow::Result<Result<Vec<Value>, String>> {
    let mut connection = Connection::open(&[UnixTarget::Path(path)], TIMEOUT)?;
    let reply = connection.call(&Call {
        destination: ":1.5",
        path: "/org/a11y/atspi/accessible/root",
        interface: "org.a11y.atspi.Accessible",
        member: "GetRoleName",
        args: Vec::new(),
        reply: "s",
    })?;
    Ok(reply.map_err(|error| error.name))
}

#[test]
fn the_reply_from_the_called_peer_is_accepted() {
    let (path, server) = fake_bus("genuine", |stream| {
        stream
            .write_all(&message(2, 2, Some(":1.5"), "s", &text("push button")))
            .unwrap();
    });
    let reply = call_app(path).unwrap().unwrap();
    assert_eq!(reply, [Value::Str("push button".into())]);
    server.join().unwrap();
}

#[test]
fn a_signal_carrying_the_reply_serial_is_not_a_reply() {
    let (path, server) = fake_bus("signal", |stream| {
        stream
            .write_all(&message(4, 2, Some(":1.5"), "s", &text("spoofed")))
            .unwrap();
    });
    let error = call_app(path).unwrap_err();
    assert!(format!("{error:#}").contains("no answer"), "{error:#}");
    server.join().unwrap();
}

#[test]
fn a_reply_from_another_application_is_not_the_reply() {
    let (path, server) = fake_bus("impostor", |stream| {
        stream
            .write_all(&message(2, 2, Some(":1.66"), "s", &text("spoofed")))
            .unwrap();
        stream
            .write_all(&message(2, 2, None, "s", &text("spoofed")))
            .unwrap();
    });
    let error = call_app(path).unwrap_err();
    assert!(format!("{error:#}").contains("no answer"), "{error:#}");
    server.join().unwrap();
}

#[test]
fn a_reply_of_the_wrong_type_is_an_error_reply_not_a_value() {
    let (path, server) = fake_bus("mistyped", |stream| {
        // GetRoleName answers a string; this answers a number.
        let bytes = message(2, 2, Some(":1.5"), "u", &7u32.to_le_bytes());
        stream.write_all(&bytes).unwrap();
    });
    let reply = call_app(path).unwrap();
    let name = reply.unwrap_err();
    assert!(name.contains("UnexpectedSignature"), "{name}");
    server.join().unwrap();
}

#[test]
fn a_reply_that_trickles_past_the_timeout_ends_the_call() {
    let (path, server) = fake_bus("trickle", |stream| {
        let reply = message(2, 2, Some(":1.5"), "s", &text(&"x".repeat(40)));
        stream.write_all(&reply[..16]).unwrap();
        for byte in &reply[16..] {
            if stream.write_all(&[*byte]).is_err() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    });
    let started = Instant::now();
    let error = call_app(path).unwrap_err();
    assert!(format!("{error:#}").contains("no answer"), "{error:#}");
    assert!(
        started.elapsed() < TIMEOUT * 3,
        "the timeout bounds the whole call, not each read: {:?}",
        started.elapsed()
    );
    server.join().unwrap();
}
