//! Deck sets (docs/port/PLAN.md §2.4, §10.4 F, §12.5) and `compare`'s deck references
//! (`ai/compare.py:116-124`).
//!
//! `tests` (`tests/ai/decks.py` `DECKS`) and `tuning` (`ai/tuning.py:89-93` `TUNING_DECKS`) are
//! embedded: `port/oracle/export_decks.py` writes them as deck files under
//! `crates/gcg-eval/decks/` and checks that each parses back to the Python `DeckList`.
//! `examples` (`examples/decks/*.txt`) and `eval` (`examples/decks/eval/*.txt`) are read at run
//! time relative to the working directory, as Python's `load_decks("examples")` is; `panel` is
//! `examples` then `eval` (8 decks, 28 pairs). The held-out split of the panel is read from
//! `docs/port/eval/panel.json` ([`Panel`]).

use std::path::{Path, PathBuf};

use gcg_data::{Deck, DeckError, DeckList, load_deck, parse_deck, py_path_str, to_decklist};

use crate::DeckSet;

/// `EXAMPLES` (`ai/compare.py:59`).
pub const EXAMPLES_DIR: &str = "examples/decks";
/// The evaluation decks (PLAN §12.5).
pub const EVAL_DIR: &str = "examples/decks/eval";
/// The pre-registered held-out split (PLAN §12.5, X16).
pub const PANEL_FILE: &str = "docs/port/eval/panel.json";

/// `tests/ai/decks.py` `DECKS`, in order.
const TESTS: [(&str, &str); 2] = [
    (
        "tests-federation",
        include_str!("../decks/tests-federation.txt"),
    ),
    ("tests-seed", include_str!("../decks/tests-seed.txt")),
];

/// `ai/tuning.py` `TUNING_DECKS`, in order.
const TUNING: [(&str, &str); 3] = [
    ("tuning-zeon", include_str!("../decks/tuning-zeon.txt")),
    ("tuning-wing", include_str!("../decks/tuning-wing.txt")),
    (
        "tuning-tekkadan",
        include_str!("../decks/tuning-tekkadan.txt"),
    ),
];

/// A deck with the name reports use (a file stem).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct NamedDeck {
    pub name: String,
    pub deck: DeckList,
}

fn embedded_deck(name: &str, text: &str) -> NamedDeck {
    let deck = parse_deck(text, name, None)
        .and_then(|d| to_decklist(&d))
        .unwrap_or_else(|e| panic!("embedded deck {name} does not load: {e}"));
    NamedDeck {
        name: name.to_owned(),
        deck,
    }
}

/// An embedded set (`tests` or `tuning`); `None` for the sets read from disk.
#[must_use]
pub fn embedded(set: DeckSet) -> Option<Vec<NamedDeck>> {
    let table: &[(&str, &str)] = match set {
        DeckSet::Tests => &TESTS,
        DeckSet::Tuning => &TUNING,
        DeckSet::Examples | DeckSet::Eval | DeckSet::Panel => return None,
    };
    Some(table.iter().map(|(n, t)| embedded_deck(n, t)).collect())
}

/// `load_deck(p)` and `to_decklist`, with the CLI's texts (`cli.py:102-110`): I/O and decoding
/// errors name the path themselves, other problems get `{path}: ` in front.
///
/// # Errors
/// The deck file's problem.
pub fn read_deck(path: &Path) -> Result<Deck, String> {
    load_deck(path).map_err(|e| match e {
        DeckError::Io { .. } | DeckError::Decode { .. } => e.to_string(),
        DeckError::Invalid { .. } | DeckError::TooLarge { .. } => {
            format!("{}: {e}", py_path_str(path))
        }
    })
}

fn deck_file(path: &Path) -> Result<NamedDeck, String> {
    let deck = read_deck(path)?;
    let list = to_decklist(&deck).map_err(|e| format!("{}: {e}", py_path_str(path)))?;
    let name = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(NamedDeck { name, deck: list })
}

/// `sorted(Path(dir).glob("*.txt"))`: the `.txt` entries of `dir`, by name.
///
/// # Errors
/// The directory cannot be listed, or a deck does not load.
pub fn dir_decks(dir: &Path) -> Result<Vec<NamedDeck>, String> {
    let entries = std::fs::read_dir(dir)
        .map_err(|e| format!("cannot list deck directory {}: {e}", py_path_str(dir)))?;
    let mut paths: Vec<(String, PathBuf)> = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| format!("cannot list {}: {e}", py_path_str(dir)))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(".txt") {
            paths.push((name, entry.path()));
        }
    }
    paths.sort();
    paths.iter().map(|(_, p)| deck_file(p)).collect()
}

