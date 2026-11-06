//! Native Rust versions of the tagged deck-construction tests (PLAN §9.2). `tests/deck` is outside the
//! per-test trace suites (`oracle/pytest_trace.py` traces `tests/rules`, `tests/cards`,
//! `tests/effects` and `tests/ai`), and these tests make no engine call, so only a native test
//! gives the Rust implementation evidence for their rules and FAQ entries.
//!
//! - `tests/deck/test_validate.py`: the tests tagged with rules 6-1-1-x, 3-x-2 and 2-1-2 and FAQ
//!   Q1-Q7, each asserting its exact message;
//! - `tests/deck/test_examples.py::test_example_deck_is_legal_and_implemented`.
//!
//! `validate_deck(deck)` defaults to `implemented=True`, which reads the effect registry; that
//! is why these tests live in a crate that depends on `gcg-effects` (`gcg_data::validate` takes
//! the registry as its `ImplementationStatus`).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use gcg_data::official;
use gcg_data::{
    CardCount, CardDb, CardType, Color, Deck, MAIN_DECK_TYPES, ValidateOptions, Violation,
    ViolationCode, deck_digest, load_deck, parse_deck, validate,
};
use gcg_effects::Registry;

fn examples() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/decks")
}

fn registry() -> &'static Registry {
    static REG: OnceLock<Arc<Registry>> = OnceLock::new();
    REG.get_or_init(Registry::embedded)
}

fn db() -> &'static CardDb {
    CardDb::embedded()
}

/// `validate_deck(deck)`: `banlist=True, implemented=True`.
fn validate_deck(deck: &Deck) -> Vec<Violation> {
    validate(
        deck,
        ValidateOptions {
            banlist: true,
            implemented: Some(registry()),
        },
    )
}

fn codes(deck: &Deck) -> Vec<ViolationCode> {
    validate_deck(deck).iter().map(|v| v.code).collect()
}

fn only(deck: &Deck, code: ViolationCode) -> Vec<String> {
    validate_deck(deck)
        .into_iter()
        .filter(|v| v.code == code)
        .map(|v| v.message)
        .collect()
}

fn cards(v: &Violation) -> Vec<&str> {
    v.cards.iter().map(|c| &**c).collect()
}

/// Card counts in insertion order, as the Python `dict[str, int]` of `conftest.py`.
type Counts = Vec<(String, i64)>;

/// `base_counts`: `dict(Counter(deck.main + deck.resources))` of the Blue/White example deck.
fn base_counts() -> Counts {
    let path = examples().join("blue-white-federation.txt");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path:?}: {e}"));
    let deck = parse_deck(&text, "deck", None).unwrap_or_else(|e| panic!("{e}"));
    deck.main
        .iter()
        .chain(deck.resources.iter())
        .map(|c| {
            let n = i64::try_from(c.count).unwrap_or_else(|e| panic!("{e}"));
            (c.card_number.to_string(), n)
        })
        .collect()
}

fn set(counts: &mut Counts, number: &str, count: i64) {
    match counts.iter_mut().find(|(n, _)| n == number) {
        Some(slot) => slot.1 = count,
        None => counts.push((number.to_owned(), count)),
    }
}

fn count_of(counts: &Counts, number: &str) -> i64 {
    counts
        .iter()
        .find(|(n, _)| n == number)
        .map_or(0, |(_, c)| *c)
}

/// `swap(counts, remove, add)` (`test_validate.py`): drops numbers left at zero copies.
fn swap(counts: &Counts, remove: &[(&str, i64)], add: &[(&str, i64)]) -> Counts {
    let mut out = counts.clone();
    for (n, c) in remove {
        let now = count_of(&out, n);
        set(&mut out, n, now - c);
    }
    for (n, c) in add {
        let now = count_of(&out, n);
        set(&mut out, n, now + c);
    }
    out.retain(|(_, c)| *c > 0);
    out
}

/// `make_deck(counts)`: `parse_deck(deck_text(counts), name="test-deck")` (`conftest.py`).
fn make_deck(counts: &Counts) -> Deck {
    let text = counts
        .iter()
        .filter(|(_, c)| *c > 0)
        .map(|(n, c)| format!("{c} {n}"))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    parse_deck(&text, "test-deck", None).unwrap_or_else(|e| panic!("{e}"))
}

