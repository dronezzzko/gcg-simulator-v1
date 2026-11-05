//! Native Rust version of `tests/deck` (T-078): `test_examples.py`, `test_parse.py` and
//! `test_validate.py`, with Python's inputs and assertions, over `gcg-data`'s parser and
//! validator, the effect registry and the engine. Two tests are not reproduced here because they
//! replace module globals (`monkeypatch` of `official.banlist`); their exported Python results
//! are compared by `gcg-data` (`src/validate.rs::attribute_notes_equal_python`), as the
//! register `tests/python_suites.json` records.
//!
//! In Python a deck's `main` is the sorted tuple of every copy's card number; here it is the
//! sorted list of `(card number, copies)`, so `Counter(deck.main)` is the list itself and
//! `deck.main.count(n)` is `copies(&deck.main, n)`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gcg_core::SplitMix64;
use gcg_data::official::{banlist, matches};
use gcg_data::{
    CardCount, CardDb, CardType, Deck, DeckEntry, DeckError, DefId, ImplementationStatus,
    MAIN_DECK_TYPES, ValidateOptions, Violation, deck_digest, load_deck, parse_deck, require_legal,
    to_decklist, validate,
};
use gcg_effects::Registry;
use gcg_engine::{NewGameOptions, apply, new_game};

const BL_DATE: &str = "2026-09-25";
const VANILLA: &str = "a Unit card that is Lv.2 with cost 1, 2 AP, and 2 HP, and without effects";
const NOT_APPLIED: &str = "is not applied: the simulator cannot verify an unmodified starter deck";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn example_paths() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(root().join("examples/decks"))
        .expect("examples/decks")
        .map(|e| e.expect("entry").path())
        .filter(|p| p.extension().is_some_and(|x| x == "txt"))
        .collect();
    out.sort();
    out
}

fn db() -> &'static CardDb {
    CardDb::embedded()
}

fn registry() -> Arc<Registry> {
    Registry::embedded()
}

/// `validate_deck(deck)` with the registry's implementation status.
fn violations(deck: &Deck) -> Vec<Violation> {
    let reg = registry();
    validate(
        deck,
        ValidateOptions {
            banlist: true,
            implemented: Some(reg.as_ref()),
        },
    )
}

fn codes_with(deck: &Deck, banlist: bool, implemented: bool) -> Vec<&'static str> {
    let reg = registry();
    let status: Option<&dyn ImplementationStatus> = if implemented {
        Some(reg.as_ref())
    } else {
        None
    };
    validate(
        deck,
        ValidateOptions {
            banlist,
            implemented: status,
        },
    )
    .iter()
    .map(|v| v.code.as_str())
    .collect()
}

fn codes(deck: &Deck) -> Vec<&'static str> {
    codes_with(deck, true, true)
}

fn only(deck: &Deck, code: &str) -> Vec<String> {
    violations(deck)
        .into_iter()
        .filter(|v| v.code.as_str() == code)
        .map(|v| v.message)
        .collect()
}

fn copies(list: &[CardCount], number: &str) -> u128 {
    list.iter()
        .find(|c| &*c.card_number == number)
        .map_or(0, |c| c.count)
}

fn counter(list: &[CardCount]) -> BTreeMap<String, u128> {
    list.iter()
        .map(|c| (c.card_number.to_string(), c.count))
        .collect()
}

/// `deck_text(counts)` of `tests/deck/conftest.py`, in the counts' order.
fn deck_text(counts: &[(String, i64)]) -> String {
    let mut s: String = counts
        .iter()
        .filter(|(_, c)| *c > 0)
        .map(|(n, c)| format!("{c} {n}"))
        .collect::<Vec<_>>()
        .join("\n");
    s.push('\n');
    s
}

/// `base_counts`: `dict(Counter(deck.main + deck.resources))` of the Blue/White example, in
/// first-occurrence order (the sorted main deck, then the resources).
fn base_counts() -> Vec<(String, i64)> {
    let text = std::fs::read_to_string(root().join("examples/decks/blue-white-federation.txt"))
        .expect("example deck");
    let deck = parse_deck(&text, "deck", None).expect("parses");
    deck.main
        .iter()
        .chain(deck.resources.iter())
        .map(|c| {
            (
                c.card_number.to_string(),
                i64::try_from(c.count).expect("small"),
            )
        })
        .collect()
}

/// `swap(counts, remove, add)`: dict order kept, new keys appended, non-positive dropped.
fn swap(
    counts: &[(String, i64)],
    remove: &[(&str, i64)],
    add: &[(&str, i64)],
) -> Vec<(String, i64)> {
    let mut out = counts.to_vec();
    for (n, c) in remove {
        let e = out
            .iter_mut()
            .find(|(k, _)| k == n)
            .unwrap_or_else(|| panic!("{n} is in the deck"));
        e.1 -= c;
    }
    for (n, c) in add {
        match out.iter_mut().find(|(k, _)| k == n) {
            Some(e) => e.1 += c,
            None => out.push(((*n).to_owned(), *c)),
        }
    }
    out.retain(|(_, c)| *c > 0);
    out
}

