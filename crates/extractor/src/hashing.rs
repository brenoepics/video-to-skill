//! SHA-256 helpers. sha2 0.11 dropped the `std::io::Write` impl on
//! hashers and `LowerHex` on digests, so hashing and hex-encoding live
//! here once instead of being reimplemented per call site.

use std::io::Read;
use std::path::Path;

use anyhow::Result;
use sha2::{Digest, Sha256};

/// Hex SHA-256 of a byte slice.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    to_hex(Sha256::digest(bytes).as_slice())
}

/// Hex SHA-256 of a file, streamed.
pub fn sha256_hex_of_file(path: &Path) -> Result<String> {
    let mut hasher = Sha256::new();
    let mut file = std::fs::File::open(path)?;
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(to_hex(hasher.finalize().as_slice()))
}

fn to_hex(digest: &[u8]) -> String {
    use std::fmt::Write;
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        // Writing to a String cannot fail.
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

#[cfg(test)]
mod tests {
    use super::sha256_hex;

    #[test]
    fn matches_a_known_digest() {
        // printf 'abc' | shasum -a 256
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
