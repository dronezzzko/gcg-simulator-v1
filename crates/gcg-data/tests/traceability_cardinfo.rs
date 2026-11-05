//! Native ports of the card-information rule tests that leave no engine trace (PLAN §9.2): the
//! per-test trace plugin (`port/oracle/pytest_trace.py`) records only engine calls, so these
//! data-only tests are listed as `untraced` in `port/corpus/v1/tier1/tests/MANIFEST-tests.json`
//! and need a native test for Rust evidence.
//!
//! - `tests/rules/test_cardinfo.py::test_there_are_five_card_types` (rule 2-3-2)
//! - `tests/rules/test_cardinfo.py::test_there_are_five_card_colors` (rule 2-4-2-1)
//! - `tests/rules/test_pairing.py::test_only_unit_cards_have_link_conditions` (rules 2-12-2,
//!   3-2-6)

use std::collections::BTreeSet;

use gcg_data::{CardDb, CardType, Color};
use serde_json::Value;

/// `load_raw_printings()` (`cards/db.py:57-59`): one JSON object per non-blank line of the
/// packaged `gcgapi/cards.ndjson`.
fn raw_printings() -> Vec<Value> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../src/gcg_sim/data/gcgapi/cards.ndjson"
    );
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("{path}: {e}")))
        .collect()
}

fn db() -> &'static CardDb {
    CardDb::embedded()
}

// gcg-tags: rule=2-3-2
#[test]
fn there_are_five_card_types() {
    let types: BTreeSet<CardType> = db()
        .real_cards()
        .filter(|c| !c.is_token())
        .map(|c| c.card_type)
        .collect();
    assert_eq!(
        types,
        BTreeSet::from([
            CardType::Unit,
            CardType::Pilot,
            CardType::Command,
            CardType::Base,
            CardType::Resource,
        ])
    );
    let mut tokens = 0;
    for token in db().real_cards().filter(|c| c.is_token()) {
        let t = token.card_type;
        let kinds = [t.is_unit(), t.is_base(), t.is_resource()];
        assert_eq!(
            kinds.iter().filter(|k| **k).count(),
            1,
            "{} ({}) is exactly one of a Unit, a Base or a Resource",
            token.card_number,
            t.as_str()
        );
        tokens += 1;
    }
    assert!(tokens > 0, "the card pool has tokens");
}

// gcg-tags: rule=2-4-2-1
#[test]
fn there_are_five_card_colors() {
    let colors: BTreeSet<Color> = db().real_cards().filter_map(|c| c.color).collect();
    assert_eq!(
        colors,
        BTreeSet::from([
            Color::Blue,
            Color::Green,
            Color::Red,
            Color::White,
            Color::Purple,
        ])
    );
}

const AMURO: &str = "ST01-010";
const KAIS_RESOLVE: &str = "ST01-013";
const DESERT_BASE: &str = "GD01-126";

// gcg-tags: rule=2-12-2 rule=3-2-6
#[test]
fn only_unit_cards_have_link_conditions() {
    let printings = raw_printings();
    let with_link: BTreeSet<&str> = printings
        .iter()
        .filter(|r| match &r["link"] {
            Value::Null => false,
            Value::String(s) => !s.is_empty() && s != "-",
            _ => true,
        })
        .map(|r| {
            r["card_type"]
                .as_str()
                .unwrap_or_else(|| panic!("card_type is not a string: {r}"))
        })
        .collect();
    assert_eq!(with_link, BTreeSet::from(["UNIT"]));
    let linked_types: BTreeSet<CardType> = db()
        .defs()
        .iter()
        .filter(|c| c.link.is_some())
        .map(|c| c.card_type)
        .collect();
    assert_eq!(linked_types, BTreeSet::from([CardType::Unit]));
    for number in [AMURO, KAIS_RESOLVE, DESERT_BASE] {
        let card = db()
            .get(number)
            .unwrap_or_else(|| panic!("{number} is in the card data"));
        assert_eq!(card.link, None, "{number} has no link condition");
    }
}