fn with(counts: &[(String, i64)], number: &str, count: i64) -> Vec<(String, i64)> {
    let mut out = counts.to_vec();
    match out.iter_mut().find(|(k, _)| k == number) {
        Some(e) => e.1 = count,
        None => out.push((number.to_owned(), count)),
    }
    out
}

/// `make_deck(counts)`: `parse_deck(deck_text(counts), name="test-deck")`.
fn make_deck(counts: &[(String, i64)]) -> Deck {
    parse_deck(&deck_text(counts), "test-deck", None).expect("the deck parses")
}

fn invalid(err: DeckError) -> (Vec<String>, Vec<Violation>, String) {
    let text = err.to_string();
    match err {
        DeckError::Invalid {
            problems,
            violations,
            ..
        } => (problems, violations, text),
        other => panic!("not a DeckError(problems): {other}"),
    }
}

fn parse_error(text: &str, name: &str) -> (Vec<String>, Vec<Violation>, String) {
    invalid(parse_deck(text, name, None).expect_err("a DeckError"))
}

fn name_of(number: &str) -> String {
    db().get(number).expect("a card").name.to_string()
}

// --- test_examples.py -----------------------------------------------------------------------

#[test]
fn there_are_at_least_three_example_decks() {
    assert!(example_paths().len() >= 3);
}

// gcg-tags: rule=6-1-1 rule=6-1-1-1 rule=6-1-1-2 rule=6-1-1-3 rule=6-1-1-4 rule=6-1-1-5
#[test]
fn example_deck_is_legal_and_implemented() {
    for path in example_paths() {
        let deck = load_deck(&path).expect("loads");
        assert_eq!(violations(&deck), [], "{}", path.display());
        assert_eq!(deck.main.iter().map(|c| c.count).sum::<u128>(), 50);
        assert_eq!(deck.resources.iter().map(|c| c.count).sum::<u128>(), 10);
    }
}

#[test]
fn example_deck_names_its_archetype_first() {
    for path in example_paths() {
        let text = std::fs::read_to_string(&path).expect("read");
        let first = text.lines().next().unwrap_or_default();
        assert!(first.starts_with("# "), "{}", path.display());
        assert!(first.chars().count() > 10, "{}", path.display());
    }
}

#[test]
fn needs_engine_example_matchups_play_to_completion() {
    let reg = registry();
    let paths = example_paths();
    for a in &paths {
        for b in &paths {
            if a == b {
                continue;
            }
            let decks = [
                to_decklist(&load_deck(a).expect("a")).expect("a decklist"),
                to_decklist(&load_deck(b).expect("b")).expect("b decklist"),
            ];
            for seed in 0..3_u64 {
                let mut st = new_game(
                    &reg,
                    [&decks[0], &decks[1]],
                    seed,
                    NewGameOptions::default(),
                )
                .expect("a new game");
                let mut rng = SplitMix64::new(seed + 100);
                while let Some(options) = st.pending().map(|p| p.options.to_vec()) {
                    let action = options[rng.randrange_usize(options.len())];
                    apply(&mut st, action, true).expect("a legal action");
                }
                assert!(st.winner().is_some(), "{a:?} vs {b:?} seed {seed}");
                assert!(st.end_reason().is_some(), "{a:?} vs {b:?} seed {seed}");
            }
        }
    }
}

// --- test_parse.py --------------------------------------------------------------------------

const SAMPLE: &str = concat!(
    "# a comment line\n",
    "4 GD01-008 Guntank\n",
    "\n",
    "2 GD01-001_p1 Gundam\n",
    "3x st01-005 GM\n",
    "   # indented comment\n",
    "10 R-001 Resource\n",
);

#[test]
fn parses_counts_comments_and_blank_lines() {
    let deck = parse_deck(SAMPLE, "sample", Some("sample.txt")).expect("parses");
    assert_eq!(&*deck.name, "sample");
    assert_eq!(deck.source.as_deref(), Some("sample.txt"));
    assert_eq!(
        counter(&deck.main),
        BTreeMap::from([
            ("GD01-001".to_owned(), 2),
            ("GD01-008".to_owned(), 4),
            ("ST01-005".to_owned(), 3)
        ])
    );
    assert_eq!(
        counter(&deck.resources),
        BTreeMap::from([("R-001".to_owned(), 10)])
    );
    let numbers: Vec<&str> = deck.main.iter().map(|c| &*c.card_number).collect();
    let mut sorted = numbers.clone();
    sorted.sort();
    assert_eq!(numbers, sorted);
    assert_eq!(
        deck.entries.iter().map(|e| e.line).collect::<Vec<_>>(),
        [2, 4, 5, 7]
    );
}

