//! T-057, first part: native ports of the 12 lead-example tests
//! (`tests/rules/test_lead_examples.py`, `tests/cards/test_lead_examples.py`), written against
//! `gcg_engine::testkit` as the Python tests are against `gcg_sim.testkit`. Each asserts the
//! Python test's outcomes and, in addition, that it ran Python's game: the state after
//! `Scenario.start` has the digest that the test's per-test trace
//! (`port/corpus/v1/tier1/tests/{rules,cards}/test_lead_examples.ndjson`) recorded, the test
//! applies as many actions as Python's, and its final state has Python's digest.

use std::fs;
use std::path::PathBuf;

use gcg_effects::{Kw, Registry};
use gcg_engine::canon::state_digest;
use gcg_engine::testkit::{
    attack, block, end_main, has_action, keywords, no, pass_all, play, select, to_next_turn, yes,
    zone_of,
};
use gcg_engine::{
    ActionKind, AddOptions, DecisionKind, EndReason, GameState, PLAYER_TARGET, Scenario,
    ScenarioOptions, Step, TestkitError, Zone,
};
use serde_json::Value;

const VANILLA_2_2: &str = "GD01-060"; // Zaku Mariner, Lv2 cost1 2/2, no effects
const BLOCKER: &str = "GD01-072"; // Launcher Strike Gundam, <Blocker>
const BURST_ADD: &str = "GD01-097"; // Guel Jeturk, 【Burst】Add this card to your hand.
const PILOT: &str = "GD01-089"; // Riddhe Marcenas, a Pilot card

fn digest16(st: &GameState) -> String {
    state_digest(st)[..16].to_owned()
}

/// What the Python test's trace recorded: the digest prefix after `Scenario.start` and after
/// each `apply` it made.
struct PythonRun {
    test: String,
    start16: String,
    applies: Vec<String>,
    actions_at_start: i64,
}

/// The Python run of `tests/<dir>/test_lead_examples.py::<name>`, checked against `st` right
/// after `Scenario.start`.
fn like_python(st: &GameState, dir: &str, name: &str) -> PythonRun {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../port/corpus/v1/tier1/tests")
        .join(dir)
        .join("test_lead_examples.ndjson");
    let test = format!("tests/{dir}/test_lead_examples.py::{name}");
    let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let mut inside = false;
    let mut start16 = None;
    let mut applies = Vec::new();
    for line in text.lines() {
        let rec: Value = serde_json::from_str(line).expect("a JSON record");
        match rec["t"].as_str().expect("a record type") {
            "header" => inside = rec["id"] == test.as_str(),
            "scenario" if inside => {
                assert!(start16.is_none(), "{test}: one Scenario");
                start16 = Some(rec["after16"].as_str().expect("after16").to_owned());
            }
            "apply" if inside => {
                assert!(rec["error"].is_null(), "{test}: {rec}");
                applies.push(rec["after16"].as_str().expect("after16").to_owned());
            }
            "end" if inside => assert_eq!(rec["outcome"], "passed", "{test}"),
            _ => {}
        }
    }
    let start16 = start16.unwrap_or_else(|| panic!("{test}: no trace"));
    assert_eq!(
        digest16(st),
        start16,
        "{test}: the state after Scenario.start"
    );
    PythonRun {
        test,
        start16,
        applies,
        actions_at_start: st.action_count(),
    }
}

impl PythonRun {
    /// The test applied Python's number of actions and ends in Python's state.
    fn ends_like_python(&self, st: &GameState) {
        let applied = usize::try_from(st.action_count() - self.actions_at_start).unwrap();
        assert_eq!(
            applied,
            self.applies.len(),
            "{}: actions applied",
            self.test
        );
        let last = self.applies.last().unwrap_or(&self.start16);
        assert_eq!(&digest16(st), last, "{}: the final state", self.test);
    }
}

fn scenario() -> Scenario {
    Scenario::new(&Registry::embedded(), ScenarioOptions::default())
}

fn unit(sc: &mut Scenario, player: u8, number: &str) -> u16 {
    sc.add(player, number, Zone::Battle, AddOptions::default())
        .unwrap()
}

fn pending_kind(st: &GameState) -> Option<DecisionKind> {
    st.pending().map(|d| d.hdr.kind)
}

// ---------------------------------------------------------------------------------------------
// tests/rules/test_lead_examples.py

// gcg-tags: rule=3-2-4 rule=3-2-6-3
#[test]
fn lead_newly_deployed_unit_cannot_attack() -> Result<(), TestkitError> {
    let mut sc = scenario();
    sc.resources(0, 3, 0, 0)?;
    let card = sc.add(0, VANILLA_2_2, Zone::Hand, AddOptions::default())?;
    sc.start(Step::Main)?;
    let st = sc.state_mut();
    let py = like_python(st, "rules", "test_newly_deployed_unit_cannot_attack");
    play(st, card, None, 0)?;
    assert_eq!(zone_of(st, card), Zone::Battle);
    assert!(!has_action(
        st,
        ActionKind::Attack,
        Some(i32::from(card)),
        None
    ));
    py.ends_like_python(st);
    Ok(())
}

