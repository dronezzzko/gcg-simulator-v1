//! The positions the Python AI tests build with the testkit, ported line by line:
//! `tests/ai/situations.py`, and the builders of `test_root_moves.py`,
//! `test_puzzles_targets.py`, `test_policy_knobs.py`, `test_search_options.py` and
//! `test_evaluation_roles.py` that other modules import.

use std::sync::Arc;

use gcg_effects::Registry;
use gcg_engine::testkit::{attack, pass_all, play};
use gcg_engine::{
    Action, ActionKind, AddOptions, DecisionKind, GameState, ScenarioOptions, Step, Uid, Zone,
};

use super::{FILLER, PLAYER, Sc, apply_ok, new_game, pending_kind};

/// `ZEON` (`tests/ai/test_root_moves.py:39`, `test_puzzles_targets.py:24`).
pub const ZEON: [&str; 7] = [
    "ST03-008", "GD01-059", "GD01-035", "GD01-030", "GD01-031", "ST03-013", "GD01-115",
];

/// `FEDERATION` (`tests/ai/test_puzzles_targets.py:25-40`).
pub const FEDERATION: [&str; 14] = [
    "GD01-008", "ST01-005", "GD01-004", "ST01-004", "ST01-001", "GD01-001", "GD04-008", "ST01-010",
    "GD01-087", "ST01-012", "ST01-014", "ST04-013", "GD01-099", "GD04-122",
];

pub fn ok<T, E: std::fmt::Debug>(r: Result<T, E>) -> T {
    r.unwrap_or_else(|e| panic!("{e:?}"))
}

/// `play(st, uid)` / `play(st, uid, onto=unit)`.
pub fn play_card(st: &mut GameState, uid: Uid, onto: Option<Uid>) -> Action {
    ok(play(st, uid, onto, 0))
}

/// `attack(st, attacker, target)`.
pub fn attack_with(st: &mut GameState, attacker: Uid, target: i32) -> Action {
    ok(attack(st, attacker, target))
}

pub fn assert_pending(st: &GameState, kind: DecisionKind) {
    assert_eq!(pending_kind(st), Some(kind));
}

// ---------------------------------------------------------------------------------------------
// tests/ai/situations.py

/// `choose_first()` (`situations.py:17-18`).
pub fn choose_first() -> GameState {
    new_game(5)
}

/// `redraw()` (`situations.py:21-25`).
pub fn redraw() -> GameState {
    let mut st = new_game(6);
    let first = st.pending().expect("a decision").options[0];
    apply_ok(&mut st, first);
    st
}

/// `main_phase()` (`situations.py:28-34`).
pub fn main_phase() -> GameState {
    let mut sc = Sc::new();
    sc.resources(0, 4, 0, 0);
    sc.add(0, "ST01-001");
    sc.rested(1, "ST04-005");
    sc.hand(0, &["GD01-018", "ST01-010", "ST01-014"]);
    sc.start()
}

/// `block_step()` (`situations.py:37-44`).
pub fn block_step() -> GameState {
    let mut sc = Sc::new();
    let gm = sc.add(0, "ST01-005");
    sc.add(1, "ST01-008");
    sc.add(1, "ST01-009");
    let mut st = sc.start();
    attack_with(&mut st, gm, PLAYER);
    st
}

/// `action_step()` (`situations.py:47-54`).
pub fn action_step() -> GameState {
    let mut sc = Sc::new();
    let gundam = sc.add(0, "GD01-013");
    sc.resources(1, 3, 0, 0);
    sc.hand(1, &["ST01-014"]);
    let mut st = sc.start();
    attack_with(&mut st, gundam, PLAYER);
    st
}

/// `order_trigger()` (`situations.py:57-61`).
pub fn order_trigger() -> GameState {
    let mut sc = Sc::new();
    for _ in 0..2 {
        sc.add_with(
            0,
            "GD01-004",
            Zone::Battle,
            AddOptions {
                damage: 1,
                ..AddOptions::default()
            },
        );
    }
    sc.start_at(Step::EndStep)
}

