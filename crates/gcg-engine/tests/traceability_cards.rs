//! Native Rust versions of rule tests that read compiled card scripts without running the engine, and
//! of the load-time card coverage test (PLAN §9.2, §9.3).
//!
//! The per-test trace plugin (`oracle/pytest_trace.py`) records only engine calls, so these
//! tests are `untraced` in `oracle/corpus/v1/tier1/tests/MANIFEST-tests.json`:
//!
//! - `tests/rules/test_effects.py::test_effects_divide_into_five_types` (rule 10-1-4);
//! - `tests/rules/test_pairing.py::test_pilot_text_splits_into_pilot_text_and_unit_text`
//!   (rules 3-3-9, 3-3-9-1, 3-3-9-2, 2-11-3).
//!
//! The coverage tests reproduce `tests/effects/test_card_coverage.py::test_every_card_is_implemented`
//! and `::test_vanilla_cards_have_no_abilities`: every one of the 1152 real card numbers (1153
//! definitions) has a script in the embedded IR, and no definition reports an unimplemented
//! effect to deck validation.

use std::sync::{Arc, OnceLock};

use gcg_data::{CardDb, ImplementationStatus, SYNTHETIC_PREFIX};
use gcg_effects::{AbilityNodeId, CardScript, Node, Registry, ScriptSource};

fn registry() -> &'static Registry {
    static REG: OnceLock<Arc<Registry>> = OnceLock::new();
    REG.get_or_init(Registry::embedded)
}

fn db() -> &'static CardDb {
    registry().db()
}

/// `reg.cards[reg.db[number].def_id].script`.
fn script(number: &str) -> &'static CardScript {
    let def = db()
        .get(number)
        .unwrap_or_else(|| panic!("{number} is in the card data"))
        .def_id;
    let node = registry()
        .card(def)
        .script
        .unwrap_or_else(|| panic!("{number} has a script"));
    match registry().node(node) {
        Node::CardScript(s) => s,
        other => panic!("{number}: the script node is {other:?}"),
    }
}

fn ability(a: AbilityNodeId) -> &'static Node {
    registry().node(a.node())
}

/// `type(a).__name__` of an `Ability` (`effects/dsl.py:1373-1383`).
fn kind(a: AbilityNodeId) -> &'static str {
    match ability(a) {
        Node::Keyword(_) => "Keyword",
        Node::Constant(_) => "Constant",
        Node::Triggered(_) => "Triggered",
        Node::Activated(_) => "Activated",
        Node::Command(_) => "Command",
        Node::Burst(_) => "Burst",
        Node::Replacement(_) => "Replacement",
        Node::NameAlias(_) => "NameAlias",
        Node::PlayModifier(_) => "PlayModifier",
        other => panic!("not an Ability node: {other:?}"),
    }
}

/// `_abilities(number)` (`tests/rules/test_effects.py`): `script.abilities +
/// script.unit_abilities`.
fn abilities(number: &str) -> Vec<AbilityNodeId> {
    let s = script(number);
    s.abilities
        .iter()
        .chain(s.unit_abilities.iter())
        .copied()
        .collect()
}

fn kinds(number: &str) -> Vec<&'static str> {
    abilities(number).into_iter().map(kind).collect()
}

const REPAIR_AURA: &str = "GD01-001";
const TURN_A: &str = "GD04-073";
const AIRFRAME_SEIZURE: &str = "GD05-111";
const SETSUNA: &str = "ST07-009";

// gcg-tags: rule=10-1-4
#[test]
fn effects_divide_into_five_types() {
    assert_eq!(kinds(REPAIR_AURA), ["Constant", "Triggered"]);
    assert_eq!(kinds(TURN_A), ["Activated"]);
    assert_eq!(kinds(AIRFRAME_SEIZURE), ["Command"]);
    let triggered: Vec<&gcg_effects::Triggered> = abilities(SETSUNA)
        .into_iter()
        .filter_map(|a| match ability(a) {
            Node::Triggered(t) => Some(t),
            _ => None,
        })
        .collect();
    let [attack_effect] = triggered[..] else {
        panic!("{SETSUNA} has one Triggered ability: {triggered:?}");
    };
    // "... all your (CB) Units get AP+1 instead."
    let [substitution] = attack_effect.steps[..] else {
        panic!("{SETSUNA}: the attack effect has one step: {attack_effect:?}");
    };
    let Node::If(substitution) = registry().node(substitution.node()) else {
        panic!("{SETSUNA}: the step is not an If");
    };
    assert!(!substitution.then.is_empty());
    assert!(!substitution.otherwise.is_empty());
}

const CAGALLI: &str = "GD01-096";
const HEERO: &str = "ST02-010";
const AMURO: &str = "ST01-010";
const BANAGHER: &str = "GD01-088";

// gcg-tags: rule=3-3-9 rule=3-3-9-1 rule=3-3-9-2 rule=2-11-3
#[test]
fn pilot_text_splits_into_pilot_text_and_unit_text() {
    for number in [CAGALLI, HEERO, AMURO, BANAGHER] {
        let s = script(number);
        let own: Vec<&str> = s.abilities.iter().map(|a| kind(*a)).collect();
        assert_eq!(own, ["Burst"], "{number}");
        assert!(!s.unit_abilities.is_empty(), "{number}");
        assert!(
            s.unit_abilities.iter().all(|a| kind(*a) != "Burst"),
            "{number}"
        );
    }
}

// --- tests/effects/test_card_coverage.py ----------------------------------------------------------

#[test]
fn every_card_is_implemented() {
    let reg = registry();
    let missing: Vec<String> = db()
        .defs()
        .iter()
        .filter(|d| reg.card(d.def_id).script.is_none())
        .filter(|d| !d.card_number.starts_with(SYNTHETIC_PREFIX))
        .map(|d| d.card_number.to_string())
        .collect();
    assert!(
        missing.is_empty(),
        "{} unimplemented cards, e.g. {:?}",
        missing.len(),
        &missing[..missing.len().min(10)]
    );
    // The Rust IR holds a script for every definition: the 1152 real card numbers and the
    // Unit variant.
    assert_eq!(db().len(), 1153);
    assert_eq!(db().real_cards().count(), 1152);
    let scripted = db()
        .defs()
        .iter()
        .filter(|d| reg.card(d.def_id).script.is_some())
        .count();
    assert_eq!(scripted, 1153);
    let unimplemented: Vec<&str> = db()
        .defs()
        .iter()
        .filter(|d| reg.unimplemented_error(d.def_id).is_some())
        .map(|d| &*d.card_number)
        .collect();
    assert_eq!(unimplemented, Vec::<&str>::new());
}

#[test]
fn vanilla_cards_have_no_abilities() {
    let reg = registry();
    let mut vanilla = 0;
    for c in db().real_cards() {
        if reg.card(c.def_id).source != ScriptSource::Vanilla {
            continue;
        }
        vanilla += 1;
        let s = script(&c.card_number);
        assert!(s.abilities.is_empty(), "{}", c.card_number);
        assert!(s.unit_abilities.is_empty(), "{}", c.card_number);
    }
    assert_eq!(vanilla, 312, "Python parametrises 312 vanilla real cards");
}
