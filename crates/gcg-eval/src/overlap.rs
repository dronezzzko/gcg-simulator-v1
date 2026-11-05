//! `gcg-eval decks --overlap` (docs/dev/PLAN.md §12.5; W-20): main-deck copy overlaps between
//! deck sets, and the panel bound on the held-out decks.
//!
//! The overlap of two decks is the size of the multiset intersection of their main decks
//! (`sum over cards of min(copies in A, copies in B)`, out of 50); resource decks are not
//! counted. The matrix rows default to the panel (examples, then evaluation decks) and the
//! columns to the tuning decks (`ai/tuning.py:89-93`) and the AI test decks
//! (`tests/ai/decks.py`). Each row deck is labelled with its largest overlap against the
//! tuning decks and against the test decks, and, when `docs/dev/eval/panel.json` exists,
//! whether the split holds it out. With `--check` every held-out deck of the split must
//! overlap every tuning deck and every test deck by at most `--bound` copies (default 10,
//! §12.5), or the command exits 1.

use std::io::Write;
use std::path::{Path, PathBuf};

use gcg_core::pyjson::Json;
use gcg_data::{DeckList, DefId};

use crate::DeckSet;
use crate::bench_cmd::CommandError;
use crate::decks::{NamedDeck, PANEL_FILE, Panel, load_decks};

/// The report format.
pub const OVERLAP_FORMAT: &str = "gcg-eval-deck-overlap";
pub const OVERLAP_VERSION: u32 = 1;
/// The §12.5 bound: a held-out deck shares at most 10 of 50 main-deck copies with any tuning
/// or AI test deck.
pub const DEFAULT_BOUND: u32 = 10;

/// The parsed arguments.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct OverlapArgs {
    pub rows: Vec<DeckSet>,
    pub cols: Vec<DeckSet>,
    pub bound: u32,
    pub check: bool,
    pub out: Option<PathBuf>,
}

/// The main-deck copy overlap of two decks.
#[must_use]
pub fn overlap(a: &DeckList, b: &DeckList) -> u32 {
    let count = |d: &DeckList| {
        let mut ids: Vec<DefId> = d.main.to_vec();
        ids.sort();
        let mut out: Vec<(DefId, u32)> = Vec::new();
        for id in ids {
            match out.last_mut() {
                Some((last, n)) if *last == id => *n += 1,
                _ => out.push((id, 1)),
            }
        }
        out
    };
    let (ca, cb) = (count(a), count(b));
    let (mut i, mut j, mut total) = (0, 0, 0);
    while i < ca.len() && j < cb.len() {
        match ca[i].0.cmp(&cb[j].0) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                total += ca[i].1.min(cb[j].1);
                i += 1;
                j += 1;
            }
        }
    }
    total
}

fn load_sets(sets: &[DeckSet]) -> Result<Vec<NamedDeck>, String> {
    let mut out = Vec::new();
    for s in sets {
        out.extend(load_decks(*s)?);
    }
    Ok(out)
}

fn max_against(d: &NamedDeck, others: &[NamedDeck]) -> (u32, String) {
    let mut best = (0, String::new());
    for o in others {
        let v = overlap(&d.deck, &o.deck);
        if v > best.0 || best.1.is_empty() {
            best = (v, o.name.clone());
        }
    }
    best
}

/// The held-out decks of `split` that break the bound: `(held-out deck, other deck,
/// overlap)` in order, plus held-out names that no row deck carries.
#[must_use]
pub fn violations(
    split: &Panel,
    panel: &[NamedDeck],
    tuning: &[NamedDeck],
    tests: &[NamedDeck],
    bound: u32,
) -> (Vec<(String, String, u32)>, Vec<String>) {
    let mut over = Vec::new();
    let mut unknown = Vec::new();
    for name in split.held_out_decks() {
        let Some(d) = panel.iter().find(|d| d.name == name) else {
            unknown.push(name);
            continue;
        };
        for o in tuning.iter().chain(tests) {
            let v = overlap(&d.deck, &o.deck);
            if v > bound {
                over.push((name.clone(), o.name.clone(), v));
            }
        }
    }
    (over, unknown)
}

