//! D-Bus messages: encoding the method calls this client makes and
//! decoding whatever the bus sends back.
//!
//! Only what a read-only accessibility client needs is encoded: method
//! calls whose arguments are strings and 32-bit integers. Every call sets
//! `NO_AUTO_START`, so asking the bus never D-Bus-activates a service to
//! answer it.

use anyhow::{bail, Context, Result};

use super::value::{Reader, Value};

/// Header flag: do not start the destination service to answer.
pub const NO_AUTO_START: u8 = 0x2;
/// The largest reply this client accepts, refused before it is read. Text
/// is asked for 16 384 characters at most (64 KiB of UTF-8), so only
/// `GetChildren` of a very large container comes near: 4 MiB holds some
/// 50 000 children.
const MAX_MESSAGE_BYTES: usize = 1 << 22;
/// The fixed part every message starts with.
pub const FIXED_HEADER_BYTES: usize = 16;

const FIELD_PATH: u8 = 1;
const FIELD_INTERFACE: u8 = 2;
const FIELD_MEMBER: u8 = 3;
const FIELD_ERROR_NAME: u8 = 4;
const FIELD_REPLY_SERIAL: u8 = 5;
const FIELD_DESTINATION: u8 = 6;
const FIELD_SENDER: u8 = 7;
const FIELD_SIGNATURE: u8 = 8;

/// One method call.
#[derive(Debug, Clone)]
pub struct Call<'a> {
    pub destination: &'a str,
    pub path: &'a str,
    pub interface: &'a str,
    pub member: &'a str,
    pub args: Vec<Arg<'a>>,
    /// The signature the reply must have; any other is not decoded.
    pub reply: &'a str,
}

/// A call argument.
#[derive(Debug, Clone, Copy)]
pub enum Arg<'a> {
    Str(&'a str),
    I32(i32),
    U32(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    MethodCall,
    MethodReturn,
    Error,
    Signal,
}

/// The header fields a client acts on, and where the body starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub kind: Kind,
    pub serial: u32,
    pub reply_serial: Option<u32>,
    pub error_name: Option<String>,
    /// Set by the bus daemon on every message it routes; a peer cannot
    /// forge it.
    pub sender: Option<String>,
    pub signature: String,
    body_start: usize,
    big_endian: bool,
}

/// A decoded message: its header fields and its body.
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    pub kind: Kind,
    pub serial: u32,
    pub reply_serial: Option<u32>,
    pub error_name: Option<String>,
    pub sender: Option<String>,
    pub signature: String,
    pub body: Vec<Value>,
}

#[derive(Default)]
struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    fn pad(&mut self, to: usize) {
        let aligned = self.buf.len().next_multiple_of(to);
        self.buf.resize(aligned, 0);
    }

    fn u32(&mut self, n: u32) {
        self.pad(4);
        self.buf.extend(n.to_le_bytes());
    }

    fn string(&mut self, text: &str) -> Result<()> {
        if text.contains('\0') {
            bail!("a D-Bus string cannot contain NUL: {text:?}");
        }
        self.u32(u32::try_from(text.len()).context("a string is too long for D-Bus")?);
        self.buf.extend(text.as_bytes());
        self.buf.push(0);
        Ok(())
    }

    fn signature(&mut self, signature: &str) -> Result<()> {
        self.buf
            .push(u8::try_from(signature.len()).context("a signature is too long")?);
        self.buf.extend(signature.as_bytes());
        self.buf.push(0);
        Ok(())
    }

    /// One `(yv)` header field holding a string-like value.
    fn field(&mut self, code: u8, type_code: char, text: &str) -> Result<()> {
        self.pad(8);
        self.buf.push(code);
        self.signature(type_code.encode_utf8(&mut [0; 4]))?;
        if type_code == 'g' {
            self.signature(text)
        } else {
            self.string(text)
        }
    }
}

/// Encode `call` as a little-endian `METHOD_CALL` with `serial`.
pub fn encode_call(call: &Call<'_>, serial: u32) -> Result<Vec<u8>> {
    let mut body = Writer::default();
    let mut signature = String::new();
    for arg in &call.args {
        match *arg {
            Arg::Str(text) => {
                signature.push('s');
                body.string(text)?;
            }
            Arg::I32(n) => {
                signature.push('i');
                body.pad(4);
                body.buf.extend(n.to_le_bytes());
            }
            Arg::U32(n) => {
                signature.push('u');
                body.u32(n);
            }
        }
    }
    if !call.path.starts_with('/') {
        bail!("{:?} is not an object path", call.path);
    }
    let mut out = Writer::default();
    out.buf.extend([b'l', 1, NO_AUTO_START, 1]);
    out.u32(u32::try_from(body.buf.len()).context("the call is too large")?);
    out.u32(serial);
    out.u32(0); // the header fields' length, patched below
    out.field(FIELD_PATH, 'o', call.path)?;
    out.field(FIELD_INTERFACE, 's', call.interface)?;
    out.field(FIELD_MEMBER, 's', call.member)?;
    out.field(FIELD_DESTINATION, 's', call.destination)?;
    if !signature.is_empty() {
        out.field(FIELD_SIGNATURE, 'g', &signature)?;
    }
    let fields = u32::try_from(out.buf.len() - FIXED_HEADER_BYTES)?;
    out.buf[12..16].copy_from_slice(&fields.to_le_bytes());
    out.pad(8);
    out.buf.extend(body.buf);
    Ok(out.buf)
}

