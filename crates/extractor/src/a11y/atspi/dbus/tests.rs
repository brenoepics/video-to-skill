use super::address::{unix_targets, UnixTarget};
use super::message::{decode, encode_call, frame_length, Arg, Call, Kind, NO_AUTO_START};
use super::value::{split_signature, Reader, Value};

fn hello() -> Call<'static> {
    Call {
        destination: "org.freedesktop.DBus",
        path: "/org/freedesktop/DBus",
        interface: "org.freedesktop.DBus",
        member: "Hello",
        args: Vec::new(),
    }
}

#[test]
fn a_call_is_laid_out_as_the_specification_says() {
    let bytes = encode_call(&hello(), 1).unwrap();

    // Little endian, METHOD_CALL, NO_AUTO_START, version 1, empty body,
    // serial 1, then 109 bytes of header fields padded to 128.
    assert_eq!(
        &bytes[..16],
        &[
            b'l',
            1,
            NO_AUTO_START,
            1,
            0,
            0,
            0,
            0,
            1,
            0,
            0,
            0,
            109,
            0,
            0,
            0
        ]
    );
    assert_eq!(bytes.len(), 128);
    // PATH (1) holds an object path ('o') of 21 bytes.
    assert_eq!(&bytes[16..24], &[1, 1, b'o', 0, 21, 0, 0, 0]);
    assert_eq!(&bytes[24..45], b"/org/freedesktop/DBus");
    assert_eq!(frame_length(&bytes[..16]).unwrap(), 128);
}

#[test]
fn a_call_with_arguments_carries_their_signature_and_aligned_body() {
    let call = Call {
        destination: ":1.7",
        path: "/org/a11y/atspi/accessible/3",
        interface: "org.a11y.atspi.Text",
        member: "GetText",
        args: vec![Arg::Str("x"), Arg::I32(0), Arg::U32(7)],
    };

    let bytes = encode_call(&call, 9).unwrap();

    let message = decode(&bytes).unwrap();
    assert_eq!(message.kind, Kind::MethodCall);
    assert_eq!(message.serial, 9);
    assert_eq!(message.signature, "siu");
    assert_eq!(
        message.body,
        [Value::Str("x".into()), Value::I32(0), Value::U32(7)]
    );
    assert_eq!(bytes.len() % 8, 0, "the body follows an 8-aligned header");
}

#[test]
fn strings_with_a_nul_are_refused_rather_than_truncated() {
    let mut call = hello();
    call.args = vec![Arg::Str("a\0b")];
    assert!(encode_call(&call, 1).is_err());
}

/// A `METHOD_RETURN` (or `ERROR`) built the way a bus would send it, in either
/// byte order, so replies are decoded without a bus.
fn reply(
    big_endian: bool,
    kind: u8,
    error_name: Option<&str>,
    signature: &str,
    body: &[u8],
) -> Vec<u8> {
    let u32_bytes = |n: usize| {
        let n = u32::try_from(n).unwrap();
        if big_endian {
            n.to_be_bytes()
        } else {
            n.to_le_bytes()
        }
    };
    let pad = |bytes: &mut Vec<u8>| {
        while !bytes.len().is_multiple_of(8) {
            bytes.push(0);
        }
    };
    // Fields start at offset 16, so 8-alignment inside them is absolute.
    let mut fields = vec![5, 1, b'u', 0];
    fields.extend(u32_bytes(3));
    if let Some(name) = error_name {
        pad(&mut fields);
        fields.extend([4, 1, b's', 0]);
        fields.extend(u32_bytes(name.len()));
        fields.extend(name.as_bytes());
        fields.push(0);
    }
    pad(&mut fields);
    fields.extend([8, 1, b'g', 0, u8::try_from(signature.len()).unwrap()]);
    fields.extend(signature.as_bytes());
    fields.push(0);
    let mut out = vec![if big_endian { b'B' } else { b'l' }, kind, 0, 1];
    out.extend(u32_bytes(body.len()));
    out.extend(u32_bytes(5));
    out.extend(u32_bytes(fields.len()));
    out.extend(&fields);
    pad(&mut out);
    out.extend(body);
    out
}

#[test]
fn replies_decode_in_both_byte_orders() {
    // GetChildren → a(so): one (":1.4", "/root") entry.
    let mut le = vec![22, 0, 0, 0, 0, 0, 0, 0, 4, 0, 0, 0];
    le.extend(b":1.4\0\0\0\0");
    le.extend([5, 0, 0, 0]);
    le.extend(b"/root\0");
    let mut be = vec![0, 0, 0, 22, 0, 0, 0, 0, 0, 0, 0, 4];
    be.extend(b":1.4\0\0\0\0");
    be.extend([0, 0, 0, 5]);
    be.extend(b"/root\0");

    for (big, body) in [(false, le), (true, be)] {
        let bytes = reply(big, 2, None, "a(so)", &body);
        assert_eq!(frame_length(&bytes[..16]).unwrap(), bytes.len());
        let message = decode(&bytes).unwrap();
        assert_eq!(message.kind, Kind::MethodReturn);
        assert_eq!(message.reply_serial, Some(3));
        let expected = Value::Array(vec![Value::Struct(vec![
            Value::Str(":1.4".into()),
            Value::Str("/root".into()),
        ])]);
        assert_eq!(message.body, [expected], "big endian: {big}");
    }
}

