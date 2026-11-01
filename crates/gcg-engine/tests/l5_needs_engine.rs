//! Layer 5 tests that run the whole engine (rules management, the derived view, the
//! interpreter, battle): they pass once layers L2-L4 are integrated (PLAN §10.5, M5.5). Every
//! test name starts with `needs_engine_`.
//!
//! - the "always option 0" game of Inv 06b §18 (its first 24 decisions and its final state) and
//!   two seeded random games, decision by decision against Python's digests, with
//!   `invariants::check` after every action;
//! - the setup decisions into turn 1, the README scenario, `invariants::check` on mid-game
//!   states, `procedures` against the recorded options, `testkit::install_view`;
//! - the native ports of the 12 lead-example tests (`tests/rules/test_lead_examples.py`,
//!   `tests/cards/test_lead_examples.py`).

mod l5_common;

use std::sync::Arc;

use gcg_core::rng::SplitMix64;
use gcg_effects::Kw;
use gcg_engine::canon::state_digest;
use gcg_engine::invariants::{check, initial_multiset};
use gcg_engine::testkit::{
    attack, block, end_main, has_action, install_view, keywords, no, pass_all, play, select,
    to_next_turn, yes, zone_of,
};
use gcg_engine::{
    Action, ActionKind, AddOptions, CheckError, DecisionKind, EndReason, GameState, NewGameOptions,
    PLAYER_TARGET, Scenario, ScenarioOptions, Step, TestkitError, Zone, apply, legal_actions,
    procedures, view,
};
use l5_common::{
    action, action_json, int, load, pending_json, record, records, reg, start_game, u64_of,
};
use serde_json::Value;

/// `turn:STEP:kind:Pplayer:n_options:chosen` (Inv 06b §18).
fn decision_line(st: &GameState, chosen: Action) -> String {
    let d = st.pending().expect("a decision");
    format!(
        "{}:{}:{}:P{}:{}:{chosen}",
        st.turn(),
        st.step().py_name(),
        d.hdr.kind.as_str(),
        d.hdr.player,
        d.options.len()
    )
}

#[test]
fn needs_engine_first_option_game_matches_inventory_vectors() {
    const FIRST_24: [&str; 24] = [
        "0:SETUP_CHOOSE_FIRST:choose_first:P1:2:go_first(1)",
        "0:SETUP_REDRAW_P1:redraw:P1:2:keep()",
        "0:SETUP_REDRAW_P2:redraw:P0:2:keep()",
        "1:MAIN:main:P1:1:end_main()",
        "2:MAIN:main:P0:2:play_unit(2,1)",
        "2:MAIN:main:P0:1:end_main()",
        "3:MAIN:main:P1:3:play_unit(53,0)",
        "3:MAIN:main:P1:2:play_unit(62,0)",
        "3:MAIN:main:P1:1:end_main()",
        "4:MAIN:main:P0:3:play_command(12,0)",
        "4:MAIN:select:P0:2:select(53)",
        "4:MAIN:main:P0:2:attack(2,-2)",
        "4:MAIN:main:P0:1:end_main()",
        "5:MAIN:main:P1:6:play_unit(54,0)",
        "5:MAIN:main:P1:4:play_unit(63,0)",
        "5:MAIN:main:P1:3:attack(62,-2)",
        "5:MAIN:main:P1:1:end_main()",
        "6:MAIN:main:P0:5:play_unit(0,0)",
        "6:MAIN:main:P0:5:pair(1,2,0)",
        "6:MAIN:main:P0:3:attack(2,-2)",
        "6:MAIN:main:P0:1:end_main()",
        "7:MAIN:main:P1:14:play_base(50,0)",
        "7:MAIN:main:P1:18:play_unit(51,0)",
        "7:MAIN:main:P1:8:activate(50,765,0)",
    ];
    let rec = record("game", "first-option/rgz-bwf/12345");
    let mut st = start_game(rec, NewGameOptions::default()).expect("new_game");
    let mut lines = Vec::new();
    while let Some(d) = st.pending() {
        let a = d.options[0];
        lines.push(decision_line(&st, a));
        apply(&mut st, a, true).expect("apply");
    }
    assert_eq!(lines[..24], FIRST_24);
    assert_eq!(lines.len(), 69);
    assert_eq!(st.winner(), Some(1));
    assert_eq!(st.end_reason(), Some(EndReason::BattleDamage));
    assert_eq!((st.turn(), st.active(), st.action_count()), (11, 1, 69));
    assert_eq!((st.version(), st.event_group()), (357, 123));
    assert_eq!(st.rng_state(), 7_774_064_181_650_859_138);
    assert_eq!(
        state_digest(&st),
        "5ce28ed973cf9e06670a38f6a320c26a9a2c9f15fa5066e8fd8a6744c2f31e9a"
    );
}

