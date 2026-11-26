//! What a hostile peer can send: oversized frames, amplifying arrays,
//! mistyped header fields, over-deep signatures, and random bytes.

use super::message::{decode, frame_length};
use super::value::{split_signature, Reader, Value, MAX_VALUES};

/// A `METHOD_RETURN` with `REPLY_SERIAL` 1, a `SIGNATURE` field typed
/// `sig_type` (always `g` on the wire from a real bus) and an optional
/// `SENDER`.
fn message(sig_type: u8, signature: &str, sender: Option<&str>, body: &[u8]) -> Vec<u8> {
    let pad = |bytes: &mut Vec<u8>| {
        while !bytes.len().is_multiple_of(8) {
            bytes.push(0);
        }
    };
    let len = |n: usize| u32::try_from(n).unwrap().to_le_bytes();
    let mut fields = vec![5, 1, b'u', 0, 1, 0, 0, 0];
    if let Some(sender) = sender {
        fields.extend([7, 1, b's', 0]);
        fields.extend(len(sender.len()));
        fields.extend(sender.as_bytes());
        fields.push(0);
        pad(&mut fields);
    }
    fields.extend([8, 1, sig_type, 0]);
    if sig_type == b'g' {
        fields.push(u8::try_from(signature.len()).unwrap());
    } else {
        fields.extend(len(signature.len()));
    }
    fields.extend(signature.as_bytes());
    fields.push(0);
    let mut out = vec![b'l', 2, 0, 1];
    out.extend(len(body.len()));
    out.extend(len(9));
    out.extend(len(fields.len()));
    out.extend(&fields);
    pad(&mut out);
    out.extend(body);
    out
}

#[test]
fn a_byte_array_decodes_as_bytes_not_one_value_per_byte() {
    let mut body = 3u32.to_le_bytes().to_vec();
    body.extend(b"abc");
    let decoded = decode(&message(b'g', "ay", None, &body)).unwrap();
    assert_eq!(decoded.body, [Value::Bytes(b"abc".to_vec())]);
}

#[test]
fn frames_are_capped_near_the_largest_real_reply() {
    let header = |body: u32| {
        let mut fixed = vec![b'l', 2, 0, 1];
        fixed.extend(body.to_le_bytes());
        fixed.extend(1u32.to_le_bytes());
        fixed.extend(8u32.to_le_bytes());
        fixed
    };
    assert!(
        frame_length(&header(1 << 20)).is_ok(),
        "1 MiB is a big GetChildren"
    );
    assert!(
        frame_length(&header(8 << 20)).is_err(),
        "8 MiB is no real reply"
    );
}

#[test]
fn one_message_decodes_into_a_bounded_number_of_values() {
    let count = MAX_VALUES + 1;
    let mut body = u32::try_from(count * 4).unwrap().to_le_bytes().to_vec();
    body.resize(4 + count * 4, 0);
    let error = Reader::new(&body, false).read_all("au").unwrap_err();
    assert!(format!("{error:#}").contains("values"), "{error:#}");
}

#[test]
fn the_sender_is_decoded_and_a_signature_field_must_be_typed_g() {
    let mut body = 2u32.to_le_bytes().to_vec();
    body.extend(b"ok\0");
    let decoded = decode(&message(b'g', "s", Some(":1.42"), &body)).unwrap();
    assert_eq!(decoded.sender.as_deref(), Some(":1.42"));

    let error = decode(&message(b's', "s", None, &body)).unwrap_err();
    assert!(format!("{error:#}").contains("SIGNATURE"), "{error:#}");
}

#[test]
fn signatures_allow_32_arrays_and_32_structs_but_no_more() {
    let arrays = |n: usize| format!("{}y", "a".repeat(n));
    let structs = |n: usize| format!("{}y{}", "(".repeat(n), ")".repeat(n));
    assert!(split_signature(&arrays(32)).is_ok());
    assert!(split_signature(&arrays(33)).is_err());
    assert!(split_signature(&structs(32)).is_ok());
    assert!(split_signature(&structs(33)).is_err());
    let both = format!("{}{}", "a".repeat(32), structs(32));
    assert!(split_signature(&both).is_ok(), "the limits are separate");
    // Dict entries count as structs.
    let dicts = format!("{}y{}", "a{s".repeat(33), "}".repeat(33));
    assert!(split_signature(&dicts).is_err());
    assert!(
        split_signature(&"y".repeat(256)).is_err(),
        "longer than 255 bytes"
    );
}

/// A deterministic xorshift, so the fuzz run is the same every time.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % u64::try_from(n).unwrap()).unwrap()
    }
}

#[test]
fn random_and_mutated_messages_are_refused_without_panicking() {
    const ALPHABET: &[u8] = b"ybnqiuxtdsoghva(){}";
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let mut body = 2u32.to_le_bytes().to_vec();
    body.extend(b"ok\0");
    let seeds = [
        message(b'g', "s", Some(":1.4"), &body),
        message(b'g', "a{sv}", None, &[8, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0]),
    ];
    for round in 0..20_000 {
        let mut bytes = seeds[round % seeds.len()].clone();
        for _ in 0..=rng.below(6) {
            if bytes.is_empty() {
                break;
            }
            let at = rng.below(bytes.len());
            match rng.below(3) {
                0 => bytes[at] = u8::try_from(rng.next() & 0xff).unwrap(),
                1 => bytes.truncate(at),
                _ => bytes[at] = ALPHABET[rng.below(ALPHABET.len())],
            }
        }
        let _ = frame_length(&bytes);
        let _ = decode(&bytes);
        let signature: String = (0..rng.below(12))
            .map(|_| char::from(ALPHABET[rng.below(ALPHABET.len())]))
            .collect();
        let _ = Reader::new(&bytes, rng.below(2) == 0).read_all(&signature);
    }
}
