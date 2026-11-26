//! D-Bus type signatures: splitting one into its complete types, and
//! the alignment each type's values take on the wire.

use anyhow::{bail, Context, Result};

/// The specification's nesting limits: 32 arrays and, separately, 32
/// structs (dict entries count as structs).
const MAX_ARRAYS: usize = 32;
const MAX_STRUCTS: usize = 32;
/// A value may additionally sit inside variants; the specification caps
/// the whole message at 64 levels.
pub(super) const MAX_DEPTH: usize = 64;
/// A signature's length is one byte on the wire.
const MAX_SIGNATURE_BYTES: usize = 255;

/// How deep the type being parsed already sits.
#[derive(Clone, Copy, Default)]
pub(super) struct Nesting {
    arrays: usize,
    structs: usize,
}

impl Nesting {
    fn array(self) -> Result<Self> {
        if self.arrays >= MAX_ARRAYS {
            bail!("more than {MAX_ARRAYS} nested arrays");
        }
        Ok(Self {
            arrays: self.arrays + 1,
            ..self
        })
    }

    fn structure(self) -> Result<Self> {
        if self.structs >= MAX_STRUCTS {
            bail!("more than {MAX_STRUCTS} nested structs");
        }
        Ok(Self {
            structs: self.structs + 1,
            ..self
        })
    }
}

/// Split `signature` into its single complete types.
pub fn split_signature(signature: &str) -> Result<Vec<&str>> {
    if signature.len() > MAX_SIGNATURE_BYTES {
        bail!("a D-Bus signature is longer than {MAX_SIGNATURE_BYTES} bytes");
    }
    let mut out = Vec::new();
    let mut rest = signature;
    while !rest.is_empty() {
        let len = complete_type_len(rest.as_bytes(), Nesting::default())
            .with_context(|| format!("invalid D-Bus signature {signature:?}"))?;
        let (head, tail) = rest.split_at(len);
        out.push(head);
        rest = tail;
    }
    Ok(out)
}

fn is_basic(code: u8) -> bool {
    b"ybnqiuxtdsogh".contains(&code)
}

/// The length of the complete type at the start of `sig`.
pub(super) fn complete_type_len(sig: &[u8], nesting: Nesting) -> Result<usize> {
    match sig.first() {
        None => bail!("a type is missing"),
        Some(&code) if is_basic(code) || code == b'v' => Ok(1),
        Some(b'a') if sig.get(1) == Some(&b'{') => {
            let inner = nesting.array()?.structure()?;
            let key = sig.get(2).copied().filter(|&code| is_basic(code));
            if key.is_none() {
                bail!("a dict entry's key must be a basic type");
            }
            let value = complete_type_len(&sig[3..], inner)?;
            if sig.get(3 + value) != Some(&b'}') {
                bail!("an unterminated dict entry");
            }
            Ok(4 + value)
        }
        Some(b'a') => Ok(1 + complete_type_len(&sig[1..], nesting.array()?)?),
        Some(b'(') => {
            let inner = nesting.structure()?;
            let mut at = 1;
            while sig.get(at) != Some(&b')') {
                if at >= sig.len() {
                    bail!("an unterminated struct");
                }
                at += complete_type_len(&sig[at..], inner)?;
            }
            if at == 1 {
                bail!("an empty struct");
            }
            Ok(at + 1)
        }
        Some(&other) => bail!("unknown type code {:?}", char::from(other)),
    }
}

pub(super) fn alignment(code: u8) -> usize {
    match code {
        b'n' | b'q' => 2,
        b'b' | b'i' | b'u' | b'h' | b's' | b'o' | b'a' => 4,
        b'x' | b't' | b'd' | b'(' | b'{' => 8,
        _ => 1,
    }
}