#[test]
fn needs_engine_games_replay_decision_by_decision() {
    for rec in records("game") {
        let id = rec["id"].as_str().unwrap();
        let mut st = start_game(rec, NewGameOptions::default()).expect("new_game");
        let initial = initial_multiset(&st);
        let mut rng = rec["policy_seed"]
            .as_str()
            .map(|s| SplitMix64::new(s.parse().unwrap()));
        for (i, d) in rec["decisions"].as_array().unwrap().iter().enumerate() {
            let at = format!("{id} decision {i}");
            assert_eq!(
                &state_digest(&st)[..16],
                d["digest16"].as_str().unwrap(),
                "{at}"
            );
            let dec = st.pending().expect("a decision");
            assert_eq!(i64::from(st.turn()), int(&d["turn"]), "{at}");
            assert_eq!(i64::from(st.step().value()), int(&d["step"]), "{at}");
            assert_eq!(dec.hdr.kind.as_str(), d["kind"].as_str().unwrap(), "{at}");
            assert_eq!(i64::from(dec.hdr.player), int(&d["player"]), "{at}");
            let n = dec.options.len();
            assert_eq!(n as i64, int(&d["n"]), "{at}");
            let a = match &mut rng {
                None => dec.options[0],
                Some(r) => dec.options[r.randrange_usize(n)],
            };
            assert_eq!(action_json(&a), d["chosen"], "{at}");
            apply(&mut st, a, true).unwrap_or_else(|e| panic!("{at}: {e}"));
            check(&st, Some(&initial)).unwrap_or_else(|e| panic!("{at}: {e}"));
        }
        let f = &rec["final"];
        assert_eq!(state_digest(&st), f["digest"].as_str().unwrap(), "{id}");
        assert_eq!(st.winner().map(i64::from), f["winner"].as_i64(), "{id}");
        assert_eq!(
            st.end_reason().map(|e| e.as_str()),
            f["end_reason"].as_str(),
            "{id}"
        );
        assert_eq!(st.rng_state(), u64_of(&f["rng"]), "{id}");
        assert!(legal_actions(&st).is_empty());
    }
}

#[test]
fn needs_engine_setup_into_turn_one() {
    for rec in records("new_game") {
        let id = rec["id"].as_str().unwrap();
        let chooser = rec["chooser"].as_u64().map(|c| u8::try_from(c).unwrap());
        let o = NewGameOptions {
            chooser,
            ..NewGameOptions::default()
        };
        let mut st = start_game(rec, o).expect("new_game");
        for (k, s) in rec["steps"].as_array().unwrap().iter().enumerate() {
            apply(&mut st, action(&s["apply"]), true).expect("a setup decision");
            assert_eq!(
                state_digest(&st),
                s["digest"].as_str().unwrap(),
                "{id} step {k}"
            );
            assert_eq!(
                st.version(),
                s["version"].as_u64().unwrap(),
                "{id} step {k}"
            );
            assert_eq!(pending_json(&st), s["pending"], "{id} step {k}");
        }
        assert_eq!(st.step(), Step::Main, "{id}");
    }
}

#[test]
fn needs_engine_readme_scenario_starts_in_main() {
    let rec = record("scenario_start", "readme-example");
    let mut sc = Scenario::new(&reg(), ScenarioOptions::default());
    let gundam = sc
        .add(0, "GD01-001", Zone::Hand, AddOptions::default())
        .unwrap();
    assert_eq!(i64::from(gundam), int(&rec["hand"]));
    sc.resources(0, 5, 0, 0).unwrap();
    sc.start(Step::Main).unwrap();
    let st = sc.state();
    assert_eq!(state_digest(st), rec["after"].as_str().unwrap());
    let opts: Vec<Value> = legal_actions(st).iter().map(action_json).collect();
    assert_eq!(Value::from(opts), rec["options"]);
}