/// `burst()` (`situations.py:64-71`).
pub fn burst() -> GameState {
    let mut sc = Sc::new();
    let gm = sc.add(0, "ST01-005");
    sc.shields(1, &["ST01-010"]);
    let mut st = sc.start();
    attack_with(&mut st, gm, PLAYER);
    ok(pass_all(&mut st, 50));
    st
}

/// `yes_no()` (`situations.py:74-81`).
pub fn yes_no() -> GameState {
    let mut sc = Sc::new();
    sc.resources(0, 3, 0, 0);
    let age = sc.in_hand(0, "GD02-021");
    sc.hand(0, &["GD02-030"]);
    let mut st = sc.start();
    play_card(&mut st, age, None);
    st
}

/// `select_target()` (`situations.py:84-92`).
pub fn select_target() -> GameState {
    let mut sc = Sc::new();
    sc.resources(0, 3, 0, 0);
    let guntank = sc.in_hand(0, "ST01-004");
    sc.add(1, "ST01-008");
    sc.add(1, "ST02-009");
    let mut st = sc.start();
    play_card(&mut st, guntank, None);
    st
}

/// `select_up_to_two()` (`situations.py:95-104`).
pub fn select_up_to_two() -> GameState {
    let mut sc = Sc::new();
    sc.resources(0, 4, 0, 0);
    let orders = sc.in_hand(0, "GD01-099");
    sc.add(1, "ST01-008");
    sc.add(1, "ST02-009");
    sc.add(1, "ST04-005");
    let mut st = sc.start();
    play_card(&mut st, orders, None);
    st
}

/// `select_mode()` (`situations.py:107-114`).
pub fn select_mode() -> GameState {
    let mut sc = Sc::new();
    sc.resources(0, 4, 0, 0);
    let card = sc.in_hand(0, "GD05-106");
    sc.trash(0, &["GD01-088"]);
    let mut st = sc.start();
    play_card(&mut st, card, None);
    st
}

/// `excess()` (`situations.py:117-125`).
pub fn excess() -> GameState {
    let mut sc = Sc::new();
    sc.resources(0, 2, 0, 0);
    for _ in 0..6 {
        sc.add(0, "ST01-005");
    }
    let gm = sc.in_hand(0, "ST04-005");
    let mut st = sc.start();
    play_card(&mut st, gm, None);
    st
}

/// `discard()` (`situations.py:128-135`).
pub fn discard() -> GameState {
    let mut sc = Sc::new();
    sc.resources(0, 4, 0, 0);
    let strike = sc.in_hand(0, "ST04-002");
    sc.hand(0, &["ST04-005", "ST04-013"]);
    let mut st = sc.start();
    play_card(&mut st, strike, None);
    st
}

/// `arrange()` (`situations.py:138-144`).
pub fn arrange() -> GameState {
    let mut sc = Sc::new();
    sc.resources(0, 1, 0, 0);
    let dopp = sc.in_hand(0, "GD01-039");
    let mut st = sc.start();
    play_card(&mut st, dopp, None);
    st
}

/// A position builder.
pub type Build = fn() -> GameState;

/// `SITUATIONS` (`situations.py:147-160`) in dict order, keyed by decision kind.
pub const SITUATIONS: [(DecisionKind, Build); 12] = [
    (DecisionKind::ChooseFirst, choose_first),
    (DecisionKind::Redraw, redraw),
    (DecisionKind::Main, main_phase),
    (DecisionKind::Block, block_step),
    (DecisionKind::ActionStep, action_step),
    (DecisionKind::OrderTrigger, order_trigger),
    (DecisionKind::Burst, burst),
    (DecisionKind::YesNo, yes_no),
    (DecisionKind::Select, select_target),
    (DecisionKind::Excess, excess),
    (DecisionKind::Discard, discard),
    (DecisionKind::Arrange, arrange),
];

/// `EXTRA_SELECTS` (`situations.py:162-165`).
pub const EXTRA_SELECTS: [(&str, Build); 2] = [
    ("select_up_to_two", select_up_to_two),
    ("select_mode", select_mode),
];

