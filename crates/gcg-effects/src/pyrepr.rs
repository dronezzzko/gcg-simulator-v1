//! CPython `repr` of a `str` and the content id of a `repr` text (docs/dev/PLAN.md §5.7;
//! open item O2 of docs/dev/contracts.md).
//!
//! `gcg_core::pyrepr::repr_str` accepts ASCII only. The effect IR also holds printable
//! non-ASCII text (`Ⅱ`, `①`, ...), which CPython's `unicode_repr` copies unchanged. Whether a
//! non-ASCII character is printable depends on the Unicode database, so the IR loader admits
//! only the characters of the IR's `printable_non_ascii` table (each checked with
//! `str.isprintable()` by `oracle/export_ir.py`); every string that reaches this printer
//! has passed that check.

use std::fmt::Write as _;

use sha2::{Digest, Sha256};

use crate::contracts::ContentId;

/// `repr(s)`: single quotes unless `s` contains `'` and no `"`; `\\`, `\t`, `\n`, `\r` and the
/// quote are escaped, other ASCII control characters print as `\xNN`, and printable text
/// (including the admitted non-ASCII characters) is copied (CPython `unicode_repr`).
#[must_use]
pub(crate) fn repr_str(s: &str) -> String {
    let quote = if s.contains('\'') && !s.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::with_capacity(s.len() + 2);
    out.push(quote);
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if u32::from(c) < 0x20 || c == '\u{7f}' => {
                let _ = write!(out, "\\x{:02x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

/// `hashlib.sha256(text.encode()).hexdigest()[:16]` (`runner/digest.py:26-27`).
#[must_use]
pub(crate) fn content_id_of(text: &str) -> ContentId {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = Sha256::digest(text.as_bytes());
    let mut out = [0u8; 16];
    for (i, b) in digest.iter().take(8).enumerate() {
        out[2 * i] = HEX[usize::from(b >> 4)];
        out[2 * i + 1] = HEX[usize::from(b & 0x0f)];
    }
    ContentId(out)
}

impl ContentId {
    /// The 16 lowercase hex characters.
    #[must_use]
    pub fn as_str(&self) -> &str {
        // Only `from_hex` and `content_id_of` build a ContentId, both from ASCII hex digits.
        std::str::from_utf8(&self.0).unwrap_or_default()
    }

    /// Parses 16 lowercase hex characters (the form `runner/digest.py:26-27` writes).
    #[must_use]
    pub fn from_hex(s: &str) -> Option<Self> {
        let bytes: [u8; 16] = s.as_bytes().try_into().ok()?;
        bytes
            .iter()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
            .then_some(Self(bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_and_escapes_match_cpython() {
        assert_eq!(repr_str("it's"), "\"it's\"");
        assert_eq!(repr_str("say \"hi\""), "'say \"hi\"'");
        assert_eq!(repr_str("both ' and \""), "'both \\' and \"'");
        assert_eq!(repr_str("Zaku Ⅱ ①"), "'Zaku Ⅱ ①'");
        assert_eq!(
            repr_str("a\\b\tc\nd\re\u{0}f\u{7f}"),
            "'a\\\\b\\tc\\nd\\re\\x00f\\x7f'"
        );
        assert_eq!(repr_str(""), "''");
    }

    #[test]
    fn content_ids_are_the_first_16_hex_digits() {
        // repr of keyword:Repair.
        let text = "Program(instrs=(Recover(ref=This(), amount=KwAmount(keyword=<Kw.REPAIR: \
                    'Repair'>, ref=This())),), label='keyword:Repair')";
        assert_eq!(content_id_of(text).as_str(), "b3df0ca881a27647");
        let id = ContentId::from_hex("b3df0ca881a27647").expect("valid hex");
        assert_eq!(id, content_id_of(text));
        assert_eq!(ContentId::from_hex("B3DF0CA881A27647"), None);
        assert_eq!(ContentId::from_hex("b3df0ca881a2764"), None);
    }
}