#[test]
fn needs_engine_invariants_on_mid_game_states() {
    let all = records("state");
    let by_id = |id: &str| all.iter().find(|r| r["id"] == id).expect("a state");
    let mut checked = 0;
    for rec in &all {
        let inv = &rec["invariants"];
        if !inv["needs_view"].as_bool().unwrap() {
            continue;
        }
        let id = rec["id"].as_str().unwrap();
        let game = id.rsplit_once('/').unwrap().0;
        let base = load(&by_id(&format!("{game}/d0"))["snapshot"]);
        let st = load(&rec["snapshot"]);
        let got = check(&st, Some(&initial_multiset(&base)));
        match inv["error"].as_str() {
            None => assert_eq!(got, Ok(()), "{id}"),
            Some(msg) => {
                let Err(CheckError::Violation(v)) = got else {
                    panic!("{id}: {got:?}");
                };
                assert_eq!(v.to_string(), msg, "{id}");
            }
        }
        checked += 1;
    }
    assert!(checked >= 10, "{checked}");
}

#[test]
fn needs_engine_procedures_reproduce_the_recorded_options() {
    let mut checked = 0;
    for rec in records("state") {
        let st = load(&rec["snapshot"]);
        let Some(d) = st.pending() else { continue };
        if !st.frames().is_empty() {
            continue;
        }
        match d.hdr.kind {
            DecisionKind::Main => {
                assert_eq!(
                    procedures::main_options(&st).unwrap(),
                    d.options,
                    "{}",
                    rec["id"]
                );
                checked += 1;
            }
            DecisionKind::ActionStep => {
                let mut opts = procedures::action_step_options(&st, d.hdr.player).unwrap();
                opts.push(Action::of(ActionKind::Pass));
                assert_eq!(opts, d.options, "{}", rec["id"]);
                checked += 1;
            }
            _ => {}
        }
    }
    assert!(checked >= 5, "{checked}");
}

#[test]
fn needs_engine_install_view_reuses_the_view_while_the_key_holds() {
    let rec = records("state")
        .into_iter()
        .find(|r| r["invariants"]["needs_view"].as_bool().unwrap())
        .unwrap();
    let st = load(&rec["snapshot"]);
    let from = st.clone();
    let key = view::derived(&from).unwrap().key();
    install_view(&st, &from, key).unwrap();
    assert!(Arc::ptr_eq(
        &view::derived(&st).unwrap(),
        &view::derived(&from).unwrap()
    ));
    let mut other = key;
    other.turn += 1;
    assert!(install_view(&st, &from, other).is_err());
}

// ---------------------------------------------------------------------------------------------
// Native ports of the lead-example tests (tests/rules/test_lead_examples.py)

const VANILLA_2_2: &str = "GD01-060"; // Zaku Mariner, Lv2 cost1 2/2, no effects
const BLOCKER: &str = "GD01-072"; // Launcher Strike Gundam, <Blocker>
const BURST_ADD: &str = "GD01-097"; // Guel Jeturk, 【Burst】Add this card to your hand.
const PILOT: &str = "GD01-089"; // Riddhe Marcenas, a Pilot card

fn scenario() -> Scenario {
    Scenario::new(&reg(), ScenarioOptions::default())
}

fn unit(sc: &mut Scenario, player: u8, number: &str) -> u16 {
    sc.add(player, number, Zone::Battle, AddOptions::default())
        .unwrap()
}

fn pending_kind(st: &GameState) -> Option<DecisionKind> {
    st.pending().map(|d| d.hdr.kind)
}