#[test]
fn alt_art_product_ids_normalize_to_the_card_number() {
    let deck =
        parse_deck("2 GD01-001_p1\n1 gd01-001_P2\n1 GD01-001\n", "deck", None).expect("parses");
    assert_eq!(
        counter(&deck.main),
        BTreeMap::from([("GD01-001".to_owned(), 4)])
    );
    assert_eq!(
        deck.entries.iter().map(|e| &*e.written).collect::<Vec<_>>(),
        ["GD01-001_p1", "gd01-001_P2", "GD01-001"]
    );
    assert!(deck.entries.iter().all(|e| &*e.card_number == "GD01-001"));
}

#[test]
fn unknown_alt_art_suffix_of_a_known_card_normalizes() {
    let deck = parse_deck("1 GD01-001_p99\n", "deck", None).expect("parses");
    assert_eq!(
        counter(&deck.main),
        BTreeMap::from([("GD01-001".to_owned(), 1)])
    );
}

#[test]
fn names_are_normalized_before_comparison() {
    for (line, number) in [
        ("4 GD01-035 Zaku II", "GD01-035"),
        ("4 GD01-035 zaku \u{2161}", "GD01-035"),
        ("4 ST03-006 Char's Zaku II", "ST03-006"),
        (
            "4 ST03-006 Char\u{2019}s  Zaku\u{200b} \u{2161}",
            "ST03-006",
        ),
        ("4 GD03-027 Z'Gok E", "GD03-027"),
        ("4 GD03-027 Z\u{2019}Gok E", "GD03-027"),
        ("\u{feff}4 GD01-008 Guntank", "GD01-008"),
    ] {
        let deck = parse_deck(&format!("{line}\n"), "deck", None)
            .unwrap_or_else(|e| panic!("{line:?}: {e}"));
        assert_eq!(
            counter(&deck.main),
            BTreeMap::from([(number.to_owned(), 4)]),
            "{line:?}"
        );
    }
}

#[test]
fn normalize_name() {
    use gcg_data::names::normalize_name;
    assert_eq!(normalize_name("Zaku \u{2161}"), normalize_name("ZAKU II"));
    assert_eq!(normalize_name("Z\u{2019}Gok\u{a0}E"), "z'gok e");
    assert_eq!(normalize_name("A\u{200b}B"), "ab");
}

#[test]
fn syntax_error_names_the_line() {
    let (problems, violations, _) = parse_error("4 GD01-008\nfour GD01-001\n", "deck");
    assert_eq!(
        problems,
        ["line 2: expected '<count> <card_number> [name]', got 'four GD01-001'"]
    );
    assert_eq!(
        violations
            .iter()
            .map(|v| v.code.as_str())
            .collect::<Vec<_>>(),
        ["SYNTAX"]
    );
}

#[test]
fn zero_count_is_a_syntax_error() {
    let (problems, _, _) = parse_error("0 GD01-008\n", "deck");
    assert_eq!(problems, ["line 1: count must be at least 1 for GD01-008"]);
}

#[test]
fn unknown_id_is_reported_with_its_line() {
    let (problems, violations, _) = parse_error("4 GD01-008\n4 GD99-999 Nothing\n", "deck");
    assert_eq!(
        problems,
        ["line 2: unknown card id 'GD99-999' (not in the card data)"]
    );
    assert_eq!(violations[0].code.as_str(), "UNKNOWN_ID");
}

#[test]
fn name_mismatch_is_reported() {
    let (problems, violations, _) = parse_error("4 GD01-008 Gundam\n", "deck");
    assert_eq!(
        problems,
        ["line 1: GD01-008 is 'Guntank' in the card data, not 'Gundam'"]
    );
    assert_eq!(violations[0].code.as_str(), "NAME_MISMATCH");
    assert_eq!(&*violations[0].cards, [Box::from("GD01-008")]);
}

#[test]
fn every_parse_problem_is_listed_in_one_error() {
    let (problems, violations, message) =
        parse_error("4 GD01-008 Gundam\nbad line\n4 XX-1\n", "broken");
    assert_eq!(
        violations
            .iter()
            .map(|v| v.code.as_str())
            .collect::<Vec<_>>(),
        ["NAME_MISMATCH", "SYNTAX", "UNKNOWN_ID"]
    );
    assert_eq!(
        message.lines().next(),
        Some("deck 'broken' has 3 problem(s):")
    );
    assert!(
        problems
            .iter()
            .all(|p| message.contains(&format!("  - {p}")))
    );
}

