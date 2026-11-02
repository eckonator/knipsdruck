// SPDX-License-Identifier: MIT OR Apache-2.0
//! Zufälliger Zugangsschlüssel für die Handyseite und Zufallsports.

use std::io;

/// Base32-Alphabet nach RFC 4648, kleingeschrieben (leichter abzutippen, kompakter QR-Code).
const ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";

/// Base32 (RFC 4648) in Kleinbuchstaben, ohne `=`-Auffüllung.
pub fn base32(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(5) * 8);
    let mut buffer: u32 = 0;
    let mut bits = 0;
    for &b in bytes {
        buffer = (buffer << 8) | u32::from(b);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(ALPHABET[((buffer >> bits) & 31) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(ALPHABET[((buffer << (5 - bits)) & 31) as usize] as char);
    }
    out
}

fn random_error(e: getrandom::Error) -> io::Error {
    io::Error::other(format!("Zufallszahlen nicht verfügbar: {e}"))
}

/// Neuer Schlüssel mit 128 Bit Zufall (26 Zeichen).
pub fn new_token() -> io::Result<String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(random_error)?;
    Ok(base32(&bytes))
}

/// Zufälliger Port aus dem dynamischen Bereich 49152–65535.
pub fn random_port() -> io::Result<u16> {
    let r = getrandom::u32().map_err(random_error)?;
    Ok(49152 + (r % 16384) as u16)
}

/// Vergleich in konstanter Zeit, damit die Antwortzeit nichts über den Schlüssel verrät.
pub fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc4648_vectors() {
        let cases = [
            ("", ""),
            ("f", "my"),
            ("fo", "mzxq"),
            ("foo", "mzxw6"),
            ("foob", "mzxw6yq"),
            ("fooba", "mzxw6ytb"),
            ("foobar", "mzxw6ytboi"),
        ];
        for (input, expected) in cases {
            assert_eq!(base32(input.as_bytes()), expected, "{input}");
        }
        assert_eq!(base32(&[0xff; 5]), "77777777");
        assert_eq!(base32(&[0; 2]), "aaaa");
    }

    #[test]
    fn token_shape() {
        let a = new_token().unwrap();
        let b = new_token().unwrap();
        assert_eq!(a.len(), 26);
        assert!(a.bytes().all(|c| ALPHABET.contains(&c)));
        assert_ne!(a, b);
    }

    #[test]
    fn ports_in_range() {
        for _ in 0..100 {
            assert!(random_port().unwrap() >= 49152);
        }
    }

    #[test]
    fn compare() {
        assert!(same("abc", "abc"));
        assert!(!same("abc", "abd"));
        assert!(!same("abc", "abcd"));
    }
}
