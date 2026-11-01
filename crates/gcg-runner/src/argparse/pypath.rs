//! `str(pathlib.Path(text))` on POSIX (CPython 3.15.1 `Lib/pathlib/__init__.py:251-301`,
//! `Lib/posixpath.py:142-162`): the parser's `type=Path` arguments print and open as this
//! normalised text (Inv 01 hazard 49): `./gcg-sim-out/` is `gcg-sim-out`, `a//b` is `a/b`,
//! `''` is `.`, and exactly two leading slashes are kept. Checked against vectors exported by
//! `port/oracle/export_cli.py`.

use super::pystr::PyStr;

const SEP: u32 = '/' as u32;
const DOT: u32 = '.' as u32;

/// `str(PurePosixPath(text))`.
#[must_use]
pub fn path_str(text: &PyStr) -> PyStr {
    let cps = text.code_points();
    // posixpath.splitroot: "//x" keeps its two slashes; "/x" and "///x" get one.
    let (root, rel): (&[u32], &[u32]) = if cps.first() != Some(&SEP) {
        (&[], cps)
    } else if cps.get(1) != Some(&SEP) || cps.get(2) == Some(&SEP) {
        (&cps[..1], &cps[1..])
    } else {
        (&cps[..2], &cps[2..])
    };
    let mut out: Vec<u32> = root.to_vec();
    let mut first = true;
    for part in rel.split(|&c| c == SEP) {
        if part.is_empty() || part == [DOT] {
            continue;
        }
        if !first {
            out.push(SEP);
        }
        out.extend_from_slice(part);
        first = false;
    }
    if out.is_empty() {
        out.push(DOT);
    }
    PyStr::from_code_points(out)
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
}