// gcg-tags: rule=3-2-4 rule=3-2-6-3
#[test]
fn needs_engine_lead_newly_deployed_unit_cannot_attack() -> Result<(), TestkitError> {
    let mut sc = scenario();
    sc.resources(0, 3, 0, 0)?;
    let card = sc.add(0, VANILLA_2_2, Zone::Hand, AddOptions::default())?;
    sc.start(Step::Main)?;
    let st = sc.state_mut();
    play(st, card, None, 0)?;
    assert_eq!(zone_of(st, card), Zone::Battle);
    assert!(!has_action(
        st,
        ActionKind::Attack,
        Some(i32::from(card)),
        None
    ));
    Ok(())
}

// gcg-tags: rule=8-5-2-2 rule=1-2-2-1 rule=11-2-1-1
#[test]
fn needs_engine_lead_battle_damage_with_empty_shield_area_wins() -> Result<(), TestkitError> {
    let mut sc = scenario();
    let u = unit(&mut sc, 0, VANILLA_2_2);
    sc.start(Step::Main)?;
    let st = sc.state_mut();
    attack(st, u, PLAYER_TARGET)?;
    pass_all(st, 50)?;
    assert_eq!(st.winner(), Some(0));
    assert_eq!(st.end_reason(), Some(EndReason::BattleDamage));
    Ok(())
}

// gcg-tags: rule=8-5-2-3 rule=8-5-2-3-1 rule=4-6-4-2
#[test]
fn needs_engine_lead_attack_destroys_top_shield() -> Result<(), TestkitError> {
    let mut sc = scenario();
    let u = unit(&mut sc, 0, VANILLA_2_2);
    let shields = sc.shields(1, &[VANILLA_2_2, VANILLA_2_2])?;
    sc.start(Step::Main)?;
    let st = sc.state_mut();
    attack(st, u, PLAYER_TARGET)?;
    pass_all(st, 50)?;
    assert_eq!(zone_of(st, shields[0]), Zone::Trash);
    assert_eq!(zone_of(st, shields[1]), Zone::Shield);
    Ok(())
}

// gcg-tags: rule=8-3-1 rule=13-1-4-1 rule=8-5-3-2
#[test]
fn needs_engine_lead_blocker_redirects_and_units_trade() -> Result<(), TestkitError> {
    let mut sc = scenario();
    let attacker = unit(&mut sc, 0, VANILLA_2_2);
    let blocker = unit(&mut sc, 1, BLOCKER);
    sc.start(Step::Main)?;
    let st = sc.state_mut();
    attack(st, attacker, PLAYER_TARGET)?;
    assert_eq!(pending_kind(st), Some(DecisionKind::Block));
    block(st, Some(blocker))?;
    pass_all(st, 50)?;
    assert!(st.card(blocker).rested);
    assert_eq!(zone_of(st, attacker), Zone::Trash);
    Ok(())
}

// gcg-tags: rule=13-2-5-1 rule=5-10-3 card=GD01-097
#[test]
fn needs_engine_lead_burst_add_to_hand() -> Result<(), TestkitError> {
    let mut sc = scenario();
    let u = unit(&mut sc, 0, VANILLA_2_2);
    let shield = sc.shields(1, &[BURST_ADD])?[0];
    sc.start(Step::Main)?;
    let st = sc.state_mut();
    attack(st, u, PLAYER_TARGET)?;
    pass_all(st, 50)?;
    assert_eq!(pending_kind(st), Some(DecisionKind::Burst));
    yes(st)?;
    assert_eq!(zone_of(st, shield), Zone::Hand);
    Ok(())
}

// gcg-tags: rule=13-2-5-2
#[test]
fn needs_engine_lead_declining_burst_trashes_shield() -> Result<(), TestkitError> {
    let mut sc = scenario();
    let u = unit(&mut sc, 0, VANILLA_2_2);
    let shield = sc.shields(1, &[BURST_ADD])?[0];
    sc.start(Step::Main)?;
    let st = sc.state_mut();
    attack(st, u, PLAYER_TARGET)?;
    pass_all(st, 50)?;
    no(st)?;
    assert_eq!(zone_of(st, shield), Zone::Trash);
    Ok(())
}

