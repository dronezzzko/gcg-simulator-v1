//! Deck sets (docs/port/PLAN.md §2.4, §10.4 F, §12.5) and `compare`'s deck references
//! (`ai/compare.py:116-124`).
//!
//! `tests` (`tests/ai/decks.py` `DECKS`) and `tuning` (`ai/tuning.py:89-93` `TUNING_DECKS`) are
//! embedded: `port/oracle/export_decks.py` writes them as deck files under
//! `crates/gcg-eval/decks/` and checks that each parses back to the Python `DeckList`.
//! `examples` (`examples/decks/*.txt`) and `eval` (`examples/decks/eval/*.txt`) are read at run
//! time relative to the working directory, as Python's `load_decks("examples")` is, and from
//! the copies `build.rs` embeds when the working directory has no `examples/decks/` (W-123);
//! `panel` is `examples` then `eval` (8 decks, 28 pairs). The held-out split of the panel is
//! read from `docs/port/eval/panel.json` ([`Panel`]).

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

/// The deck files of `examples/decks/` and `examples/decks/eval/` as they were when the
/// binary was built (`build.rs`, W-123).
mod built {
    include!(concat!(env!("OUT_DIR"), "/embedded_decks.rs"));
}

/// The built-in copy of `examples` or `eval` (and `panel`, their concatenation), used when
/// the working directory has no `examples/decks/` (W-123: an installed binary runs the named
/// sets from any directory); `None` for the always-embedded sets.
#[must_use]
pub fn built_in(set: DeckSet) -> Option<Vec<NamedDeck>> {
    let load = |t: &[(&str, &str)]| {
        t.iter()
            .map(|(n, x)| embedded_deck(n, x))
            .collect::<Vec<_>>()
    };
    match set {
        DeckSet::Examples => Some(load(built::EXAMPLES)),
        DeckSet::Eval => Some(load(built::EVAL)),
        DeckSet::Panel => {
            let mut d = load(built::EXAMPLES);
            d.extend(load(built::EVAL));
            Some(d)
        }
        DeckSet::Tests | DeckSet::Tuning => None,
    }
}

/// `dir`'s decks when the working directory has `examples/decks/`, else the built-in copy.
fn disk_or_built_in(dir: &str, set: DeckSet) -> Result<Vec<NamedDeck>, String> {
    if Path::new(EXAMPLES_DIR).is_dir() {
        dir_decks(Path::new(dir))
    } else {
        Ok(built_in(set).unwrap_or_default())
    }
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
        DeckSet::Examples => disk_or_built_in(EXAMPLES_DIR, set),
        DeckSet::Eval => disk_or_built_in(EVAL_DIR, set),
        DeckSet::Panel => {
            let mut decks = disk_or_built_in(EXAMPLES_DIR, DeckSet::Examples)?;
            decks.extend(disk_or_built_in(EVAL_DIR, DeckSet::Eval)?);
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
    if !Path::new(EXAMPLES_DIR).is_dir()
        && let Some(found) = built_in(DeckSet::Panel)
            .into_iter()
            .flatten()
            .find(|d| d.name == name)
    {
        return Ok(found);
    }
    Err(format!(
        "no deck named {name:?} in the tests or tuning sets, {EXAMPLES_DIR} or {EVAL_DIR}"
    ))
}

/// The pre-registered panel split (`docs/port/eval/panel.json`, PLAN §12.5, X16): the held-out
/// decks and the pairs with their held-out flag. A pair is held out when it contains a
/// held-out deck or when a pair entry flags it (either source suffices, so a split written
/// either way refuses the same pairs).
///
/// Accepted forms, so that the file the deck-panel owner registers is read as PLAN §12.5
/// describes it ("pairs with a held-out flag"):
/// - held-out decks: a list of names under `held_out`, `held_out_decks` or `heldout_decks`, or
///   `decks` entries that are objects with a name (`name`, `deck`, `stem` or `file`) and a flag
///   (`held_out`, `heldout`, `held-out`, or `role`/`split` equal to `held-out`);
/// - pairs: a list under `pairs` of `[a, b, flag]` arrays or of objects with the decks (`decks`
///   as a two-item list, or `a`/`b`, or `deck_a`/`deck_b`) and the flag as above.
///
/// Names are deck file stems; a path or a `.txt` name is reduced to its stem. A file that names
/// no held-out information at all is an error, never a split without held-out pairs.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Panel {
    /// The held-out decks, by stem.
    pub held_out: Vec<String>,
    /// Pair entries `(a, b, held_out)`, by stem.
    pub pairs: Vec<(String, String, bool)>,
}

