//! T-036: deck texts, deck files and directly constructed decks give Python's parse outcome,
//! validation results (default, `banlist=False`, `implemented=False`) and `require_legal`
//! text, message for message and in Python's order (`decks.json`). Every violation code
//! occurs in the fixture.

mod common;

use std::path::{Path, PathBuf};

use common::{
    Unimplemented, arr, counts_json, deck_from, entries_json, fixture, opt_s, s, violation_rows,
};
use gcg_data::{
    Deck, DeckError, ValidateOptions, ViolationCode, deck_digest, load_deck, parse_deck,
    require_legal, validate,
};
use serde_json::Value;

fn check_error(id: &str, err: &DeckError, want: &Value) {
    assert_eq!(err.to_string(), s(&want["str"]), "{id}: str(DeckError)");
    let DeckError::Invalid {
        deck_name,
        problems,
        violations,
    } = err
    else {
        panic!("{id}: expected DeckError::Invalid, got {err}");
    };
    assert_eq!(deck_name.as_deref(), opt_s(&want["deck_name"]), "{id}");
    let want_problems: Vec<&str> = arr(&want["problems"]).iter().map(s).collect();
    assert_eq!(problems, &want_problems, "{id}");
    assert_eq!(violation_rows(violations), want["violations"], "{id}");
}

fn check_deck(id: &str, deck: &Deck, want: &Value, dir: Option<&str>) {
    assert_eq!(&*deck.name, s(&want["name"]), "{id}: name");
    let source = deck
        .source
        .as_deref()
        .map(|src| dir.map_or_else(|| src.to_owned(), |d| src.replace(d, "<dir>")));
    assert_eq!(source.as_deref(), opt_s(&want["source"]), "{id}: source");
    assert_eq!(counts_json(&deck.main), want["main"], "{id}: main");
    assert_eq!(
        counts_json(&deck.resources),
        want["resources"],
        "{id}: resources"
    );
    assert_eq!(
        entries_json(&deck.entries),
        want["entries"],
        "{id}: entries"
    );
    assert_eq!(deck.digest, s(&want["digest"]), "{id}: digest");
    assert_eq!(deck_digest(deck), s(&want["digest"]), "{id}: deck_digest");
}

fn check_validations(id: &str, deck: &Deck, case: &Value) {
    let status = Unimplemented::from_case(case);
    let want = &case["validate"];
    let run = |banlist: bool, implemented: bool| {
        violation_rows(&validate(
            deck,
            ValidateOptions {
                banlist,
                implemented: implemented.then_some(&status as &dyn gcg_data::ImplementationStatus),
            },
        ))
    };
    assert_eq!(
        run(true, true),
        want["default"],
        "{id}: validate_deck(deck)"
    );
    assert_eq!(run(false, true), want["no_banlist"], "{id}: banlist=False");
    assert_eq!(
        run(true, false),
        want["no_implemented"],
        "{id}: implemented=False"
    );
    let legal = require_legal(deck, &status).err().map(|e| e.to_string());
    assert_eq!(
        legal.as_deref(),
        opt_s(&want["require_legal"]),
        "{id}: require_legal"
    );
}

fn codes_seen(case: &Value, seen: &mut Vec<String>) {
    let mut rows: Vec<&Value> = Vec::new();
    if let Some(v) = case.get("validate") {
        for key in ["default", "no_banlist", "no_implemented"] {
            rows.extend(arr(&v[key]));
        }
    }
    if let Some(e) = case.get("error") {
        rows.extend(arr(&e["violations"]));
    }
    for r in rows {
        let code = s(&r[0]).to_owned();
        if !seen.contains(&code) {
            seen.push(code);
        }
    }
}

fn scratch_dir() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("gcg-data-deck-files");
    if dir.exists() {
        std::fs::remove_dir_all(&dir).expect("clear the scratch directory");
    }
    std::fs::create_dir_all(&dir).expect("create the scratch directory");
    dir
}

fn hex_bytes(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("hex byte"))
        .collect()
}

fn check_file(case: &Value, dir: &Path) {
    let id = s(&case["id"]);
    let name = s(&case["filename"]);
    let path = dir.join(name);
    match case["bytes_hex"].as_str() {
        Some(hex) => std::fs::write(&path, hex_bytes(hex)).expect("write the deck file"),
        None if name == "a-directory" => std::fs::create_dir_all(&path).expect("mkdir"),
        None => {}
    }
    let shown = path.to_string_lossy().into_owned();
    let dir_text = dir.to_string_lossy().into_owned();
    let result = load_deck(&path);
    if let Some(strerror) = case.get("os_error") {
        let e = result
            .err()
            .unwrap_or_else(|| panic!("{id}: Python raised OSError"));
        assert!(matches!(e, DeckError::Io { .. }), "{id}: {e}");
        assert_eq!(
            e.to_string(),
            format!("cannot read deck file {shown}: {}", s(strerror)),
            "{id}"
        );
    } else if let Some(decode) = case.get("decode_error") {
        let e = result
            .err()
            .unwrap_or_else(|| panic!("{id}: Python raised UnicodeDecodeError"));
        let DeckError::Decode { reason, start, .. } = &e else {
            panic!("{id}: expected Decode, got {e}");
        };
        assert_eq!(
            (*reason, *start as u64),
            (s(&decode[0]), decode[1].as_u64().unwrap())
        );
        assert_eq!(
            e.to_string(),
            format!(
                "{shown}: not UTF-8 text ({} at byte {})",
                s(&decode[0]),
                decode[1]
            ),
            "{id}"
        );
    } else if let Some(want) = case.get("error") {
        let e = result
            .err()
            .unwrap_or_else(|| panic!("{id}: Python raised DeckError"));
        check_error(id, &e, want);
    } else {
        let deck = result.unwrap_or_else(|e| panic!("{id}: {e}"));
        check_deck(id, &deck, &case["deck"], Some(&dir_text));
        check_validations(id, &deck, case);
    }
}

