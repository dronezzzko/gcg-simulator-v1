//! Python `str` values of the command line: sequences of code points, lone surrogates
//! included.
//!
//! CPython 3.15 runs in UTF-8 mode (PEP 686): it decodes `argv` and the environment as UTF-8
//! with `surrogateescape`, so every undecodable byte `b` becomes the lone surrogate
//! `U+DC00 + b`; it writes stdout with `surrogateescape` (the original bytes come back) and
//! stderr with `backslashreplace` (a surrogate prints as `\udcff`). Rust `String` cannot hold
//! a lone surrogate, so arguments are kept as code points. Checked with the transcripts of
//! `port/oracle/export_cli.py` (the undecodable-byte cases).

use std::ffi::{OsStr, OsString};
use std::fmt::Write as _;

/// A Python `str`: Unicode scalar values and lone surrogates (`0xD800..=0xDFFF`).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct PyStr(Vec<u32>);

const SURROGATES: std::ops::RangeInclusive<u32> = 0xD800..=0xDFFF;
const ESCAPED_BYTES: std::ops::RangeInclusive<u32> = 0xDC80..=0xDCFF;

impl PyStr {
    #[must_use]
    pub const fn new() -> Self {
        Self(Vec::new())
    }

    /// The code points of a Rust string.
    #[must_use]
    pub fn from_text(s: &str) -> Self {
        Self(s.chars().map(u32::from).collect())
    }

    /// Code points as they are; `cps` may hold lone surrogates.
    #[must_use]
    pub const fn from_code_points(cps: Vec<u32>) -> Self {
        Self(cps)
    }

    /// `os.fsdecode(bytes)` in UTF-8 mode: UTF-8 with `surrogateescape`, one surrogate per
    /// undecodable byte (CPython's `Py_DecodeLocale` / `_Py_DecodeUTF8Ex`).
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let mut out = Vec::with_capacity(bytes.len());
        let mut rest = bytes;
        loop {
            match std::str::from_utf8(rest) {
                Ok(valid) => {
                    out.extend(valid.chars().map(u32::from));
                    return Self(out);
                }
                Err(e) => {
                    let (valid, tail) = rest.split_at(e.valid_up_to());
                    // `valid` is UTF-8 by the error's contract.
                    out.extend(String::from_utf8_lossy(valid).chars().map(u32::from));
                    let bad = e.error_len().unwrap_or(tail.len());
                    out.extend(tail[..bad].iter().map(|&b| 0xDC00 + u32::from(b)));
                    rest = &tail[bad..];
                }
            }
        }
    }

    /// An `argv` entry or environment value as Python sees it.
    #[must_use]
    pub fn from_os(s: &OsStr) -> Self {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt as _;
            Self::from_bytes(s.as_bytes())
        }
        #[cfg(not(unix))]
        {
            Self::from_text(&s.to_string_lossy())
        }
    }

    /// `os.fsencode(self)`: UTF-8 with `surrogateescape`. A lone surrogate outside
    /// `U+DC80..=U+DCFF` (never produced by [`PyStr::from_bytes`]) is written as `\uXXXX`.
    #[must_use]
    pub fn to_surrogateescape_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.0.len());
        for &cp in &self.0 {
            if ESCAPED_BYTES.contains(&cp) {
                out.push(u8::try_from(cp - 0xDC00).unwrap_or(b'?'));
            } else {
                push_utf8_or_escape(&mut out, cp);
            }
        }
        out
    }

    /// The bytes Python's stderr writes for this text (UTF-8, `backslashreplace`).
    #[must_use]
    pub fn to_stderr_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.0.len());
        for &cp in &self.0 {
            push_utf8_or_escape(&mut out, cp);
        }
        out
    }

    /// The path this text names, as Python's `open()` passes it to the OS.
    #[must_use]
    pub fn to_os_string(&self) -> OsString {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt as _;
            OsString::from_vec(self.to_surrogateescape_bytes())
        }
        #[cfg(not(unix))]
        {
            OsString::from(String::from_utf8_lossy(&self.to_surrogateescape_bytes()).into_owned())
        }
    }

    /// The text when it holds no lone surrogate.
    #[must_use]
    pub fn to_string_opt(&self) -> Option<String> {
        self.0.iter().map(|&cp| char::from_u32(cp)).collect()
    }

    #[must_use]
    pub fn code_points(&self) -> &[u32] {
        &self.0
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// `self == s`.
    #[must_use]
    pub fn eq_str(&self, s: &str) -> bool {
        self.0.iter().copied().eq(s.chars().map(u32::from))
    }

    /// `self.startswith(prefix)`.
    #[must_use]
    pub fn starts_with(&self, prefix: &Self) -> bool {
        self.0.starts_with(&prefix.0)
    }

    /// `self[i]` as a code point.
    #[must_use]
    pub fn at(&self, i: usize) -> Option<u32> {
        self.0.get(i).copied()
    }

    /// `self[start..end]`.
    #[must_use]
    pub fn slice(&self, start: usize, end: usize) -> Self {
        Self(self.0[start..end].to_vec())
    }

    /// `self.partition('=')` as `(head, Some(tail))`, or `(self, None)` without a `=`.
    #[must_use]
    pub fn partition_eq(&self) -> (Self, Option<Self>) {
        match self.0.iter().position(|&cp| cp == u32::from('=')) {
            Some(i) => (self.slice(0, i), Some(self.slice(i + 1, self.0.len()))),
            None => (self.clone(), None),
        }
    }

    /// `ch in self` for an ASCII character.
    #[must_use]
    pub fn contains_char(&self, ch: char) -> bool {
        self.0.contains(&u32::from(ch))
    }

    pub fn push_str(&mut self, s: &str) {
        self.0.extend(s.chars().map(u32::from));
    }

    pub fn push_py(&mut self, s: &Self) {
        self.0.extend_from_slice(&s.0);
    }

    /// `repr(self)` (CPython `unicode_repr`): [`gcg_data::py_repr`], extended to lone
    /// surrogates, which are never printable and print as `\udcxx`. The quote is chosen over
    /// the whole text (single unless it holds `'` and no `"`); each surrogate-free run is
    /// escaped by `py_repr` of the run plus the other quote character, which makes `py_repr`
    /// pick that same quote; the outer quote and the added character are then cut off.
    #[must_use]
    pub fn repr(&self) -> Self {
        if let Some(text) = self.to_string_opt() {
            return Self::from_text(&gcg_data::py_repr(&text));
        }
        let quote = if self.contains_char('\'') && !self.contains_char('"') {
            '"'
        } else {
            '\''
        };
        let other = if quote == '"' { '\'' } else { '"' };
        let mut out = String::with_capacity(self.0.len() + 2);
        out.push(quote);
        for run in self.0.split_inclusive(|cp| SURROGATES.contains(cp)) {
            let (text, surrogate) = match run.split_last() {
                Some((&last, rest)) if SURROGATES.contains(&last) => (rest, Some(last)),
                _ => (run, None),
            };
            if !text.is_empty() {
                let mut part: String = text.iter().filter_map(|&cp| char::from_u32(cp)).collect();
                part.push(other);
                let quoted = gcg_data::py_repr(&part);
                // `quote` + escaped run + `other` + `quote`, all ASCII at both ends.
                out.push_str(&quoted[1..quoted.len() - 2]);
            }
            if let Some(cp) = surrogate {
                let _ = write!(out, "\\u{cp:04x}");
            }
        }
        out.push(quote);
        Self::from_text(&out)
    }
}