/// The copies of `number` in `cards` (`deck.main.count(number)`).
fn copies(cards: &[CardCount], number: &str) -> u128 {
    cards
        .iter()
        .find(|c| &*c.card_number == number)
        .map_or(0, |c| c.count)
}

/// One card number per copy, in order (Python's `Deck.main` / `Deck.resources` tuples).
fn expand(cards: &[CardCount]) -> Vec<Box<str>> {
    cards
        .iter()
        .flat_map(|c| {
            let n = usize::try_from(c.count).unwrap_or_else(|e| panic!("{e}"));
            std::iter::repeat_n(c.card_number.clone(), n)
        })
        .collect()
}

/// The run-length form of a sorted tuple of card numbers.
fn rle(numbers: &[Box<str>]) -> Box<[CardCount]> {
    let mut out: Vec<CardCount> = Vec::new();
    for n in numbers {
        match out.last_mut() {
            Some(last) if last.card_number == *n => last.count += 1,
            _ => out.push(CardCount {
                card_number: n.clone(),
                count: 1,
            }),
        }
    }
    out.into()
}

/// `dataclasses.replace(deck, main=..., resources=...)` with sorted tuples.
fn replaced(base: &Deck, main: &[Box<str>], resources: &[Box<str>]) -> Deck {
    let mut deck = Deck {
        main: rle(main),
        resources: rle(resources),
        ..base.clone()
    };
    deck.digest = deck_digest(&deck);
    deck
}

fn card_type(number: &str) -> CardType {
    db().get(number)
        .unwrap_or_else(|| panic!("{number} is in the card data"))
        .card_type
}

#[test]
fn base_deck_is_legal() {
    assert_eq!(validate_deck(&make_deck(&base_counts())), []);
}

// --- MAIN_SIZE / RESOURCE_SIZE ----------------------------------------------------------------