#[test]
fn load_deck_uses_file_stem_and_handles_bom() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("deck-suite-bom");
    std::fs::create_dir_all(&dir).expect("dir");
    let path = dir.join("my-deck.txt");
    std::fs::write(&path, "\u{feff}# header\n4 GD01-008 Guntank\n").expect("write");
    let deck = load_deck(&path).expect("loads");
    assert_eq!(&*deck.name, "my-deck");
    assert_eq!(
        deck.source.as_deref(),
        Some(gcg_data::py_path_str(&path).as_str())
    );
    assert_eq!(
        counter(&deck.main),
        BTreeMap::from([("GD01-008".to_owned(), 4)])
    );
    let _ = std::fs::remove_dir_all(&dir);
}

fn expanded(list: &[CardCount]) -> Vec<String> {
    list.iter()
        .flat_map(|c| {
            std::iter::repeat_n(
                c.card_number.to_string(),
                usize::try_from(c.count).expect("small"),
            )
        })
        .collect()
}

fn numbers_of(ids: &[DefId]) -> Vec<String> {
    ids.iter()
        .map(|&i| db().by_id(i).card_number.to_string())
        .collect()
}

#[test]
fn to_decklist_is_the_main_deck_and_the_resources() {
    let deck = parse_deck(SAMPLE, "deck", None).expect("parses");
    let list = to_decklist(&deck).expect("a decklist");
    assert_eq!(numbers_of(&list.main), expanded(&deck.main));
    assert_eq!(numbers_of(&list.resources), expanded(&deck.resources));
}

#[test]
fn digest_ignores_order_comments_names_and_split_lines() {
    let a = parse_deck("4 GD01-008 Guntank\n2 GD01-001\n10 R-001\n", "a", None).expect("a");
    let b = parse_deck(
        "# x\n2 GD01-001_p1 Gundam\n2 GD01-008\n2 GD01-008\n10 R-001\n",
        "b",
        None,
    )
    .expect("b");
    let c = parse_deck("3 GD01-008\n2 GD01-001\n10 R-001\n", "deck", None).expect("c");
    assert_eq!(deck_digest(&a), deck_digest(&b));
    assert_ne!(deck_digest(&a), deck_digest(&c));
    assert_eq!(deck_digest(&a).len(), 64);
}

/// The Hypothesis property with 60 seeded cases (`gcg_core::prop`): counts over the first 60
/// real Unit and Resource numbers, with or without names, between comment lines.
#[test]
fn round_trip_preserves_counts() {
    let mut pool: Vec<&str> = db()
        .real_cards()
        .filter(|c| matches!(c.card_type, CardType::Unit | CardType::Resource))
        .map(|c| &*c.card_number)
        .collect();
    pool.sort();
    pool.truncate(60);
    let comments = ["# c", "", "   ", "#"];
    gcg_core::prop::check("deck-round-trip", 60, |rng| {
        let mut counts: BTreeMap<&str, u128> = BTreeMap::new();
        for _ in 0..1 + rng.randrange_usize(12) {
            counts.insert(
                pool[rng.randrange_usize(pool.len())],
                1 + u128::from(rng.randrange(9)),
            );
        }
        let with_names = rng.randrange(2) == 1;
        let lines: Vec<String> = (0..rng.randrange_usize(5))
            .map(|_| comments[rng.randrange_usize(comments.len())].to_owned())
            .collect();
        let cards: Vec<String> = counts
            .iter()
            .map(|(n, c)| {
                if with_names {
                    format!("{c} {n} {}", name_of(n))
                } else {
                    format!("{c} {n}")
                }
            })
            .collect();
        let text = [lines.clone(), cards, lines].concat().join("\n");
        let deck = parse_deck(&text, "deck", None).map_err(|e| e.to_string())?;
        let mut got = counter(&deck.main);
        got.extend(counter(&deck.resources));
        let want: BTreeMap<String, u128> =
            counts.iter().map(|(n, c)| ((*n).to_owned(), *c)).collect();
        if got != want {
            return Err(format!("{text:?}: {got:?}"));
        }
        let resource = |n: &str| {
            db().get(n)
                .is_some_and(|c| c.card_type == CardType::Resource)
        };
        if !deck.resources.iter().all(|c| resource(&c.card_number))
            || deck.main.iter().any(|c| resource(&c.card_number))
        {
            return Err(format!("{text:?}: resources and main deck mixed up"));
        }
        Ok(())
    });
}

// --- test_validate.py -----------------------------------------------------------------------

#[test]
fn base_deck_is_legal() {
    assert_eq!(violations(&make_deck(&base_counts())), []);
}

// gcg-tags: rule=6-1-1 faq=Q1
#[test]
fn main_deck_must_have_exactly_50_cards() {
    for delta in [-1_i64, 1] {
        let deck = make_deck(&swap(&base_counts(), &[], &[("ST01-005", delta)]));
        let vs = violations(&deck);
        assert_eq!(
            vs.iter().map(|v| v.code.as_str()).collect::<Vec<_>>(),
            ["MAIN_SIZE"]
        );
        assert_eq!(
            vs[0].message,
            format!(
                "the deck has {} cards; it must have exactly 50 (rule 6-1-1)",
                50 + delta
            )
        );
    }
}