/// The situation building a decision of `kind`.
pub fn situation(kind: DecisionKind) -> GameState {
    SITUATIONS
        .iter()
        .find(|(k, _)| *k == kind)
        .map(|(_, f)| f())
        .expect("a situation of that kind")
}

// ---------------------------------------------------------------------------------------------
// tests/ai/test_root_moves.py

/// `hide_enemy_cards(sc, hand)` (`test_root_moves.py:74-79`): `hand` cards of player 1 hidden
/// from player 0 and a varied 28-card deck.
pub fn hide_enemy_cards(sc: &mut Sc, hand: usize) {
    for i in 0..hand {
        sc.hidden_in_hand(1, ZEON[(2 * i) % ZEON.len()]);
    }
    let deck: Vec<&str> = (0..28).map(|i| ZEON[i % ZEON.len()]).collect();
    sc.deck(1, &deck);
}

/// `rests_the_blocker(rested_decoy=..., hidden=...)` (`test_root_moves.py:86-99`).
pub fn rests_the_blocker(rested_decoy: bool, hidden: usize) -> GameState {
    let mut sc = Sc::new();
    sc.resources(0, 3, 0, 0);
    sc.add(0, "ST01-005");
    sc.hand(0, &["ST02-014"]);
    sc.add(1, "ST01-008");
    if rested_decoy {
        sc.rested(1, "ST04-005");
    }
    hide_enemy_cards(&mut sc, hidden);
    sc.shields(0, &[FILLER, FILLER, FILLER]);
    sc.start()
}

/// `holds_removal()` (`test_root_moves.py:102-111`).
pub fn holds_removal() -> GameState {
    let mut sc = Sc::new();
    sc.resources(0, 4, 3, 0);
    sc.add_with(
        0,
        "GD01-013",
        Zone::Battle,
        AddOptions {
            deployed_this_turn: true,
            ..AddOptions::default()
        },
    );
    sc.in_hand(0, "ST02-014");
    sc.add(1, "ST01-008");
    sc.resources(1, 2, 0, 0);
    sc.shields(0, &[FILLER, FILLER, FILLER, FILLER]);
    sc.start()
}

/// `zero_ap_scenario(active_resources=..., hidden=...)` (`test_root_moves.py:147-160`).
pub fn zero_ap_scenario(active_resources: usize, hidden: usize) -> (GameState, Uid) {
    let mut sc = Sc::new();
    sc.resources(0, active_resources, 0, 0);
    let worker = sc.add(0, "ST05-003");
    sc.add(0, "GD05-060");
    sc.hand(0, &["ST01-014"]);
    sc.add(1, "ST04-005");
    hide_enemy_cards(&mut sc, hidden);
    sc.shields(0, &[FILLER, FILLER, FILLER]);
    sc.shields(1, &[FILLER, FILLER, FILLER]);
    (sc.start(), worker)
}

/// `amuro_rests(extra_units=..., hidden=..., reactor=...)` (`test_root_moves.py:216-238`):
/// the state, the rested Schuzrum-Galluss and the active Zaku II.
pub fn amuro_rests(
    extra_units: &[&str],
    hidden: usize,
    reactor: Option<&str>,
) -> (GameState, Uid, Uid) {
    let mut sc = Sc::new();
    sc.resources(0, 4, 0, 0);
    let host = sc.add(0, "GD01-013");
    for number in extra_units {
        sc.add(0, number);
    }
    let amuro = sc.in_hand(0, "ST01-010");
    let rested = sc.rested(1, "GD04-040");
    let zaku = sc.add(1, "GD01-035");
    if let Some(r) = reactor {
        sc.base(1, Some(r));
    }
    sc.resources(1, 4, 0, 0);
    hide_enemy_cards(&mut sc, hidden);
    sc.filler_shields(0, 5);
    let enemy: Vec<&str> = (0..5).map(|i| ZEON[(3 * i) % ZEON.len()]).collect();
    sc.shields(1, &enemy);
    let mut st = sc.start();
    play_card(&mut st, amuro, Some(host));
    assert_pending(&st, DecisionKind::Select);
    (st, rested, zaku)
}