/// A deck reference reduced to its file stem (`examples/decks/eval/x.txt` -> `x`).
#[must_use]
pub fn stem_of(name: &str) -> String {
    let last = name.rsplit('/').next().unwrap_or(name);
    last.strip_suffix(".txt").unwrap_or(last).to_owned()
}

const DECK_NAME_KEYS: [&str; 4] = ["name", "deck", "stem", "file"];
const FLAG_KEYS: [&str; 4] = ["held_out", "heldout", "held-out", "heldOut"];
const ROLE_KEYS: [&str; 2] = ["role", "split"];

/// The held-out flag of an entry: a boolean flag key, or a role equal to `held-out`.
fn entry_flag(obj: &serde_json::Map<String, serde_json::Value>) -> Option<bool> {
    for k in FLAG_KEYS {
        if let Some(b) = obj.get(k).and_then(serde_json::Value::as_bool) {
            return Some(b);
        }
    }
    for k in ROLE_KEYS {
        if let Some(r) = obj.get(k).and_then(serde_json::Value::as_str) {
            let r = r.to_ascii_lowercase().replace(['_', ' '], "-");
            return Some(r == "held-out" || r == "heldout");
        }
    }
    None
}

impl Panel {
    /// Reads the split.
    ///
    /// # Errors
    /// The file is missing or malformed, or names no held-out information.
    pub fn load(path: &Path) -> Result<Self, String> {
        let shown = py_path_str(path);
        let text = std::fs::read_to_string(path).map_err(|e| {
            format!("cannot read the held-out split {shown}: {e} (PLAN §12.5 registers it)")
        })?;
        let v: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| format!("{shown}: not JSON: {e}"))?;
        Self::from_value(&v).map_err(|e| format!("{shown}: {e}"))
    }

    /// The split of a parsed `panel.json`.
    ///
    /// # Errors
    /// A malformed entry, or no held-out information.
    pub fn from_value(v: &serde_json::Value) -> Result<Self, String> {
        use serde_json::Value;
        let obj = v
            .as_object()
            .ok_or_else(|| "the held-out split is a JSON object".to_owned())?;
        let mut panel = Self::default();
        let mut informed = false;
        for key in ["held_out", "held_out_decks", "heldout_decks"] {
            let Some(list) = obj.get(key) else { continue };
            let items = list
                .as_array()
                .ok_or_else(|| format!("{key:?} must be a list of deck names"))?;
            for n in items {
                let name = n
                    .as_str()
                    .ok_or_else(|| format!("{key:?} must be a list of deck names"))?;
                panel.held_out.push(stem_of(name));
            }
            informed = true;
        }
        if let Some(Value::Array(decks)) = obj.get("decks") {
            for d in decks {
                let Value::Object(o) = d else { continue };
                let name = DECK_NAME_KEYS
                    .iter()
                    .find_map(|k| o.get(*k).and_then(Value::as_str));
                if let (Some(name), Some(flag)) = (name, entry_flag(o)) {
                    informed = true;
                    if flag {
                        panel.held_out.push(stem_of(name));
                    }
                }
            }
        }
        if let Some(pairs) = obj.get("pairs") {
            let items = pairs
                .as_array()
                .ok_or_else(|| "\"pairs\" must be a list".to_owned())?;
            for (i, p) in items.iter().enumerate() {
                let bad = || format!("pair entry {i} needs two deck names and a held-out flag");
                let (a, b, flag) = match p {
                    Value::Array(t) if t.len() == 3 => (
                        t[0].as_str().ok_or_else(bad)?,
                        t[1].as_str().ok_or_else(bad)?,
                        t[2].as_bool().ok_or_else(bad)?,
                    ),
                    Value::Object(o) => {
                        let names = if let Some(Value::Array(d)) = o.get("decks") {
                            (d.len() == 2)
                                .then(|| (d[0].as_str(), d[1].as_str()))
                                .and_then(|(a, b)| a.zip(b))
                        } else {
                            [("a", "b"), ("deck_a", "deck_b")]
                                .iter()
                                .find_map(|(x, y)| {
                                    o.get(*x)
                                        .and_then(Value::as_str)
                                        .zip(o.get(*y).and_then(Value::as_str))
                                })
                        };
                        let (a, b) = names.ok_or_else(bad)?;
                        (a, b, entry_flag(o).ok_or_else(bad)?)
                    }
                    _ => return Err(bad()),
                };
                panel.pairs.push((stem_of(a), stem_of(b), flag));
                informed = true;
            }
        }
        if !informed {
            return Err(
                "no held-out decks or flagged pairs (PLAN §12.5: pairs with a held-out flag)"
                    .to_owned(),
            );
        }
        Ok(panel)
    }

    /// Whether the pair of decks `a`, `b` (names or paths) is a held-out pair.
    #[must_use]
    pub fn is_held_out(&self, a: &str, b: &str) -> bool {
        let (a, b) = (stem_of(a), stem_of(b));
        self.held_out.iter().any(|h| *h == a || *h == b)
            || self
                .pairs
                .iter()
                .any(|(x, y, f)| *f && ((*x == a && *y == b) || (*x == b && *y == a)))
    }

    /// The held-out decks: the listed ones, then (in first-appearance order) every deck of
    /// the pair entries whose every pair is flagged held out (X16: a held-out deck is held
    /// out in all of its pairs, a tuning deck is in at least one tuning pair).
    #[must_use]
    pub fn held_out_decks(&self) -> Vec<String> {
        let mut out = self.held_out.clone();
        for (a, b, _) in &self.pairs {
            for d in [a, b] {
                let all_flagged = self
                    .pairs
                    .iter()
                    .filter(|(x, y, _)| x == d || y == d)
                    .all(|(_, _, f)| *f);
                if all_flagged && !out.contains(d) {
                    out.push(d.clone());
                }
            }
        }
        out
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

    /// W-123: the built-in example decks are the repository's files.
    #[test]
    fn built_in_examples_are_the_files() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/decks");
        let files = dir_decks(&dir).unwrap_or_default();
        assert_eq!(files.len(), 3);
        assert_eq!(built_in(DeckSet::Examples), Some(files));
        assert_eq!(built_in(DeckSet::Tests), None);
    }

    #[test]
    fn held_out_pairs_contain_a_held_out_deck() {
        let panel = Panel {
            held_out: vec!["a".to_owned(), "b".to_owned()],
            pairs: Vec::new(),
        };
        assert!(panel.is_held_out("a", "x") && panel.is_held_out("x", "b"));
        assert!(!panel.is_held_out("x", "y"));
        assert!(panel.is_held_out("examples/decks/eval/a.txt", "y"));
    }

    #[test]
    fn panel_files_with_flagged_pairs_are_read() {
        let v = serde_json::json!({
            "version": 1,
            "decks": [{"name": "h", "role": "held-out"}, {"name": "t", "role": "tuning"}],
            "pairs": [
                {"decks": ["h", "t"], "held_out": true},
                {"a": "t", "b": "u", "heldout": false},
                ["examples/decks/eval/v.txt", "h", true],
                {"deck_a": "u", "deck_b": "w", "split": "tuning"}
            ]
        });
        let panel = Panel::from_value(&v).unwrap_or_default();
        assert!(panel.is_held_out("t", "h") && panel.is_held_out("v", "h"));
        assert!(!panel.is_held_out("t", "u") && !panel.is_held_out("u", "w"));
        assert_eq!(panel.held_out_decks(), ["h", "v"]);
        let flagged_only = serde_json::json!({"pairs": [["x", "y", true], ["y", "z", false]]});
        let p = Panel::from_value(&flagged_only).unwrap_or_default();
        assert!(p.is_held_out("y", "x") && !p.is_held_out("z", "y"));
        assert_eq!(p.held_out_decks(), ["x"]);
        assert!(Panel::from_value(&serde_json::json!({"version": 1})).is_err());
        assert!(Panel::from_value(&serde_json::json!({"pairs": [["x", "y"]]})).is_err());
        assert!(Panel::from_value(&serde_json::json!({"held_out": "x"})).is_err());
    }
}