// gcg-tags: rule=6-1-1 faq=Q5
#[test]
fn resource_deck_must_have_exactly_10_cards() {
    for resources in [9, 11] {
        let deck = make_deck(&with(&base_counts(), "R-001", resources));
        let vs = violations(&deck);
        assert_eq!(
            vs.iter().map(|v| v.code.as_str()).collect::<Vec<_>>(),
            ["RESOURCE_SIZE"]
        );
        assert_eq!(
            vs[0].message,
            format!(
                "the resource deck has {resources} cards; it must have exactly 10 Resource cards \
                 (rule 6-1-1)"
            )
        );
    }
}

// gcg-tags: rule=6-1-1-3 rule=2-1-2 faq=Q3
#[test]
fn at_most_four_copies_per_card_number() {
    let deck = make_deck(&swap(
        &base_counts(),
        &[("ST01-005", 1)],
        &[("GD01-008", 1)],
    ));
    let vs = violations(&deck);
    assert_eq!(
        vs.iter().map(|v| v.code.as_str()).collect::<Vec<_>>(),
        ["COPY_LIMIT"]
    );
    assert_eq!(
        vs[0].message,
        "5 copies of GD01-008 Guntank; at most 4 copies of cards with the same card number (all \
         printings count together) may be in a deck (rules 6-1-1-3, 2-1-2)"
    );
    assert_eq!(&*vs[0].cards, [Box::from("GD01-008")]);
}

// gcg-tags: rule=6-1-1-3 rule=2-1-2
#[test]
fn alternate_printings_count_as_the_same_card() {
    let counts = swap(&base_counts(), &[("ST01-005", 3), ("GD01-001", 2)], &[]);
    let mut text: String = counts.iter().map(|(n, c)| format!("{c} {n}\n")).collect();
    text.push_str("3 GD01-001\n2 GD01-001_p1\n");
    let vs = violations(&parse_deck(&text, "deck", None).expect("parses"));
    assert_eq!(
        vs.iter().map(|v| v.code.as_str()).collect::<Vec<_>>(),
        ["COPY_LIMIT"]
    );
    assert!(
        vs[0].message.starts_with("5 copies of GD01-001 Gundam;"),
        "{}",
        vs[0].message
    );
}

// gcg-tags: rule=6-1-1-3 faq=Q4
#[test]
fn same_name_with_different_card_numbers_counts_separately() {
    let deck = make_deck(&swap(
        &base_counts(),
        &[("GD01-001", 2), ("GD04-008", 2)],
        &[("GD01-013", 4)],
    ));
    assert_eq!(name_of("ST01-001"), "Gundam");
    assert_eq!(name_of("GD01-013"), "Gundam");
    assert_eq!(copies(&deck.main, "ST01-001"), 4);
    assert_eq!(copies(&deck.main, "GD01-013"), 4);
    assert_eq!(violations(&deck), []);
}

// gcg-tags: rule=6-1-1-5 faq=Q6
#[test]
fn resource_deck_allows_any_number_of_one_card_number() {
    let deck = make_deck(&base_counts());
    assert_eq!(
        counter(&deck.resources),
        BTreeMap::from([("R-001".to_owned(), 10)])
    );
    assert_eq!(violations(&deck), []);
    let mixed = make_deck(&swap(&base_counts(), &[("R-001", 4)], &[("R-002", 4)]));
    assert_eq!(violations(&mixed), []);
}

// gcg-tags: rule=6-1-1-2 faq=Q2
#[test]
fn at_most_two_colors() {
    let deck = make_deck(&swap(
        &base_counts(),
        &[("ST01-005", 3)],
        &[("GD01-059", 3)],
    ));
    let vs = violations(&deck);
    assert_eq!(
        vs.iter().map(|v| v.code.as_str()).collect::<Vec<_>>(),
        ["COLORS"]
    );
    assert_eq!(
        vs[0].message,
        "the deck uses 3 colors (Blue x41, White x6, Red x3); a deck must be constructed entirely \
         using either one or two card colors (rule 6-1-1-2). Cards of the extra color(s): GD01-059 \
         Zee Zulu"
    );
    assert_eq!(&*vs[0].cards, [Box::from("GD01-059")]);
}