// gcg-tags: rule=8-5-2-2 rule=1-2-2-1 rule=11-2-1-1
#[test]
fn lead_battle_damage_with_empty_shield_area_wins() -> Result<(), TestkitError> {
    let mut sc = scenario();
    let u = unit(&mut sc, 0, VANILLA_2_2);
    sc.start(Step::Main)?;
    let st = sc.state_mut();
    let py = like_python(
        st,
        "rules",
        "test_battle_damage_with_empty_shield_area_wins",
    );
    attack(st, u, PLAYER_TARGET)?;
    pass_all(st, 50)?;
    assert_eq!(st.winner(), Some(0));
    assert_eq!(st.end_reason(), Some(EndReason::BattleDamage));
    py.ends_like_python(st);
    Ok(())
}

// gcg-tags: rule=8-5-2-3 rule=8-5-2-3-1 rule=4-6-4-2
#[test]
fn lead_attack_destroys_top_shield() -> Result<(), TestkitError> {
    let mut sc = scenario();
    let u = unit(&mut sc, 0, VANILLA_2_2);
    let shields = sc.shields(1, &[VANILLA_2_2, VANILLA_2_2])?;
    sc.start(Step::Main)?;
    let st = sc.state_mut();
    let py = like_python(st, "rules", "test_attack_destroys_top_shield");
    attack(st, u, PLAYER_TARGET)?;
    pass_all(st, 50)?;
    assert_eq!(zone_of(st, shields[0]), Zone::Trash);
    assert_eq!(zone_of(st, shields[1]), Zone::Shield);
    py.ends_like_python(st);
    Ok(())
}

// gcg-tags: rule=8-3-1 rule=13-1-4-1 rule=8-5-3-2
#[test]
fn lead_blocker_redirects_and_units_trade() -> Result<(), TestkitError> {
    let mut sc = scenario();
    let attacker = unit(&mut sc, 0, VANILLA_2_2);
    let blocker = unit(&mut sc, 1, BLOCKER);
    sc.start(Step::Main)?;
    let st = sc.state_mut();
    let py = like_python(st, "rules", "test_blocker_redirects_and_units_trade");
    attack(st, attacker, PLAYER_TARGET)?;
    assert_eq!(pending_kind(st), Some(DecisionKind::Block));
    block(st, Some(blocker))?;
    pass_all(st, 50)?;
    assert!(st.card(blocker).rested);
    assert_eq!(zone_of(st, attacker), Zone::Trash); // Launcher Strike deals 2 to the 2-HP Zaku
    py.ends_like_python(st);
    Ok(())
}

// gcg-tags: rule=13-2-5-1 rule=5-10-3 card=GD01-097
#[test]
fn lead_burst_add_to_hand() -> Result<(), TestkitError> {
    let mut sc = scenario();
    let u = unit(&mut sc, 0, VANILLA_2_2);
    let shield = sc.shields(1, &[BURST_ADD])?[0];
    sc.start(Step::Main)?;
    let st = sc.state_mut();
    let py = like_python(st, "rules", "test_burst_add_to_hand");
    attack(st, u, PLAYER_TARGET)?;
    pass_all(st, 50)?;
    assert_eq!(pending_kind(st), Some(DecisionKind::Burst));
    yes(st)?;
    assert_eq!(zone_of(st, shield), Zone::Hand);
    py.ends_like_python(st);
    Ok(())
}

// gcg-tags: rule=13-2-5-2
#[test]
fn lead_declining_burst_trashes_shield() -> Result<(), TestkitError> {
    let mut sc = scenario();
    let u = unit(&mut sc, 0, VANILLA_2_2);
    let shield = sc.shields(1, &[BURST_ADD])?[0];
    sc.start(Step::Main)?;
    let st = sc.state_mut();
    let py = like_python(st, "rules", "test_declining_burst_trashes_shield");
    attack(st, u, PLAYER_TARGET)?;
    pass_all(st, 50)?;
    no(st)?;
    assert_eq!(zone_of(st, shield), Zone::Trash);
    py.ends_like_python(st);
    Ok(())
}