/// The overlap document.
///
/// # Errors
/// A deck set that does not load, or a malformed split.
pub fn document(args: &OverlapArgs) -> Result<(Json, bool), String> {
    let rows = load_sets(&args.rows)?;
    let cols = load_sets(&args.cols)?;
    let tuning = load_decks(DeckSet::Tuning)?;
    let tests = load_decks(DeckSet::Tests)?;
    let split = if Path::new(PANEL_FILE).exists() {
        Some(Panel::load(Path::new(PANEL_FILE))?)
    } else {
        None
    };
    let held: Vec<String> = split
        .as_ref()
        .map(Panel::held_out_decks)
        .unwrap_or_default();
    let mut doc = Json::object();
    doc.set("format", OVERLAP_FORMAT);
    doc.set("version", OVERLAP_VERSION);
    doc.set("bound", args.bound);
    doc.set(
        "rows",
        Json::Array(rows.iter().map(|d| d.name.as_str().into()).collect()),
    );
    doc.set(
        "cols",
        Json::Array(cols.iter().map(|d| d.name.as_str().into()).collect()),
    );
    doc.set(
        "matrix",
        Json::Array(
            rows.iter()
                .map(|r| {
                    Json::Array(
                        cols.iter()
                            .map(|c| overlap(&r.deck, &c.deck).into())
                            .collect(),
                    )
                })
                .collect(),
        ),
    );
    let labels = rows
        .iter()
        .map(|d| {
            let (t, tn) = max_against(d, &tuning);
            let (a, an) = max_against(d, &tests);
            let mut o = Json::object();
            o.set("deck", d.name.as_str());
            o.set("max_tuning", t);
            o.set("max_tuning_deck", tn.as_str());
            o.set("max_tests", a);
            o.set("max_tests_deck", an.as_str());
            o.set("within_bound", t <= args.bound && a <= args.bound);
            o.set(
                "held_out",
                if split.is_some() {
                    Json::Bool(held.contains(&d.name))
                } else {
                    Json::Null
                },
            );
            o
        })
        .collect();
    doc.set("decks", Json::Array(labels));
    let mut ok = true;
    let check = match &split {
        None => Json::Null,
        Some(s) => {
            let panel = load_decks(DeckSet::Panel).unwrap_or_else(|_| rows.clone());
            let (over, unknown) = violations(s, &panel, &tuning, &tests, args.bound);
            ok = over.is_empty() && unknown.is_empty() && !held.is_empty();
            let mut c = Json::object();
            c.set(
                "held_out",
                Json::Array(held.iter().map(|h| h.as_str().into()).collect()),
            );
            c.set(
                "over_bound",
                Json::Array(
                    over.iter()
                        .map(|(h, o, v)| {
                            Json::Array(vec![h.as_str().into(), o.as_str().into(), (*v).into()])
                        })
                        .collect(),
                ),
            );
            c.set(
                "unknown",
                Json::Array(unknown.iter().map(|u| u.as_str().into()).collect()),
            );
            c.set("pass", ok);
            c
        }
    };
    doc.set("check", check);
    if args.check && split.is_none() {
        return Err(format!("--check needs the held-out split in {PANEL_FILE}"));
    }
    Ok((doc, ok))
}

/// Runs `decks --overlap`.
///
/// # Errors
/// A setup problem, the output, or a failed `--check`.
pub fn run(args: &OverlapArgs, out: &mut dyn Write) -> Result<(), CommandError> {
    let (doc, ok) = document(args).map_err(CommandError::Invalid)?;
    crate::rows::emit(&doc, args.out.as_ref(), out)?;
    if args.check && !ok {
        return Err(CommandError::Invalid(format!(
            "a held-out deck shares more than {} main-deck copies with a tuning or test deck \
             (PLAN §12.5)",
            args.bound
        )));
    }
    Ok(())
}