/// `mono_color_deck(colors)`: 50 cards of implemented, unrestricted main-deck numbers of
/// exactly `colors` (four copies each, taking the colors in turn), plus ten R-001.
fn mono_color_deck(colors: &[&str]) -> Deck {
    let bl = banlist();
    let mut avoid: Vec<&str> = bl.banned.iter().map(|n| &**n).collect();
    avoid.extend(bl.restricted.iter().map(|r| &*r.card_number));
    avoid.extend(
        bl.banned_pairs
            .iter()
            .flat_map(|p| p.cards.iter().map(|n| &**n)),
    );
    for rule in bl.attribute_rules.iter() {
        for c in db().real_cards() {
            if rule.enumerated.iter().any(|n| **n == *c.card_number)
                || matches(&rule.member_predicate, c, None)
            {
                avoid.push(&c.card_number);
            }
        }
    }
    let reg = registry();
    let pools: Vec<Vec<&str>> = colors
        .iter()
        .map(|color| {
            db().real_cards()
                .filter(|c| MAIN_DECK_TYPES.contains(&c.card_type))
                .filter(|c| c.color.is_some_and(|k| k.as_str() == *color))
                .filter(|c| !avoid.contains(&&*c.card_number))
                .filter(|c| reg.unimplemented_error(c.def_id).is_none())
                .map(|c| &*c.card_number)
                .collect()
        })
        .collect();
    let mut main: Vec<&str> = Vec::new();
    for i in 0..13 {
        for pool in &pools {
            main.extend([pool[i]; 4]);
        }
    }
    let mut text: String = main[..50].iter().map(|n| format!("1 {n}\n")).collect();
    text.push_str("10 R-001\n");
    parse_deck(&text, &colors.join("-"), None).expect("parses")
}

// gcg-tags: rule=6-1-1-2 rule=6-1-1-2-1
#[test]
fn one_or_two_colors_are_legal() {
    for colors in [
        &["Red"][..],
        &["Green", "White"],
        &["Blue"],
        &["Purple", "Red"],
    ] {
        let deck = mono_color_deck(colors);
        let mut used: Vec<&str> = deck
            .main
            .iter()
            .filter_map(|c| db().get(&c.card_number).and_then(|d| d.color))
            .map(|k| k.as_str())
            .collect();
        used.sort();
        used.dedup();
        let mut want = colors.to_vec();
        want.sort();
        assert_eq!(used, want);
        assert_eq!(violations(&deck), [], "{colors:?}");
    }
}

// gcg-tags: rule=6-1-1-1 rule=3-2-2 rule=3-3-2 rule=3-4-2 rule=3-5-2 rule=3-6-2 rule=6-1-1-4 faq=Q2 faq=Q6
#[test]
fn card_types_route_to_the_deck_they_compose() {
    let deck = make_deck(&base_counts());
    let mut main_types: Vec<CardType> = deck
        .main
        .iter()
        .map(|c| db().get(&c.card_number).expect("card").card_type)
        .collect();
    main_types.sort_by_key(|t| *t as u8);
    main_types.dedup();
    let mut want = MAIN_DECK_TYPES.to_vec();
    want.sort_by_key(|t| *t as u8);
    assert_eq!(main_types, want);
    assert!(
        deck.resources
            .iter()
            .all(|c| db().get(&c.card_number).expect("card").card_type == CardType::Resource)
    );
    assert!(!codes(&deck).contains(&"CARD_TYPE"));
}

// gcg-tags: rule=6-1-1-1 faq=Q7
#[test]
fn tokens_are_not_part_of_the_deck() {
    let deck = make_deck(&swap(
        &base_counts(),
        &[("ST01-005", 3)],
        &[("T-001", 2), ("EXB-001", 1)],
    ));
    let vs = violations(&deck);
    assert_eq!(
        vs.iter().map(|v| v.code.as_str()).collect::<Vec<_>>(),
        ["CARD_TYPE", "CARD_TYPE"]
    );
    assert_eq!(
        vs.iter().map(|v| v.message.as_str()).collect::<Vec<_>>(),
        [
            "EXB-001 EX Base is a token (EX BASE); tokens are prepared outside the game and are \
             never part of a deck or resource deck (rule 6-1-2, FAQ Q7)",
            "T-001 Gundam is a token (UNIT TOKEN); tokens are prepared outside the game and are \
             never part of a deck or resource deck (rule 6-1-2, FAQ Q7)",
        ]
    );
}

fn replace_first(list: &[CardCount], number: &str) -> Box<[CardCount]> {
    // `(number, *list[1:])` over the expanded tuple, as a sorted count list.
    let mut items = expanded(list);
    items[0] = number.to_owned();
    items.sort();
    let mut out: Vec<CardCount> = Vec::new();
    for n in items {
        match out.last_mut() {
            Some(last) if *last.card_number == *n => last.count += 1,
            _ => out.push(CardCount {
                card_number: n.into(),
                count: 1,
            }),
        }
    }
    out.into()
}

