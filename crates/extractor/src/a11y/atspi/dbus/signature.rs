//! D-Bus type signatures: splitting one into its complete types, and
//! the alignment each type's values take on the wire.

use anyhow::{bail, Context, Result};

/// The specification allows 32 nested arrays plus 32 nested structs.
pub(super) const MAX_DEPTH: usize = 64;

/// Split `signature` into its single complete types.
pub fn split_signature(signature: &str) -> Result<Vec<&str>> {
    let mut out = Vec::new();
    let mut rest = signature;
    while !rest.is_empty() {
        let len = complete_type_len(rest.as_bytes(), 0)
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
pub(super) fn complete_type_len(sig: &[u8], depth: usize) -> Result<usize> {
    if depth > MAX_DEPTH {
        bail!("nested deeper than {MAX_DEPTH}");
    }
    match sig.first() {
        None => bail!("a type is missing"),
        Some(&code) if is_basic(code) || code == b'v' => Ok(1),
        Some(b'a') if sig.get(1) == Some(&b'{') => {
            let key = sig.get(2).copied().filter(|&code| is_basic(code));
            if key.is_none() {
                bail!("a dict entry's key must be a basic type");
            }
            let value = complete_type_len(&sig[3..], depth + 1)?;
            if sig.get(3 + value) != Some(&b'}') {
                bail!("an unterminated dict entry");
            }
            Ok(4 + value)
        }
        Some(b'a') => Ok(1 + complete_type_len(&sig[1..], depth + 1)?),
        Some(b'(') => {
            let mut at = 1;
            while sig.get(at) != Some(&b')') {
                if at >= sig.len() {
                    bail!("an unterminated struct");
                }
                at += complete_type_len(&sig[at..], depth + 1)?;
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