// gcg-tags: rule=7-6-5-1 rule=4-8-4
#[test]
fn needs_engine_lead_hand_limit_discards_down_to_ten() -> Result<(), TestkitError> {
    let mut sc = scenario();
    sc.hand(0, &[VANILLA_2_2; 12])?;
    sc.start(Step::Main)?;
    let st = sc.state_mut();
    end_main(st)?;
    pass_all(st, 50)?;
    assert_eq!(pending_kind(st), Some(DecisionKind::Discard));
    let hand = st.zone(0, Zone::Hand).to_vec();
    select(st, &[hand[0]], None)?;
    select(st, &[hand[1]], None)?;
    assert_eq!(st.zone(0, Zone::Hand).len(), 10);
    Ok(())
}

// gcg-tags: rule=7-3-1 rule=7-4-1
#[test]
fn needs_engine_lead_draw_and_resource_phases() -> Result<(), TestkitError> {
    let mut sc = Scenario::new(
        &reg(),
        ScenarioOptions {
            active: 1,
            ..ScenarioOptions::default()
        },
    );
    sc.resource_deck(0, 3)?;
    sc.start(Step::Main)?;
    let st = sc.state_mut();
    let hand_before = st.zone(0, Zone::Hand).len();
    to_next_turn(st)?;
    assert_eq!(st.active(), 0);
    assert_eq!(st.zone(0, Zone::Hand).len(), hand_before + 1);
    assert_eq!(st.zone(0, Zone::ResourceArea).len(), 1);
    Ok(())
}

// Native ports of tests/cards/test_lead_examples.py

// gcg-tags: card=GD01-001 ruling=GD01-001:Q119
#[test]
fn needs_engine_lead_gd01_001_gains_repair_itself() -> Result<(), TestkitError> {
    let mut sc = scenario();
    let gundam = unit(&mut sc, 0, "GD01-001");
    sc.start(Step::Main)?;
    let kws = keywords(sc.state(), gundam)?;
    assert_eq!(
        kws.iter().find(|(k, _)| *k == Kw::Repair).map(|e| e.1),
        Some(1)
    );
    Ok(())
}

// gcg-tags: card=GD01-001
#[test]
fn needs_engine_lead_gd01_001_when_paired_draws_with_two_other_units() -> Result<(), TestkitError> {
    let mut sc = scenario();
    sc.resources(0, 5, 0, 0)?;
    let gundam = unit(&mut sc, 0, "GD01-001");
    unit(&mut sc, 0, VANILLA_2_2);
    unit(&mut sc, 0, VANILLA_2_2);
    let pilot = sc.add(0, PILOT, Zone::Hand, AddOptions::default())?;
    sc.start(Step::Main)?;
    let st = sc.state_mut();
    let hand_before = st.zone(0, Zone::Hand).len();
    play(st, pilot, Some(gundam), 0)?;
    assert_eq!(st.zone(0, Zone::Hand).len(), hand_before);
    Ok(())
}

// gcg-tags: card=GD01-001
#[test]
fn needs_engine_lead_gd01_001_when_paired_no_draw_with_one_other_unit() -> Result<(), TestkitError>
{
    let mut sc = scenario();
    sc.resources(0, 5, 0, 0)?;
    let gundam = unit(&mut sc, 0, "GD01-001");
    unit(&mut sc, 0, VANILLA_2_2);
    let pilot = sc.add(0, PILOT, Zone::Hand, AddOptions::default())?;
    sc.start(Step::Main)?;
    let st = sc.state_mut();
    let hand_before = st.zone(0, Zone::Hand).len();
    play(st, pilot, Some(gundam), 0)?;
    assert_eq!(st.zone(0, Zone::Hand).len(), hand_before - 1);
    Ok(())
}

// gcg-tags: card=GD01-001 rule=13-1-1-1
#[test]
fn needs_engine_lead_gd01_001_repair_recovers_at_end_of_turn() -> Result<(), TestkitError> {
    let mut sc = scenario();
    let o = AddOptions {
        damage: 2,
        ..AddOptions::default()
    };
    let gundam = sc.add(0, "GD01-001", Zone::Battle, o)?;
    sc.start(Step::Main)?;
    let st = sc.state_mut();
    to_next_turn(st)?;
    assert_eq!(st.card(gundam).damage, 1);
    Ok(())
}