/// The decks of a set, named (docs/port/contracts.md gcg-eval `load_decks`).
///
/// # Errors
/// A deck of `examples` or `eval` does not load.
pub fn load_decks(set: DeckSet) -> Result<Vec<NamedDeck>, String> {
    if let Some(decks) = embedded(set) {
        return Ok(decks);
    }
    match set {
        DeckSet::Examples => dir_decks(Path::new(EXAMPLES_DIR)),
        DeckSet::Eval => dir_decks(Path::new(EVAL_DIR)),
        DeckSet::Panel => {
            let mut decks = dir_decks(Path::new(EXAMPLES_DIR))?;
            decks.extend(dir_decks(Path::new(EVAL_DIR))?);
            Ok(decks)
        }
        DeckSet::Tests | DeckSet::Tuning => unreachable!("embedded above"),
    }
}

/// `load_decks(ref)` of `ai/compare.py:116-124`: `tests`, `tuning`, `examples`, or a
/// comma-separated list of deck files. `module:attr` names Python objects and is refused.
///
/// # Errors
/// A module reference, or a deck that does not load.
pub fn compare_decks(reference: &str) -> Result<Vec<DeckList>, String> {
    let named = if reference == "examples" {
        dir_decks(Path::new(EXAMPLES_DIR))?
    } else if reference == "tests" {
        load_decks(DeckSet::Tests)?
    } else if reference == "tuning" {
        load_decks(DeckSet::Tuning)?
    } else if reference.contains(':') && !reference.ends_with(".txt") {
        return Err(format!(
            "deck reference {reference:?} names a Python module; use tests, tuning, examples \
             or comma-separated deck files"
        ));
    } else {
        reference
            .split(',')
            .map(|p| deck_file(Path::new(p)))
            .collect::<Result<Vec<_>, _>>()?
    };
    Ok(named.into_iter().map(|d| d.deck).collect())
}

/// One deck of a `--pair DECK_A:DECK_B` (PLAN §10.4 F): a path (it ends in `.txt` or holds a
/// `/`), or the stem of a deck in `tests`, `tuning`, `examples` or `eval`, searched in that
/// order.
///
/// # Errors
/// No deck has that name, or the deck does not load.
pub fn resolve_deck(name: &str) -> Result<NamedDeck, String> {
    if name.ends_with(".txt") || name.contains('/') {
        return deck_file(Path::new(name));
    }
    for set in [DeckSet::Tests, DeckSet::Tuning] {
        if let Some(found) = embedded(set).into_iter().flatten().find(|d| d.name == name) {
            return Ok(found);
        }
    }
    for dir in [EXAMPLES_DIR, EVAL_DIR] {
        let path = Path::new(dir).join(format!("{name}.txt"));
        if path.is_file() {
            return deck_file(&path);
        }
    }
    Err(format!(
        "no deck named {name:?} in the tests or tuning sets, {EXAMPLES_DIR} or {EVAL_DIR}"
    ))
}

/// The pre-registered panel split (`docs/port/eval/panel.json`, PLAN §12.5): the names
/// (file stems) of the held-out decks. A pair is held out when it contains one of them (X16).
///
/// File format: `{"held_out": ["<stem>", "<stem>"], ...}`; other keys are ignored.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Panel {
    pub held_out: Vec<String>,
}

impl Panel {
    /// Reads the split.
    ///
    /// # Errors
    /// The file is missing or malformed.
    pub fn load(path: &Path) -> Result<Self, String> {
        let shown = py_path_str(path);
        let text = std::fs::read_to_string(path).map_err(|e| {
            format!("cannot read the held-out split {shown}: {e} (PLAN §12.5 registers it)")
        })?;
        let v: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| format!("{shown}: not JSON: {e}"))?;
        let names = v
            .get("held_out")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| format!("{shown}: \"held_out\" must be a list of deck names"))?;
        let held_out = names
            .iter()
            .map(|n| {
                n.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| format!("{shown}: \"held_out\" must be a list of deck names"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { held_out })
    }

    /// Whether the pair of decks `a`, `b` is a held-out pair.
    #[must_use]
    pub fn is_held_out(&self, a: &str, b: &str) -> bool {
        self.held_out.iter().any(|h| h == a || h == b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_sets_load_in_order() {
        let tests = embedded(DeckSet::Tests).unwrap_or_default();
        assert_eq!(
            tests.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(),
            ["tests-federation", "tests-seed"]
        );
        let tuning = embedded(DeckSet::Tuning).unwrap_or_default();
        assert_eq!(tuning.len(), 3);
        for d in tests.iter().chain(&tuning) {
            assert_eq!(
                (d.deck.main.len(), d.deck.resources.len()),
                (50, 10),
                "{}",
                d.name
            );
        }
        assert!(embedded(DeckSet::Examples).is_none());
        assert!(compare_decks("tests.ai.decks:DECKS").is_err());
    }

    #[test]
    fn held_out_pairs_contain_a_held_out_deck() {
        let panel = Panel {
            held_out: vec!["a".to_owned(), "b".to_owned()],
        };
        assert!(panel.is_held_out("a", "x") && panel.is_held_out("x", "b"));
        assert!(!panel.is_held_out("x", "y"));
    }
}