/// `age_device(hidden=...)` (`test_root_moves.py:340-348`).
pub fn age_device(hidden: usize) -> GameState {
    let mut sc = Sc::new();
    sc.resources(0, 4, 0, 0);
    sc.add(0, "GD02-029");
    sc.hand(0, &["GD02-103"]);
    hide_enemy_cards(&mut sc, hidden);
    sc.shields(0, &[FILLER, FILLER, FILLER]);
    sc.shields(1, &[FILLER, FILLER, FILLER]);
    sc.start()
}

/// `identical_attackers(hidden=...)` (`test_root_moves.py:418-427`): the state, both GMs and
/// both Strike Daggers.
pub fn identical_attackers(hidden: usize) -> (GameState, [Uid; 4]) {
    let mut sc = Sc::new();
    let gm1 = sc.add(0, "ST01-005");
    let gm2 = sc.add(0, "ST01-005");
    let dagger1 = sc.rested(1, "ST04-005");
    let dagger2 = sc.rested(1, "ST04-005");
    hide_enemy_cards(&mut sc, hidden);
    sc.shields(1, &[FILLER, FILLER, FILLER]);
    (sc.start(), [gm1, gm2, dagger1, dagger2])
}

// ---------------------------------------------------------------------------------------------
// tests/ai/test_puzzles_targets.py

/// `rest_the_active_unit()` (`test_puzzles_targets.py:46-65`).
pub fn rest_the_active_unit() -> (GameState, Action) {
    let mut sc = Sc::new();
    sc.resources(0, 4, 0, 0);
    let host = sc.add(0, "GD01-013");
    sc.add(0, "ST01-005");
    let amuro = sc.in_hand(0, "ST01-010");
    sc.rested(1, "GD04-040");
    let zaku = sc.add(1, "GD01-035");
    sc.resources(1, 4, 0, 0);
    for i in 0..4 {
        sc.hidden_in_hand(1, ZEON[(2 * i) % ZEON.len()]);
    }
    let deck: Vec<&str> = (0..28).map(|i| ZEON[i % ZEON.len()]).collect();
    sc.deck(1, &deck);
    sc.filler_shields(0, 5);
    let enemy: Vec<&str> = (0..5).map(|i| ZEON[(3 * i) % ZEON.len()]).collect();
    sc.shields(1, &enemy);
    let mut st = sc.start();
    play_card(&mut st, amuro, Some(host));
    (st, select(zaku))
}

/// `damage_the_unit_that_cannot_repair()` (`test_puzzles_targets.py:68-86`).
pub fn damage_the_unit_that_cannot_repair() -> (GameState, Action) {
    let mut sc = Sc::new();
    sc.resources(0, 4, 0, 0);
    let barbatos = sc.add(0, "ST05-002");
    let mikazuki = sc.in_hand(0, "ST05-010");
    sc.rested(1, "GD01-004");
    let gm = sc.rested(1, "ST01-005");
    sc.resources(1, 4, 0, 0);
    for i in 0..5 {
        sc.hidden_in_hand(1, FEDERATION[(3 * i + 1) % FEDERATION.len()]);
    }
    let deck: Vec<&str> = (0..28).map(|i| FEDERATION[i % FEDERATION.len()]).collect();
    sc.deck(1, &deck);
    sc.filler_shields(0, 5);
    let enemy: Vec<&str> = (0..5)
        .map(|i| FEDERATION[(5 * i) % FEDERATION.len()])
        .collect();
    sc.shields(1, &enemy);
    let mut st = sc.start();
    play_card(&mut st, mikazuki, Some(barbatos));
    (st, select(gm))
}

