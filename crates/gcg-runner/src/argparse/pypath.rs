//! `str(pathlib.Path(text))` on POSIX (CPython 3.15.1 `Lib/pathlib/__init__.py:251-301`,
//! `Lib/posixpath.py:142-162`): the parser's `type=Path` arguments print and open as this
//! normalised text (Inv 01 hazard 49): `./gcg-sim-out/` is `gcg-sim-out`, `a//b` is `a/b`,
//! `''` is `.`, and exactly two leading slashes are kept. Checked against vectors exported by
//! `port/oracle/export_cli.py`.

use std::path::Path;

use super::pystr::PyStr;

/// Stands for each lone surrogate (and each genuine U+FFFD) while `gcg_data::py_path_str`
/// normalises the text.
const PLACEHOLDER: char = '\u{fffd}';

/// `str(PurePosixPath(text))`: [`gcg_data::py_path_str`], extended to lone surrogates. The
/// normalisation only removes separators and `.` components and never reorders, so every
/// placeholder survives in order and the original code points go back in their places.
#[must_use]
pub fn path_str(text: &PyStr) -> PyStr {
    let cps = text.code_points();
    let held: Vec<u32> = cps
        .iter()
        .copied()
        .filter(|&cp| char::from_u32(cp).is_none_or(|c| c == PLACEHOLDER))
        .collect();
    let valid: String = cps
        .iter()
        .map(|&cp| char::from_u32(cp).unwrap_or(PLACEHOLDER))
        .collect();
    let normalised = gcg_data::py_path_str(Path::new(&valid));
    let mut held = held.into_iter();
    PyStr::from_code_points(
        normalised
            .chars()
            .map(|c| {
                if c == PLACEHOLDER {
                    held.next().unwrap_or(u32::from(PLACEHOLDER))
                } else {
                    u32::from(c)
                }
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> String {
        path_str(&PyStr::from(s)).to_string_opt().expect("utf-8")
    }

    #[test]
    fn normalisation() {
        assert_eq!(p(""), ".");
        assert_eq!(p("./gcg-sim-out/"), "gcg-sim-out");
        assert_eq!(p("a//b"), "a/b");
        assert_eq!(p("//x"), "//x");
        assert_eq!(p("///x"), "/x");
        assert_eq!(p("/"), "/");
        assert_eq!(p("/a/../b"), "/a/../b");
    }

    #[test]
    fn surrogates_and_replacement_characters_keep_their_places() {
        // Python: str(Path("a/\udcff//./b\ufffd/\ud800")), str(Path("//\udcff/./")).
        let p = |cps: &[u32]| {
            path_str(&PyStr::from_code_points(cps.to_vec()))
                .code_points()
                .to_vec()
        };
        let slash = u32::from('/');
        let a = u32::from('a');
        let b = u32::from('b');
        let dot = u32::from('.');
        assert_eq!(
            p(&[
                a, slash, 0xDCFF, slash, slash, dot, slash, b, 0xFFFD, slash, 0xD800
            ]),
            [a, slash, 0xDCFF, slash, b, 0xFFFD, slash, 0xD800]
        );
        assert_eq!(
            p(&[slash, slash, 0xDCFF, slash, dot, slash]),
            [slash, slash, 0xDCFF]
        );
    }
}