// gcg-tags: rule=6-1-1-4 faq=Q6 faq=Q7
#[test]
fn resource_deck_holds_only_resource_cards() {
    let deck = make_deck(&swap(&base_counts(), &[("R-001", 1)], &[("EXR-001", 1)]));
    assert_eq!(
        only(&deck, "CARD_TYPE"),
        [
            "EXR-001 EX Resource is a token (EX RESOURCE); tokens are prepared outside the game \
             and are never part of a deck or resource deck (rule 6-1-2, FAQ Q7)"
        ]
    );
    let base = make_deck(&base_counts());
    let unit_in_resources = Deck {
        resources: replace_first(&base.resources, "GD01-008"),
        ..base
    };
    assert_eq!(
        only(&unit_in_resources, "CARD_TYPE"),
        [
            "GD01-008 Guntank is a UNIT card; a resource deck is constructed with Resource cards \
          only (rule 6-1-1-4)"
        ]
    );
}

// gcg-tags: rule=6-1-1-1 faq=Q2
#[test]
fn main_deck_holds_only_unit_pilot_command_base() {
    let base = make_deck(&base_counts());
    let resource_in_main = Deck {
        main: replace_first(&base.main, "R-001"),
        ..base
    };
    assert_eq!(
        only(&resource_in_main, "CARD_TYPE"),
        [
            "R-001 Resource is a RESOURCE card; a deck is constructed with Unit, Pilot, Command, \
             and Base cards only (rule 6-1-1-1)"
        ]
    );
}

#[test]
fn banned_card() {
    let deck = make_deck(&swap(
        &base_counts(),
        &[("GD01-001", 2)],
        &[("GD01-020", 2)],
    ));
    let vs = violations(&deck);
    assert_eq!(
        vs.iter().map(|v| v.code.as_str()).collect::<Vec<_>>(),
        ["BANNED"]
    );
    assert_eq!(
        vs[0].message,
        format!(
            "GD01-020 Anksha is banned: no copies may be in a deck (Banned & Restricted list \
             effective {BL_DATE})"
        )
    );
    assert_eq!(codes_with(&deck, false, true), Vec::<&str>::new());
}

#[test]
fn restricted_card() {
    let deck = make_deck(&swap(
        &base_counts(),
        &[("ST08-015", 2), ("GD04-122", 1)],
        &[("ST02-016", 3)],
    ));
    assert_eq!(codes_with(&deck, true, false), ["RESTRICTED"]);
    assert_eq!(
        only(&deck, "RESTRICTED"),
        [format!(
            "3 copies of ST02-016 Corsica Base; it is restricted to 2 copies (Banned & Restricted \
             list effective {BL_DATE}). Exception 'st02-unmodified-starter-deck' {NOT_APPLIED}."
        )]
    );
    let two = make_deck(&swap(
        &base_counts(),
        &[("ST08-015", 2)],
        &[("ST02-016", 2)],
    ));
    assert!(!codes(&two).contains(&"RESTRICTED"));
}

#[test]
fn banned_pairs() {
    for (remove, add, message) in [
        (
            ("ST01-005", 3),
            ("GD05-015", 3),
            format!(
                "GD01-008 Guntank, GD05-015 M1 Astray Shrike are a banned pair and cannot be in \
                 the same deck (Banned & Restricted list effective {BL_DATE})"
            ),
        ),
        (
            ("GD04-084", 2),
            ("ST05-010", 2),
            format!(
                "ST01-010 Amuro Ray, ST05-010 Mikazuki Augus are a banned pair and cannot be in \
                 the same deck (Banned & Restricted list effective {BL_DATE})"
            ),
        ),
    ] {
        let deck = make_deck(&swap(&base_counts(), &[remove], &[add]));
        assert_eq!(only(&deck, "BANNED_PAIR"), [message]);
        assert!(!codes(&make_deck(&base_counts())).contains(&"BANNED_PAIR"));
    }
}

#[test]
fn attribute_pair() {
    let deck = make_deck(&swap(
        &base_counts(),
        &[("ST01-005", 1)],
        &[("GD05-014", 1)],
    ));
    let vs = violations(&deck);
    assert_eq!(
        vs.iter().map(|v| v.code.as_str()).collect::<Vec<_>>(),
        ["ATTRIBUTE_PAIR"]
    );
    assert_eq!(
        vs[0].message,
        format!(
            "GD05-014 Javelin, ST01-005 GM are each \"{VANILLA}\"; every combination of two such \
             card numbers is a banned pair, so a deck may contain only one of them, at most 4 \
             copies (Banned & Restricted list effective {BL_DATE}). Exception \
             'st05-unmodified-starter-deck' {NOT_APPLIED}."
        )
    );
    assert_eq!(
        &*vs[0].cards,
        [Box::from("GD05-014"), Box::from("ST01-005")]
    );
}

