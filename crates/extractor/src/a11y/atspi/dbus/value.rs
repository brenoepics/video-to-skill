//! D-Bus values and their wire decoding, for any signature a reply can
//! carry. Everything a peer sends is checked against the specification's
//! limits — lengths, nesting, NUL terminators, booleans, UTF-8 — before
//! it is trusted, so a misbehaving application yields an error, never a
//! panic or an unbounded allocation.

use anyhow::{bail, Context, Result};

pub use super::signature::split_signature;
use super::signature::{alignment, complete_type_len, MAX_DEPTH};

/// The specification's largest array, in bytes.
const MAX_ARRAY_BYTES: usize = 1 << 26;

/// One decoded value. `s`, `o` and `g` all decode to [`Value::Str`].
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Byte(u8),
    Bool(bool),
    I16(i16),
    U16(u16),
    I32(i32),
    U32(u32),
    I64(i64),
    U64(u64),
    Double(f64),
    Str(String),
    /// A Unix file descriptor index (`h`); never used, only skipped.
    Fd(u32),
    Array(Vec<Value>),
    Struct(Vec<Value>),
    DictEntry(Box<(Value, Value)>),
    Variant(Box<Value>),
}

impl Value {
    /// This value, or what the variant(s) around it hold.
    #[must_use]
    pub fn inner(&self) -> &Value {
        match self {
            Self::Variant(inner) => inner.inner(),
            other => other,
        }
    }

    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self.inner() {
            Self::Str(text) => Some(text),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_u32(&self) -> Option<u32> {
        match self.inner() {
            Self::U32(n) => Some(*n),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_f64(&self) -> Option<f64> {
        match self.inner() {
            Self::Double(n) => Some(*n),
            _ => None,
        }
    }

    /// The members of an array or a struct.
    #[must_use]
    pub fn items(&self) -> Option<&[Value]> {
        match self.inner() {
            Self::Array(items) | Self::Struct(items) => Some(items),
            _ => None,
        }
    }
}

/// Reads values from a buffer whose offset 0 is 8-aligned in the message.
pub struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
    big_endian: bool,
}

impl<'a> Reader<'a> {
    #[must_use]
    pub fn new(buf: &'a [u8], big_endian: bool) -> Self {
        Self::at(buf, 0, big_endian)
    }

    #[must_use]
    pub fn at(buf: &'a [u8], pos: usize, big_endian: bool) -> Self {
        Self {
            buf,
            pos,
            big_endian,
        }
    }

    #[must_use]
    pub fn position(&self) -> usize {
        self.pos
    }

    /// Every value of `signature`, in order.
    pub fn read_all(&mut self, signature: &str) -> Result<Vec<Value>> {
        split_signature(signature)?
            .into_iter()
            .map(|ty| self.read(ty.as_bytes(), 0))
            .collect()
    }

    fn align(&mut self, to: usize) -> Result<()> {
        let aligned = self.pos.next_multiple_of(to);
        if aligned > self.buf.len() {
            bail!("the message ends inside padding");
        }
        self.pos = aligned;
        Ok(())
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(len)
            .filter(|&end| end <= self.buf.len())
            .context("the message ends inside a value")?;
        let bytes = &self.buf[self.pos..end];
        self.pos = end;
        Ok(bytes)
    }

    fn fixed<const N: usize>(&mut self) -> Result<[u8; N]> {
        self.align(N)?;
        let mut out = [0; N];
        out.copy_from_slice(self.take(N)?);
        if self.big_endian == cfg!(target_endian = "little") {
            out.reverse();
        }
        Ok(out)
    }

    pub fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_ne_bytes(self.fixed()?))
    }

    fn text(&mut self, len: usize) -> Result<String> {
        let bytes = self.take(len)?;
        if self.take(1)? != [0] {
            bail!("a string is not NUL-terminated");
        }
        String::from_utf8(bytes.to_vec()).context("a string is not valid UTF-8")
    }

    fn read(&mut self, ty: &[u8], depth: usize) -> Result<Value> {
        if depth > MAX_DEPTH {
            bail!("a value is nested deeper than {MAX_DEPTH}");
        }
        let Some(&code) = ty.first() else {
            bail!("a type is missing");
        };
        Ok(match code {
            b'y' => Value::Byte(self.take(1)?[0]),
            b'b' => match self.u32()? {
                0 => Value::Bool(false),
                1 => Value::Bool(true),
                other => bail!("a boolean is {other}, not 0 or 1"),
            },
            b'n' => Value::I16(i16::from_ne_bytes(self.fixed()?)),
            b'q' => Value::U16(u16::from_ne_bytes(self.fixed()?)),
            b'i' => Value::I32(i32::from_ne_bytes(self.fixed()?)),
            b'u' => Value::U32(self.u32()?),
            b'h' => Value::Fd(self.u32()?),
            b'x' => Value::I64(i64::from_ne_bytes(self.fixed()?)),
            b't' => Value::U64(u64::from_ne_bytes(self.fixed()?)),
            b'd' => Value::Double(f64::from_ne_bytes(self.fixed()?)),
            b's' | b'o' => {
                let len = usize::try_from(self.u32()?)?;
                Value::Str(self.text(len)?)
            }
            b'g' => {
                let len = usize::from(self.take(1)?[0]);
                Value::Str(self.text(len)?)
            }
            b'v' => {
                let len = usize::from(self.take(1)?[0]);
                let signature = self.text(len)?;
                if split_signature(&signature)?.len() != 1 {
                    bail!("a variant holds {signature:?}, not one complete type");
                }
                Value::Variant(Box::new(self.read(signature.as_bytes(), depth + 1)?))
            }
            b'a' => self.array(&ty[1..], depth)?,
            b'(' => {
                self.align(8)?;
                let mut members = Vec::new();
                let mut at = 1;
                while ty.get(at).is_some_and(|&c| c != b')') {
                    let len = complete_type_len(&ty[at..], depth + 1)?;
                    members.push(self.read(&ty[at..at + len], depth + 1)?);
                    at += len;
                }
                Value::Struct(members)
            }
            other => bail!("cannot read type code {:?} here", char::from(other)),
        })
    }

    fn array(&mut self, element: &[u8], depth: usize) -> Result<Value> {
        let len = usize::try_from(self.u32()?)?;
        if len > MAX_ARRAY_BYTES {
            bail!("an array of {len} bytes exceeds the specification's limit");
        }
        let first = element.first().copied().unwrap_or(0);
        self.align(alignment(first))?;
        let end = self
            .pos
            .checked_add(len)
            .filter(|&end| end <= self.buf.len())
            .context("an array runs past the end of the message")?;
        let mut items = Vec::new();
        if first == b'{' {
            let value_len = complete_type_len(&element[2..], depth + 1)?;
            while self.pos < end {
                self.align(8)?;
                let key = self.read(&element[1..2], depth + 1)?;
                let value = self.read(&element[2..2 + value_len], depth + 1)?;
                items.push(Value::DictEntry(Box::new((key, value))));
            }
        } else {
            let element_len = complete_type_len(element, depth + 1)?;
            while self.pos < end {
                items.push(self.read(&element[..element_len], depth + 1)?);
            }
        }
        if self.pos != end {
            bail!("an array's elements overrun its declared length");
        }
        Ok(Value::Array(items))
    }
}