impl From<&str> for PyStr {
    fn from(s: &str) -> Self {
        Self::from_text(s)
    }
}

/// UTF-8 of a scalar value, or `\uXXXX` (`backslashreplace`) for a lone surrogate.
fn push_utf8_or_escape(out: &mut Vec<u8>, cp: u32) {
    match char::from_u32(cp) {
        Some(c) if !SURROGATES.contains(&cp) => {
            let mut buf = [0; 4];
            out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
        }
        _ => out.extend_from_slice(format!("\\u{cp:04x}").as_bytes()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undecodable_bytes_become_lone_surrogates_and_come_back() {
        let raw = b"a\xffb\xe2\x28\xa1\xed\xa0\x80\xf0\x9f\x98";
        let s = PyStr::from_bytes(raw);
        assert_eq!(
            s.code_points(),
            [
                0x61, 0xDCFF, 0x62, 0xDCE2, 0x28, 0xDCA1, 0xDCED, 0xDCA0, 0xDC80, 0xDCF0, 0xDC9F,
                0xDC98
            ]
        );
        assert_eq!(s.to_surrogateescape_bytes(), raw);
        assert_eq!(
            String::from_utf8(s.to_stderr_bytes()).expect("ascii"),
            "a\\udcffb\\udce2(\\udca1\\udced\\udca0\\udc80\\udcf0\\udc9f\\udc98"
        );
        assert_eq!(
            PyStr::from_bytes("bö3".as_bytes())
                .to_string_opt()
                .as_deref(),
            Some("bö3")
        );
    }

    #[test]
    fn repr_keeps_the_whole_texts_quote_around_surrogates() {
        // Python: repr("'\udcff\""), repr("'\udcffx'"), repr("\udcff\\\n\udc80").
        let r = |cps: &[u32]| {
            PyStr::from_code_points(cps.to_vec())
                .repr()
                .to_string_opt()
                .expect("ASCII")
        };
        assert_eq!(r(&[0x27, 0xDCFF, 0x22]), "'\\'\\udcff\"'");
        assert_eq!(r(&[0x27, 0xDCFF, 0x78, 0x27]), "\"'\\udcffx'\"");
        assert_eq!(r(&[0xDCFF, 0x5C, 0x0A, 0xDC80]), "'\\udcff\\\\\\n\\udc80'");
    }

    #[test]
    fn partition_and_prefixes() {
        let s = PyStr::from("--mat=a=b");
        let (head, tail) = s.partition_eq();
        assert!(head.eq_str("--mat"));
        assert!(tail.expect("has =").eq_str("a=b"));
        assert_eq!(PyStr::from("--x").partition_eq().1, None);
        assert!(PyStr::from("--matches").starts_with(&PyStr::from("--mat")));
    }
}