// gcg-tags: rule=7-6-5-1 rule=4-8-4
#[test]
fn lead_hand_limit_discards_down_to_ten() -> Result<(), TestkitError> {
    let mut sc = scenario();
    sc.hand(0, &[VANILLA_2_2; 12])?;
    sc.start(Step::Main)?;
    let st = sc.state_mut();
    let py = like_python(st, "rules", "test_hand_limit_discards_down_to_ten");
    end_main(st)?;
    pass_all(st, 50)?;
    assert_eq!(pending_kind(st), Some(DecisionKind::Discard));
    let hand = st.zone(0, Zone::Hand).to_vec();
    select(st, &[hand[0]], None)?;
    select(st, &[hand[1]], None)?;
    assert_eq!(st.zone(0, Zone::Hand).len(), 10);
    py.ends_like_python(st);
    Ok(())
}

// gcg-tags: rule=7-3-1 rule=7-4-1
#[test]
fn lead_draw_and_resource_phases() -> Result<(), TestkitError> {
    let mut sc = Scenario::new(
        &Registry::embedded(),
        ScenarioOptions {
            active: 1,
            ..ScenarioOptions::default()
        },
    );
    sc.resource_deck(0, 3)?;
    sc.start(Step::Main)?;
    let st = sc.state_mut();
    let py = like_python(st, "rules", "test_draw_and_resource_phases");
    let hand_before = st.zone(0, Zone::Hand).len();
    to_next_turn(st)?;
    assert_eq!(st.active(), 0);
    assert_eq!(st.zone(0, Zone::Hand).len(), hand_before + 1);
    assert_eq!(st.zone(0, Zone::ResourceArea).len(), 1);
    py.ends_like_python(st);
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// tests/cards/test_lead_examples.py

// gcg-tags: card=GD01-001 ruling=GD01-001:Q119
#[test]
fn lead_gd01_001_gains_repair_itself() -> Result<(), TestkitError> {
    let mut sc = scenario();
    let gundam = unit(&mut sc, 0, "GD01-001");
    sc.start(Step::Main)?;
    let st = sc.state();
    let py = like_python(st, "cards", "test_gd01_001_gains_repair_itself");
    let kws = keywords(st, gundam)?;
    // Q119: the first effect applies to itself.
    assert_eq!(
        kws.iter().find(|(k, _)| *k == Kw::Repair).map(|e| e.1),
        Some(1)
    );
    py.ends_like_python(st);
    Ok(())
}

// gcg-tags: card=GD01-001
#[test]
fn lead_gd01_001_when_paired_draws_with_two_other_units() -> Result<(), TestkitError> {
    let mut sc = scenario();
    sc.resources(0, 5, 0, 0)?;
    let gundam = unit(&mut sc, 0, "GD01-001");
    unit(&mut sc, 0, VANILLA_2_2);
    unit(&mut sc, 0, VANILLA_2_2);
    let pilot = sc.add(0, PILOT, Zone::Hand, AddOptions::default())?;
    sc.start(Step::Main)?;
    let st = sc.state_mut();
    let py = like_python(
        st,
        "cards",
        "test_gd01_001_when_paired_draws_with_two_other_units",
    );
    let hand_before = st.zone(0, Zone::Hand).len();
    play(st, pilot, Some(gundam), 0)?;
    // The Pilot left the hand and one card was drawn.
    assert_eq!(st.zone(0, Zone::Hand).len(), hand_before);
    py.ends_like_python(st);
    Ok(())
}

// gcg-tags: card=GD01-001
#[test]
fn lead_gd01_001_when_paired_no_draw_with_one_other_unit() -> Result<(), TestkitError> {
    let mut sc = scenario();
    sc.resources(0, 5, 0, 0)?;
    let gundam = unit(&mut sc, 0, "GD01-001");
    unit(&mut sc, 0, VANILLA_2_2);
    let pilot = sc.add(0, PILOT, Zone::Hand, AddOptions::default())?;
    sc.start(Step::Main)?;
    let st = sc.state_mut();
    let py = like_python(
        st,
        "cards",
        "test_gd01_001_when_paired_no_draw_with_one_other_unit",
    );
    let hand_before = st.zone(0, Zone::Hand).len();
    play(st, pilot, Some(gundam), 0)?;
    assert_eq!(st.zone(0, Zone::Hand).len(), hand_before - 1);
    py.ends_like_python(st);
    Ok(())
}

// gcg-tags: card=GD01-001 rule=13-1-1-1
#[test]
fn lead_gd01_001_repair_recovers_at_end_of_turn() -> Result<(), TestkitError> {
    let mut sc = scenario();
    let o = AddOptions {
        damage: 2,
        ..AddOptions::default()
    };
    let gundam = sc.add(0, "GD01-001", Zone::Battle, o)?;
    sc.start(Step::Main)?;
    let st = sc.state_mut();
    let py = like_python(st, "cards", "test_gd01_001_repair_recovers_at_end_of_turn");
    to_next_turn(st)?;
    assert_eq!(st.card(gundam).damage, 1);
    py.ends_like_python(st);
    Ok(())
}