#[test]
fn attribute_pair_starter_deck_exception_is_not_applied() {
    let deck = make_deck(&swap(
        &base_counts(),
        &[("ST01-005", 3)],
        &[("ST05-004", 2), ("ST05-009", 1)],
    ));
    let messages = only(&deck, "ATTRIBUTE_PAIR");
    assert_eq!(messages.len(), 1);
    assert!(messages[0].starts_with("ST05-004 Graze Custom, ST05-009 Graze are each"));
    assert!(messages[0].contains(&format!(
        "Exception 'st05-unmodified-starter-deck' {NOT_APPLIED}."
    )));
}

#[test]
fn one_attribute_card_number_with_four_copies_is_legal() {
    let deck = make_deck(&swap(
        &base_counts(),
        &[("ST01-004", 1)],
        &[("ST01-005", 1)],
    ));
    assert_eq!(copies(&deck.main, "ST01-005"), 4);
    assert_eq!(violations(&deck), []);
}

#[test]
fn attribute_predicate_matches_the_enumerated_list() {
    let rules = &banlist().attribute_rules;
    assert_eq!(rules.len(), 1);
    let rule = &rules[0];
    let mut by_predicate: Vec<&str> = db()
        .real_cards()
        .filter(|c| matches(&rule.member_predicate, c, None))
        .map(|c| &*c.card_number)
        .collect();
    by_predicate.sort();
    let mut enumerated: Vec<&str> = rule.enumerated.iter().map(|n| &**n).collect();
    enumerated.sort();
    assert_eq!(by_predicate, enumerated);
    assert_eq!(by_predicate.len(), 22);
}

#[test]
fn unknown_id() {
    let base = make_deck(&base_counts());
    let mut main = expanded(&base.main);
    let last = main.len() - 1;
    main[last] = "GD99-999".to_owned();
    let mut runs: Vec<CardCount> = Vec::new();
    for n in main {
        match runs.last_mut() {
            Some(r) if *r.card_number == *n => r.count += 1,
            _ => runs.push(CardCount {
                card_number: n.into(),
                count: 1,
            }),
        }
    }
    let deck = Deck {
        main: runs.into(),
        ..base
    };
    let vs = violations(&deck);
    assert_eq!(
        vs.iter().map(|v| v.code.as_str()).collect::<Vec<_>>(),
        ["UNKNOWN_ID"]
    );
    assert_eq!(
        vs[0].message,
        "unknown card id 'GD99-999' (not in the card data)"
    );
}

#[test]
fn name_mismatch() {
    let base = make_deck(&base_counts());
    let deck = Deck {
        entries: Box::new([DeckEntry {
            count: 4,
            card_number: "GD01-008".into(),
            written: "GD01-008".into(),
            name: Some("Gundam".into()),
            line: 3,
        }]),
        ..base
    };
    assert_eq!(
        only(&deck, "NAME_MISMATCH"),
        ["line 3: GD01-008 is 'Guntank' in the card data, not 'Gundam'"]
    );
}

/// The monkeypatched registry of `test_unimplemented_card`: GD01-008 has no script, with the
/// error `not compiled`.
struct OneUnimplemented(DefId);

impl ImplementationStatus for OneUnimplemented {
    fn unimplemented_error(&self, def: DefId) -> Option<&str> {
        (def == self.0).then_some("not compiled")
    }
}

#[test]
fn unimplemented_card() {
    let status = OneUnimplemented(db().get("GD01-008").expect("card").def_id);
    let deck = make_deck(&base_counts());
    let vs = validate(
        &deck,
        ValidateOptions {
            banlist: true,
            implemented: Some(&status),
        },
    );
    assert_eq!(
        vs.iter().map(|v| v.code.as_str()).collect::<Vec<_>>(),
        ["UNIMPLEMENTED"]
    );
    assert_eq!(
        vs[0].message,
        "GD01-008 Guntank has no implemented effect in this simulator version, so games cannot \
         use it (not compiled)"
    );
    assert_eq!(codes_with(&deck, true, false), Vec::<&str>::new());
}

#[test]
fn require_legal_lists_every_violation() {
    let reg = registry();
    require_legal(&make_deck(&base_counts()), reg.as_ref()).expect("legal");
    let bad = make_deck(&swap(
        &base_counts(),
        &[("ST01-005", 3), ("GD01-001", 2)],
        &[("GD01-059", 1)],
    ));
    let (problems, violations, message) =
        invalid(require_legal(&bad, reg.as_ref()).expect_err("illegal"));
    assert_eq!(
        violations
            .iter()
            .map(|v| v.code.as_str())
            .collect::<Vec<_>>(),
        ["MAIN_SIZE", "COLORS"]
    );
    assert!(problems[0].starts_with("[MAIN_SIZE] the deck has 46 cards"));
    assert!(problems[1].starts_with("[COLORS] the deck uses 3 colors"));
    assert_eq!(
        message.lines().next(),
        Some("deck 'test-deck' has 2 problem(s):")
    );
}
