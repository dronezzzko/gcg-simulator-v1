//! Card-name normalisation for comparing names written in deck files with the card data
//! (`deck/names.py`), over the generated Unicode tables (docs/port/PLAN.md A27).

use crate::unicode;

/// `_ZERO_WIDTH` (`deck/names.py:7`): deleted everywhere by `strip_invisible` and by
/// `normalize_name`.
const ZERO_WIDTH: [char; 6] = [
    '\u{200B}', '\u{200C}', '\u{200D}', '\u{2060}', '\u{FEFF}', '\u{00AD}',
];

/// `_QUOTES` (`deck/names.py:8-22`): curly and prime quotes straightened.
fn straight_quote(c: char) -> Option<char> {
    match c {
        '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}' | '\u{2032}' | '\u{00B4}' | '`' => {
            Some('\'')
        }
        '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{2033}' => Some('"'),
        _ => None,
    }
}

/// `strip_invisible(text)` (`deck/names.py:25-26`): `text.translate(_ZERO_WIDTH)`.
#[must_use]
pub fn strip_invisible(s: &str) -> String {
    s.chars().filter(|c| !ZERO_WIDTH.contains(c)).collect()
}

/// `normalize_name(name)` (`deck/names.py:29-33`): NFKC, zero-width characters removed, quotes
/// straightened (after NFKC, so `´` and `″` are already decomposed, Inv 03 §3.2), whitespace
/// runs collapsed by `str.split()`, then `casefold()`.
#[must_use]
pub fn normalize_name(s: &str) -> String {
    let text: String = unicode::nfkc(s)
        .chars()
        .filter(|c| !ZERO_WIDTH.contains(c))
        .map(|c| straight_quote(c).unwrap_or(c))
        .collect();
    unicode::casefold(&unicode::split_whitespace(&text).join(" "))
}

/// `names_match(written, card_names)` (`deck/names.py:36-38`).
#[must_use]
pub fn names_match(written: &str, names: &[Box<str>]) -> bool {
    let wanted = normalize_name(written);
    names.iter().any(|n| normalize_name(n) == wanted)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `tests/deck/test_parse.py:65-68` and Inv 03 §3.2.
    #[test]
    fn python_examples() {
        assert_eq!(normalize_name("Zaku \u{2161}"), normalize_name("ZAKU II"));
        assert_eq!(normalize_name("Z\u{2019}Gok\u{A0}E"), "z'gok e");
        assert_eq!(normalize_name("A\u{200B}B"), "ab");
        assert_eq!(normalize_name("Z\u{B4}Gok"), "z \u{301}gok");
        assert_eq!(normalize_name("a\u{2033}b"), "a''b");
        assert_eq!(
            normalize_name("\u{FF3A}\u{FF21}\u{FF2B}\u{FF35}  ii"),
            "zaku ii"
        );
        assert_eq!(normalize_name("Stra\u{DF}e"), "strasse");
        assert_eq!(normalize_name("Gundam\u{3000}Mk-\u{2161}"), "gundam mk-ii");
        assert!(names_match("zaku \u{2161}", &["Zaku II".into()]));
        assert!(!names_match("Zaku", &["Zaku II".into()]));
        assert_eq!(strip_invisible("\u{FEFF}4 GD01\u{AD}-008"), "4 GD01-008");
    }
}
