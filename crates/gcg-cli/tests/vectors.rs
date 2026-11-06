//! The parser's Python building blocks against CPython 3.15.1 vectors
//! (`oracle/export_cli.py`): `int(text, 0)`, `int(text)`, `str(int)` and its digit
//! limit, `repr(str)`, `difflib.get_close_matches`, `str(Path(text))`, `textwrap.wrap`, and
//! the Unicode database version of the generated tables.

mod common;

use common::{py_text, str_of, vectors};
use gcg_runner::argparse::{difflib, pyint, pypath, textwrap, unicode_tables};

#[test]
fn the_tables_come_from_the_exported_unicode_database() {
    let v = vectors();
    assert_eq!(str_of(&v["python"]), "3.15.1");
    assert_eq!(
        str_of(&v["unidata_version"]),
        unicode_tables::UNIDATA_VERSION
    );
}

#[test]
fn int_parsing_matches_python() {
    let v = vectors();
    let cases = v["int"].as_array().expect("int vectors");
    assert!(cases.len() > 60);
    for case in cases {
        let text = py_text(&case["text"]);
        for (key, base) in [("base0", 0), ("base10", 10)] {
            let got = pyint::parse_int(&text, base);
            let want = &case[key];
            match (got, want.is_null()) {
                (None, true) => {}
                (Some(value), false) => {
                    assert_eq!(value.to_decimal(), str_of(&want["value"]), "{text:?} {key}");
                    assert_eq!(
                        value.py_str().is_some(),
                        want["str_ok"].as_bool().expect("bool"),
                        "str() limit of {text:?} {key}"
                    );
                }
                (got, _) => panic!("int({text:?}, {base}): got {got:?}, want {want}"),
            }
        }
    }
}

#[test]
fn repr_matches_python() {
    let v = vectors();
    for case in v["repr"].as_array().expect("repr vectors") {
        let text = py_text(&case["text"]);
        assert_eq!(
            text.repr().to_string_opt().as_deref(),
            Some(str_of(&case["repr"])),
            "repr of {text:?}"
        );
    }
}

#[test]
fn close_matches_match_python() {
    let v = vectors();
    for case in v["close_matches"].as_array().expect("difflib vectors") {
        let word: Vec<u32> = str_of(&case["word"]).chars().map(u32::from).collect();
        let options: Vec<Vec<u32>> = case["possibilities"]
            .as_array()
            .expect("list")
            .iter()
            .map(|p| str_of(p).chars().map(u32::from).collect())
            .collect();
        let refs: Vec<&[u32]> = options.iter().map(Vec::as_slice).collect();
        let got: Option<String> = difflib::close_match(&word, &refs)
            .map(|m| m.iter().filter_map(|&c| char::from_u32(c)).collect());
        let want: Option<String> = case["result"]
            .as_array()
            .expect("list")
            .first()
            .map(|r| str_of(r).to_owned());
        assert_eq!(
            got, want,
            "get_close_matches({}, {})",
            case["word"], case["possibilities"]
        );
    }
}

#[test]
fn path_text_matches_python() {
    let v = vectors();
    for case in v["path"].as_array().expect("path vectors") {
        let got = pypath::path_str(&py_text(&case["text"]));
        assert_eq!(
            got.to_string_opt().as_deref(),
            Some(str_of(&case["str"])),
            "str(Path({}))",
            case["text"]
        );
    }
}

#[test]
fn textwrap_matches_python() {
    let v = vectors();
    let cases = v["wrap"].as_array().expect("wrap vectors");
    assert!(cases.len() > 300);
    for case in cases {
        let text = str_of(&case["text"]);
        let width = usize::try_from(case["width"].as_u64().expect("width")).expect("usize");
        let want: Vec<&str> = case["lines"]
            .as_array()
            .expect("lines")
            .iter()
            .map(str_of)
            .collect();
        assert_eq!(textwrap::wrap(text, width), want, "wrap({text:?}, {width})");
    }
}