/// `PUZZLES` (`test_puzzles_targets.py:89-93`) in dict order: name, position, agent seeds
/// `1..=n`, passes required.
pub type Puzzle = (&'static str, fn() -> (GameState, Action), u64, u32);
pub const PUZZLES: [Puzzle; 2] = [
    ("rest_the_active_unit", rest_the_active_unit, 16, 14),
    (
        "damage_the_unit_that_cannot_repair",
        damage_the_unit_that_cannot_repair,
        24,
        16,
    ),
];

// ---------------------------------------------------------------------------------------------
// tests/ai/test_policy_knobs.py, test_search_options.py, test_evaluation_roles.py

/// `_protection(attacked_is_big)` (`test_policy_knobs.py:131-146`): the state and the attacked
/// Unit.
pub fn protection(attacked_is_big: bool) -> (GameState, Uid) {
    let mut sc = Sc::active(1);
    sc.resources(0, 4, 0, 0);
    sc.hand(0, &["ST03-014"]);
    let gm = sc.rested(0, "ST01-005");
    let gundam = sc.add_with(
        0,
        "GD01-013",
        Zone::Battle,
        AddOptions {
            rested: true,
            damage: if attacked_is_big { 2 } else { 0 },
            ..AddOptions::default()
        },
    );
    let attacker = sc.add(1, "ST01-005");
    sc.shields(0, &[FILLER, FILLER]);
    sc.shields(1, &[FILLER, FILLER]);
    let mut st = sc.start();
    let target = if attacked_is_big { gundam } else { gm };
    attack_with(&mut st, attacker, i32::from(target));
    (st, target)
}

/// `SEARCHER, OPPONENT` (`test_search_options.py:33`).
pub const SEARCHER: u8 = 0;
pub const OPPONENT: u8 = 1;

/// `kill_the_damaged_unit()` (`test_search_options.py:45-58`).
pub fn kill_the_damaged_unit() -> GameState {
    let mut sc = Sc::new();
    sc.resources(SEARCHER, 2, 0, 0);
    let card = sc.in_hand(SEARCHER, "GD01-115");
    sc.add(OPPONENT, "GD01-013");
    sc.rested(OPPONENT, "ST01-008");
    sc.filler_shields(SEARCHER, 5);
    sc.filler_shields(OPPONENT, 5);
    let mut st = sc.start();
    play_card(&mut st, card, None);
    st
}

/// `opponent_may_hold_a_trick()` (`test_search_options.py:95-112`).
pub fn opponent_may_hold_a_trick() -> GameState {
    let mut sc = Sc::new();
    sc.add(SEARCHER, "GD01-013");
    sc.add(SEARCHER, "ST01-005");
    sc.add(SEARCHER, "ST01-005");
    sc.resources(OPPONENT, 3, 0, 0);
    let pool = ["ST01-014", "ST01-005", "GD01-008", "GD01-013"];
    for i in 0..4 {
        sc.hidden_in_hand(OPPONENT, pool[(i + 1) % pool.len()]);
    }
    let deck: Vec<&str> = (0..24).map(|i| pool[i % pool.len()]).collect();
    sc.deck(OPPONENT, &deck);
    sc.filler_shields(SEARCHER, 5);
    sc.filler_shields(OPPONENT, 5);
    sc.start()
}

/// `_target_positions()` (`test_search_options.py:41-42`).
pub fn target_positions() -> Vec<GameState> {
    PUZZLES.iter().map(|(_, build, _, _)| build().0).collect()
}

/// `both_can_kill(defender_base=...)` (`test_evaluation_roles.py:41-57`).
pub fn both_can_kill(reg: &Arc<Registry>, defender_base: bool) -> GameState {
    let mut sc = Sc::on(
        reg,
        ScenarioOptions {
            turn: 9,
            ..ScenarioOptions::default()
        },
    );
    for p in 0..2 {
        sc.resources(p, 5, 0, 0);
    }
    sc.add(0, "ST01-005");
    sc.add(0, "ST01-005");
    sc.shields(0, &["ST01-005"]);
    sc.rested(1, "ST04-005");
    sc.rested(1, "ST04-005");
    sc.rested(1, "ST01-008");
    if defender_base {
        sc.base(1, None);
    }
    sc.start()
}

/// `Action(A.SELECT, uid)`.
pub fn select(uid: Uid) -> Action {
    Action::new(ActionKind::Select, i32::from(uid), -1, -1)
}

/// `Action(kind, a)`.
pub fn act1(kind: ActionKind, a: impl Into<i32>) -> Action {
    Action::new(kind, a.into(), -1, -1)
}