#[test]
fn deck_cases_equal_python() {
    let fx = fixture("decks.json");
    let cases = arr(&fx["cases"]);
    assert!(cases.len() >= 450, "{} cases", cases.len());
    let dir = scratch_dir();
    let mut seen = Vec::new();
    for case in cases {
        codes_seen(case, &mut seen);
        let id = s(&case["id"]);
        match s(&case["kind"]) {
            "text" => {
                let name = s(&case["name"]);
                let source = format!("{name}.txt");
                match parse_deck(s(&case["text"]), name, Some(&source)) {
                    Err(e) => {
                        let want = case.get("error").unwrap_or_else(|| panic!("{id}: {e}"));
                        check_error(id, &e, want);
                    }
                    Ok(deck) => {
                        assert!(case.get("error").is_none(), "{id}: Python raised DeckError");
                        check_deck(id, &deck, &case["deck"], None);
                        check_validations(id, &deck, case);
                    }
                }
            }
            "constructed" => {
                let deck = deck_from(&case["deck"]);
                assert_eq!(deck_digest(&deck), deck.digest, "{id}: digest");
                check_validations(id, &deck, case);
            }
            "file" => check_file(case, &dir),
            other => panic!("{id}: unknown case kind {other}"),
        }
    }
    let mut all: Vec<&str> = ViolationCode::ALL.iter().map(|c| c.as_str()).collect();
    all.retain(|c| !seen.iter().any(|x| x == c));
    assert!(all.is_empty(), "violation codes without a case: {all:?}");
}

/// `tests/deck/test_parse.py:132-139` and Inv 03 §3.3: the blue-white example's digest.
#[test]
fn example_digest() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/decks/blue-white-federation.txt");
    let deck = load_deck(&path).expect("example deck loads");
    assert_eq!(
        deck.digest,
        "a75b15f0983c82469c25de9ee34a7dc8ea420be1534ec2937ac6343565c9e108"
    );
    assert_eq!(&*deck.name, "blue-white-federation");
    let a = parse_deck("4 GD01-008 Guntank\n2 GD01-001\n10 R-001\n", "a", None).unwrap();
    let b = parse_deck(
        "# x\n2 GD01-001_p1 Gundam\n2 GD01-008\n2 GD01-008\n10 R-001\n",
        "b",
        None,
    )
    .unwrap();
    assert_eq!(a.digest, b.digest);
    assert_eq!(a.entries.len(), 3);
    assert_eq!(b.entries.len(), 4);
}

/// W-144: a count Python would expand per copy (or fail to convert) is checked from the
/// run-length totals: 10^12 copies give the size and copy-limit violations without allocating
/// one entry per copy, and `to_decklist` refuses to expand them.
#[test]
fn huge_counts_validate_without_expanding() {
    let deck = parse_deck("1000000000000 GD01-001\n10 R-001\n", "huge", None).unwrap();
    let rows = violation_rows(&validate(
        &deck,
        ValidateOptions {
            banlist: true,
            implemented: None,
        },
    ));
    assert_eq!(rows[0][0], "MAIN_SIZE");
    assert_eq!(
        rows[0][1],
        "the deck has 1000000000000 cards; it must have exactly 50 (rule 6-1-1)"
    );
    assert_eq!(rows[1][0], "COPY_LIMIT");
    assert!(
        s(&rows[1][1]).starts_with("1000000000000 copies of GD01-001 Gundam; at most 4 copies")
    );
    let err = gcg_data::to_decklist(&deck).unwrap_err();
    assert!(matches!(
        err,
        DeckError::TooLarge {
            main: 1_000_000_000_000,
            resources: 10,
            ..
        }
    ));
    assert_eq!(
        err.to_string(),
        "deck 'huge' has 1000000000000 cards and 10 resources; a game can hold at most 65535 cards"
    );
    let ok =
        gcg_data::to_decklist(&parse_deck("2 GD01-008\n1 GD01-001\n", "x", None).unwrap()).unwrap();
    let id = |n: &str| gcg_data::CardDb::embedded().get(n).unwrap().def_id;
    assert_eq!(&*ok.main, [id("GD01-001"), id("GD01-008"), id("GD01-008")]);
    assert!(ok.resources.is_empty());
}