// gcg-tags: rule=6-1-1 faq=Q1
#[test]
fn main_deck_must_have_exactly_50_cards() {
    for delta in [-1, 1] {
        let deck = make_deck(&swap(&base_counts(), &[], &[("ST01-005", delta)]));
        let violations = validate_deck(&deck);
        assert_eq!(
            violations.iter().map(|v| v.code).collect::<Vec<_>>(),
            [ViolationCode::MainSize],
            "delta {delta}"
        );
        assert_eq!(
            violations[0].message,
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
        let mut counts = base_counts();
        set(&mut counts, "R-001", resources);
        let violations = validate_deck(&make_deck(&counts));
        assert_eq!(
            violations.iter().map(|v| v.code).collect::<Vec<_>>(),
            [ViolationCode::ResourceSize],
            "{resources} resources"
        );
        assert_eq!(
            violations[0].message,
            format!(
                "the resource deck has {resources} cards; it must have exactly 10 Resource cards \
                 (rule 6-1-1)"
            )
        );
    }
}

// --- COPY_LIMIT -------------------------------------------------------------------------------

// gcg-tags: rule=6-1-1-3 rule=2-1-2 faq=Q3
#[test]
fn at_most_four_copies_per_card_number() {
    let deck = make_deck(&swap(
        &base_counts(),
        &[("ST01-005", 1)],
        &[("GD01-008", 1)],
    ));
    let violations = validate_deck(&deck);
    assert_eq!(
        violations.iter().map(|v| v.code).collect::<Vec<_>>(),
        [ViolationCode::CopyLimit]
    );
    assert_eq!(
        violations[0].message,
        "5 copies of GD01-008 Guntank; at most 4 copies of cards with the same card number \
         (all printings count together) may be in a deck (rules 6-1-1-3, 2-1-2)"
    );
    assert_eq!(cards(&violations[0]), ["GD01-008"]);
}

// gcg-tags: rule=6-1-1-3 rule=2-1-2
#[test]
fn alternate_printings_count_as_the_same_card() {
    let counts = swap(&base_counts(), &[("ST01-005", 3), ("GD01-001", 2)], &[]);
    let text = counts
        .iter()
        .map(|(n, c)| format!("{c} {n}\n"))
        .collect::<String>()
        + "3 GD01-001\n2 GD01-001_p1\n";
    let deck = parse_deck(&text, "deck", None).unwrap_or_else(|e| panic!("{e}"));
    let violations = validate_deck(&deck);
    assert_eq!(
        violations.iter().map(|v| v.code).collect::<Vec<_>>(),
        [ViolationCode::CopyLimit]
    );
    assert!(
        violations[0]
            .message
            .starts_with("5 copies of GD01-001 Gundam;"),
        "{}",
        violations[0].message
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
    let names: BTreeSet<&str> = ["ST01-001", "GD01-013"]
        .iter()
        .map(|n| &*db().get(n).unwrap_or_else(|| panic!("{n}")).name)
        .collect();
    assert_eq!(names, BTreeSet::from(["Gundam"]));
    assert_eq!(copies(&deck.main, "ST01-001"), 4);
    assert_eq!(copies(&deck.main, "GD01-013"), 4);
    assert_eq!(validate_deck(&deck), []);
}

// gcg-tags: rule=6-1-1-5 faq=Q6
#[test]
fn resource_deck_allows_any_number_of_one_card_number() {
    let deck = make_deck(&base_counts());
    assert_eq!(expand(&deck.resources), vec![Box::<str>::from("R-001"); 10]);
    assert_eq!(validate_deck(&deck), []);
    let mixed = make_deck(&swap(&base_counts(), &[("R-001", 4)], &[("R-002", 4)]));
    assert_eq!(copies(&mixed.resources, "R-001"), 6);
    assert_eq!(copies(&mixed.resources, "R-002"), 4);
    assert_eq!(validate_deck(&mixed), []);
}

// --- COLORS -----------------------------------------------------------------------------------

// gcg-tags: rule=6-1-1-2 faq=Q2
#[test]
fn at_most_two_colors() {
    let deck = make_deck(&swap(
        &base_counts(),
        &[("ST01-005", 3)],
        &[("GD01-059", 3)],
    ));
    let violations = validate_deck(&deck);
    assert_eq!(
        violations.iter().map(|v| v.code).collect::<Vec<_>>(),
        [ViolationCode::Colors]
    );
    assert_eq!(
        violations[0].message,
        "the deck uses 3 colors (Blue x41, White x6, Red x3); a deck must be constructed \
         entirely using either one or two card colors (rule 6-1-1-2). Cards of the extra \
         color(s): GD01-059 Zee Zulu"
    );
    assert_eq!(cards(&violations[0]), ["GD01-059"]);
}

/// `mono_color_deck(colors)` (`test_validate.py`): a 50-card deck of implemented, unrestricted
/// cards of exactly `colors`, 4 copies of the first 13 eligible numbers of each colour in turn.
fn mono_color_deck(colors: &[Color]) -> Deck {
    let bl = official::banlist();
    let mut avoid: BTreeSet<&str> = bl.banned.iter().map(|n| &**n).collect();
    avoid.extend(bl.restricted.iter().map(|r| &*r.card_number));
    avoid.extend(
        bl.banned_pairs
            .iter()
            .flat_map(|p| p.cards.iter().map(|n| &**n)),
    );
    for rule in &*bl.attribute_rules {
        avoid.extend(
            db().real_cards()
                .filter(|c| {
                    rule.enumerated.iter().any(|e| **e == *c.card_number)
                        || official::matches(&rule.member_predicate, c, None)
                })
                .map(|c| &*c.card_number),
        );
    }
    let pools: Vec<Vec<&str>> = colors
        .iter()
        .map(|color| {
            db().real_cards()
                .filter(|c| {
                    MAIN_DECK_TYPES.contains(&c.card_type)
                        && c.color == Some(*color)
                        && !avoid.contains(&*c.card_number)
                        && registry().card(c.def_id).script.is_some()
                })
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
    let text = main[..50]
        .iter()
        .map(|n| format!("1 {n}"))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n10 R-001\n";
    let name = colors
        .iter()
        .map(|c| c.as_str())
        .collect::<Vec<_>>()
        .join("-");
    parse_deck(&text, &name, None).unwrap_or_else(|e| panic!("{e}"))
}

// gcg-tags: rule=6-1-1-2 rule=6-1-1-2-1
#[test]
fn one_or_two_colors_are_legal() {
    for colors in [
        &[Color::Red][..],
        &[Color::Green, Color::White],
        &[Color::Blue],
        &[Color::Purple, Color::Red],
    ] {
        let deck = mono_color_deck(colors);
        let used: BTreeSet<Color> = deck
            .main
            .iter()
            .filter_map(|c| db().get(&c.card_number).and_then(|d| d.color))
            .collect();
        assert_eq!(
            used,
            colors.iter().copied().collect::<BTreeSet<_>>(),
            "{colors:?}"
        );
        assert_eq!(validate_deck(&deck), [], "{colors:?}");
    }
}

// --- CARD_TYPE --------------------------------------------------------------------------------

// gcg-tags: rule=6-1-1-1 rule=3-2-2 rule=3-3-2 rule=3-4-2 rule=3-5-2 rule=3-6-2 rule=6-1-1-4 faq=Q2 faq=Q6
#[test]
fn card_types_route_to_the_deck_they_compose() {
    let deck = make_deck(&base_counts());
    let main: BTreeSet<CardType> = deck
        .main
        .iter()
        .map(|c| card_type(&c.card_number))
        .collect();
    assert_eq!(main, MAIN_DECK_TYPES.into_iter().collect::<BTreeSet<_>>());
    let resources: BTreeSet<CardType> = deck
        .resources
        .iter()
        .map(|c| card_type(&c.card_number))
        .collect();
    assert_eq!(resources, BTreeSet::from([CardType::Resource]));
    assert!(!codes(&deck).contains(&ViolationCode::CardType));
}

// gcg-tags: rule=6-1-1-1 faq=Q7
#[test]
fn tokens_are_not_part_of_the_deck() {
    let deck = make_deck(&swap(
        &base_counts(),
        &[("ST01-005", 3)],
        &[("T-001", 2), ("EXB-001", 1)],
    ));
    let violations = validate_deck(&deck);
    assert_eq!(
        violations.iter().map(|v| v.code).collect::<Vec<_>>(),
        [ViolationCode::CardType, ViolationCode::CardType]
    );
    assert_eq!(
        violations
            .iter()
            .map(|v| v.message.as_str())
            .collect::<Vec<_>>(),
        [
            "EXB-001 EX Base is a token (EX BASE); tokens are prepared outside the game and are \
             never part of a deck or resource deck (rule 6-1-2, FAQ Q7)",
            "T-001 Gundam is a token (UNIT TOKEN); tokens are prepared outside the game and are \
             never part of a deck or resource deck (rule 6-1-2, FAQ Q7)",
        ]
    );
}

// gcg-tags: rule=6-1-1-4 faq=Q6 faq=Q7
#[test]
fn resource_deck_holds_only_resource_cards() {
    let deck = make_deck(&swap(&base_counts(), &[("R-001", 1)], &[("EXR-001", 1)]));
    assert_eq!(
        only(&deck, ViolationCode::CardType),
        [
            "EXR-001 EX Resource is a token (EX RESOURCE); tokens are prepared outside the game \
             and are never part of a deck or resource deck (rule 6-1-2, FAQ Q7)"
        ]
    );
    let base = make_deck(&base_counts());
    let mut resources = expand(&base.resources);
    resources[0] = "GD01-008".into();
    let unit_in_resources = replaced(&base, &expand(&base.main), &resources);
    assert_eq!(
        only(&unit_in_resources, ViolationCode::CardType),
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
    let mut main = expand(&base.main);
    main[0] = "R-001".into();
    main.sort();
    let resource_in_main = replaced(&base, &main, &expand(&base.resources));
    assert_eq!(
        only(&resource_in_main, ViolationCode::CardType),
        [
            "R-001 Resource is a RESOURCE card; a deck is constructed with Unit, Pilot, Command, \
             and Base cards only (rule 6-1-1-1)"
        ]
    );
}

// --- tests/deck/test_examples.py ----------------------------------------------------------------

/// `sorted(EXAMPLES.glob("*.txt"))`.
fn example_decks() -> Vec<PathBuf> {
    let dir = examples();
    let mut out: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{dir:?}: {e}"))
        .map(|e| e.unwrap_or_else(|e| panic!("{dir:?}: {e}")).path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|x| x == "txt"))
        .collect();
    out.sort();
    out
}

// gcg-tags: rule=6-1-1 rule=6-1-1-1 rule=6-1-1-2 rule=6-1-1-3 rule=6-1-1-4 rule=6-1-1-5
#[test]
fn example_deck_is_legal_and_implemented() {
    let decks = example_decks();
    assert!(decks.len() >= 3, "{decks:?}");
    for path in decks {
        let deck = load_deck(&path).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(validate_deck(&deck), [], "{path:?}");
        let size = |cards: &[CardCount]| cards.iter().map(|c| c.count).sum::<u128>();
        assert_eq!(size(&deck.main), 50, "{path:?}");
        assert_eq!(size(&deck.resources), 10, "{path:?}");
    }
}