#[test]
fn an_error_reply_carries_its_name_and_message() {
    let mut body = vec![5, 0, 0, 0];
    body.extend(b"gone!\0");
    let name = "org.freedesktop.DBus.Error.ServiceUnknown";
    let mut bytes = reply(false, 3, Some(name), "s", &body);

    let message = decode(&bytes).unwrap();

    assert_eq!(message.kind, Kind::Error);
    assert_eq!(message.reply_serial, Some(3));
    assert_eq!(message.error_name.as_deref(), Some(name));
    assert_eq!(message.body[0].as_str(), Some("gone!"));
    bytes.truncate(bytes.len() - 1);
    assert!(decode(&bytes).is_err(), "a short body is refused");
}

#[test]
fn variants_dicts_and_state_words_decode() {
    // a{ss} {"id": "save"} and v<s "File"> and au [4, 1].
    let mut body = vec![17, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0];
    body.extend(b"id\0\0");
    body.extend([4, 0, 0, 0]);
    body.extend(b"save\0");
    let mut reader = Reader::new(&body, false);
    let dict = reader.read_all("a{ss}").unwrap();
    assert_eq!(
        dict,
        [Value::Array(vec![Value::DictEntry(Box::new((
            Value::Str("id".into()),
            Value::Str("save".into())
        )))])]
    );

    let variant = [1, b's', 0, 0, 4, 0, 0, 0, b'F', b'i', b'l', b'e', 0];
    let value = Reader::new(&variant, false).read_all("v").unwrap();
    assert_eq!(value[0].as_str(), Some("File"));

    let words = [8, 0, 0, 0, 4, 0, 0, 0, 1, 0, 0, 0];
    let value = Reader::new(&words, false).read_all("au").unwrap();
    assert_eq!(
        value[0].items().unwrap(),
        [Value::U32(4), Value::U32(1)].as_slice()
    );

    let double = 2.5f64.to_le_bytes();
    let value = Reader::new(&double, false).read_all("d").unwrap();
    assert_eq!(value[0].as_f64(), Some(2.5));
}

#[test]
fn malformed_input_is_refused_not_trusted() {
    // An array claiming more bytes than the message has.
    assert!(Reader::new(&[255, 255, 0, 0], false)
        .read_all("as")
        .is_err());
    // A string without its terminating NUL.
    assert!(Reader::new(&[1, 0, 0, 0, b'a', b'b'], false)
        .read_all("s")
        .is_err());
    // A boolean that is neither 0 nor 1.
    assert!(Reader::new(&[2, 0, 0, 0], false).read_all("b").is_err());
    // Invalid UTF-8.
    assert!(Reader::new(&[1, 0, 0, 0, 0xff, 0], false)
        .read_all("s")
        .is_err());
    // Nesting past the specification's limit.
    let deep = format!("{}i", "a".repeat(70));
    assert!(split_signature(&deep).is_err());
    // Trailing bytes after the declared body are refused by decode.
    let mut bytes = reply(false, 2, None, "", &[]);
    bytes.extend([0; 8]);
    assert!(decode(&bytes).is_err());
    // A frame larger than any reply this client asks for.
    let mut huge = reply(false, 2, None, "", &[]);
    huge[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(frame_length(&huge[..16]).is_err());
}

#[test]
fn signatures_split_into_complete_types() {
    assert_eq!(
        split_signature("sa(so)a{sv}vau").unwrap(),
        ["s", "a(so)", "a{sv}", "v", "au"]
    );
    for bad in ["(", "a", "a{vs}", "()", "{ss}", "z"] {
        assert!(split_signature(bad).is_err(), "{bad}");
    }
}

#[test]
fn addresses_yield_unix_sockets_in_order() {
    let targets =
        unix_targets("tcp:host=127.0.0.1,port=1;unix:path=/run/user/1000/at-spi/bus_0,guid=ab;unix:abstract=/tmp/dbus-x%2cy")
            .unwrap();
    assert_eq!(
        targets,
        [
            UnixTarget::Path("/run/user/1000/at-spi/bus_0".into()),
            UnixTarget::Abstract(b"/tmp/dbus-x,y".to_vec()),
        ]
    );
    assert!(unix_targets("tcp:host=localhost,port=4").is_err());
    assert!(unix_targets("unix:path=/a%zz").is_err());
    assert!(unix_targets("").is_err());
}