fn big_endian(flag: u8) -> Result<bool> {
    match flag {
        b'l' => Ok(false),
        b'B' => Ok(true),
        other => bail!("not a D-Bus message (endianness byte {other:#04x})"),
    }
}

/// The whole message's length, from its first 16 bytes.
pub fn frame_length(fixed: &[u8]) -> Result<usize> {
    if fixed.len() < FIXED_HEADER_BYTES {
        bail!("a D-Bus header is {FIXED_HEADER_BYTES} bytes");
    }
    let mut reader = Reader::at(fixed, 4, big_endian(fixed[0])?);
    let body = usize::try_from(reader.u32()?)?;
    reader.u32()?;
    let fields = usize::try_from(reader.u32()?)?;
    let total = fields
        .checked_add(FIXED_HEADER_BYTES)
        .map(|header| header.next_multiple_of(8))
        .and_then(|header| header.checked_add(body))
        .filter(|&total| total <= MAX_MESSAGE_BYTES);
    total.with_context(|| format!("a D-Bus message larger than {MAX_MESSAGE_BYTES} bytes"))
}

/// Decode one complete message.
pub fn decode(bytes: &[u8]) -> Result<Message> {
    let header = decode_header(bytes)?;
    let body = header.body(bytes)?;
    Ok(Message {
        kind: header.kind,
        serial: header.serial,
        reply_serial: header.reply_serial,
        error_name: header.error_name,
        sender: header.sender,
        signature: header.signature,
        body,
    })
}

/// Decode a complete message's header only, so a client can decide
/// whether the body is worth decoding at all.
pub fn decode_header(bytes: &[u8]) -> Result<Header> {
    if frame_length(bytes)? != bytes.len() {
        bail!("a D-Bus message's length does not match its header");
    }
    let big = big_endian(bytes[0])?;
    let kind = match bytes[1] {
        1 => Kind::MethodCall,
        2 => Kind::MethodReturn,
        3 => Kind::Error,
        4 => Kind::Signal,
        other => bail!("unknown D-Bus message type {other}"),
    };
    if bytes[3] != 1 {
        bail!("unsupported D-Bus protocol version {}", bytes[3]);
    }
    let mut reader = Reader::at(bytes, 8, big);
    let serial = reader.u32()?;
    let fields = reader.read_all("a(yv)")?;
    let mut header = Header {
        kind,
        serial,
        reply_serial: None,
        error_name: None,
        sender: None,
        signature: String::new(),
        body_start: reader.position().next_multiple_of(8),
        big_endian: big,
    };
    for field in fields.first().and_then(Value::items).unwrap_or_default() {
        let (Some(Value::Byte(code)), Some(value)) = (
            field.items().and_then(|f| f.first()),
            field.items().and_then(|f| f.get(1)),
        ) else {
            bail!("a malformed D-Bus header field");
        };
        let text = || value.as_str().map(str::to_string);
        match (*code, value.inner()) {
            (FIELD_REPLY_SERIAL, Value::U32(n)) => header.reply_serial = Some(*n),
            (FIELD_ERROR_NAME, Value::Str(_)) => header.error_name = text(),
            (FIELD_SENDER, Value::Str(_)) => header.sender = text(),
            (FIELD_SIGNATURE, Value::Signature(signature)) => {
                header.signature.clone_from(signature);
            }
            (FIELD_REPLY_SERIAL | FIELD_ERROR_NAME | FIELD_SENDER | FIELD_SIGNATURE, _) => {
                bail!("a D-Bus header field {code} has the wrong type (SIGNATURE must be `g`)")
            }
            _ => {}
        }
    }
    Ok(header)
}

impl Header {
    /// Decode the body of `bytes`, the message this header came from.
    pub fn body(&self, bytes: &[u8]) -> Result<Vec<Value>> {
        let body = bytes
            .get(self.body_start..)
            .context("a D-Bus message ends inside its header")?;
        let mut reader = Reader::new(body, self.big_endian);
        let values = reader.read_all(&self.signature)?;
        if reader.position() != body.len() {
            bail!("a D-Bus message's body is longer than its signature says");
        }
        Ok(values)
    }
}
