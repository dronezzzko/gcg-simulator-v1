//! W-17 step (2) (GAP-A1): the frozen scenarios of `gcg-eval weakness scenarios`, one or more per
//! WK category of the lost weakness benchmark (docs/port/WEAKNESSES.md W-17: WK1 lethal
//! detection, WK2 lethal avoidance, WK3 attack and block, WK4 trick waste (`hold_up`), WK5
//! resource choices (`ex_variant`), WK6 mulligan, WK7 target choices, WK8 long precise lines),
//! built with the Rust testkit from the puzzles and scenario tests that already exist and frozen
//! as canonical snapshots in `docs/port/eval/GAP-A1/weakness/scenarios.json`.
//!
//! The test rebuilds every position, checks that its snapshot reloads to the same state
//! (`canon::from_canonical`, the same digest and the same reference decision), and compares the
//! document with the committed file; `GAPA1_WRITE_SCENARIOS=1` rewrites it.
//!
//! Each scenario's check (`gcg-eval weakness scenarios`, `crates/gcg-eval/src/weakness_suite.rs`):
//! `choice` (the agent's decision is one of `want`), or `turn` (the agent plays its seat until
//! the turn ends, the other seat `ref:standard` with the seed plus 100, then `winner` (a seat, or
//! `"none"`), `zones` (uid and zone) and `active` (uids not rested) must hold).

use std::path::PathBuf;

use gcg_ai::ex_used;
use gcg_effects::Registry;
use gcg_engine::canon::{from_canonical, state_digest};
use gcg_engine::testkit::snapshot;
use gcg_engine::{Action, ActionKind, AddOptions, DecisionKind, GameState, NewGameOptions, Zone};
use serde_json::{Value, json};

use crate::aiport::positions::{
    act1, attack_with, damage_the_unit_that_cannot_repair, play_card, rest_the_active_unit, select,
};
use crate::aiport::{FILLER, PLAYER, Sc, apply_ok, pending_player, with_cx};
use crate::p6b_support::reference;

const FILE: &str = "docs/port/eval/GAP-A1/weakness/scenarios.json";
const WRITE: &str = "GAPA1_WRITE_SCENARIOS";

/// One frozen scenario before serialisation.
struct Frozen {
    id: &'static str,
    category: &'static str,
    source: &'static str,
    state: GameState,
    check: Value,
}

fn action_json(a: Action) -> Value {
    json!([a.kind.as_str(), a.a, a.b, a.c])
}

fn choice(want: &[Action]) -> Value {
    json!({"kind": "choice", "want": want.iter().map(|&a| action_json(a)).collect::<Vec<_>>()})
}

fn turn(
    winner: Option<i64>,
    zones: &[(gcg_engine::Uid, Zone)],
    active: &[gcg_engine::Uid],
) -> Value {
    let mut o = serde_json::Map::new();
    o.insert("kind".into(), json!("turn"));
    o.insert(
        "winner".into(),
        winner.map_or_else(|| json!("none"), |w| json!(w)),
    );
    o.insert(
        "zones".into(),
        json!(
            zones
                .iter()
                .map(|&(u, z)| json!([u, z.py_name()]))
                .collect::<Vec<_>>()
        ),
    );
    o.insert("active".into(), json!(active));
    Value::Object(o)
}

/// `turn` with only the winner checked.
fn wins(player: i64) -> Value {
    turn(Some(player), &[], &[])
}

fn deployed() -> AddOptions<'static> {
    AddOptions {
        deployed_this_turn: true,
        ..AddOptions::default()
    }
}

// --- WK1 lethal detection ---------------------------------------------------------------------

fn lethal_exposed_player() -> GameState {
    let mut sc = Sc::new();
    sc.add(0, "ST01-005");
    sc.rested(1, "ST04-005");
    sc.shields(0, &[FILLER, FILLER]);
    sc.start()
}

fn lethal_face_over_free_kill() -> GameState {
    let mut sc = Sc::new();
    sc.add(0, "ST01-005");
    sc.add(0, "ST01-005");
    sc.rested(1, "ST01-008");
    sc.shields(1, &[FILLER]);
    sc.shields(0, &[FILLER, FILLER, FILLER]);
    sc.start()
}

fn lethal_rests_the_blocker() -> GameState {
    let mut sc = Sc::new();
    sc.resources(0, 3, 0, 0);
    sc.add(0, "ST01-005");
    sc.hand(0, &["ST02-014"]);
    sc.add(1, "ST01-008");
    sc.shields(0, &[FILLER, FILLER, FILLER]);
    sc.start()
}

fn pair_the_linking_pilot() -> GameState {
    let mut sc = Sc::new();
    sc.resources(0, 4, 0, 0);
    sc.add_with(0, "ST01-001", Zone::Battle, deployed());
    sc.hand(0, &["ST01-010"]);
    sc.shields(0, &[FILLER, FILLER, FILLER]);
    sc.start()
}

fn burst_aware_attack_order() -> GameState {
    let mut sc = Sc::new();
    sc.add(0, "GD01-027");
    sc.add(0, "ST04-005");
    sc.shields(1, &["ST02-014"]);
    sc.deck(1, &["ST02-014"; 20]);
    sc.shields(0, &[FILLER, FILLER, FILLER]);
    sc.start()
}

// --- WK2 lethal avoidance ---------------------------------------------------------------------

fn blocks_to_survive() -> (GameState, Action) {
    let mut sc = Sc::active(1);
    let gm = sc.add(1, "ST01-005");
    let blocker = sc.add(0, "ST01-008");
    sc.shields(1, &[FILLER, FILLER]);
    let mut st = sc.start();
    attack_with(&mut st, gm, PLAYER);
    (st, act1(ActionKind::Block, blocker))
}

fn trick_to_survive() -> GameState {
    let mut sc = Sc::active(1);
    let gm = sc.add(1, "ST01-005");
    sc.resources(0, 3, 0, 0);
    sc.hand(0, &["ST01-014"]);
    sc.shields(1, &[FILLER, FILLER]);
    let mut st = sc.start();
    attack_with(&mut st, gm, PLAYER);
    st
}

// --- WK3 attack and block ---------------------------------------------------------------------

fn favourable_trade() -> (GameState, gcg_engine::Uid, gcg_engine::Uid) {
    let mut sc = Sc::new();
    let gundam = sc.add(0, "GD01-013");
    let dagger = sc.rested(1, "ST04-005");
    sc.shields(0, &[FILLER, FILLER]);
    sc.filler_shields(1, 5);
    (sc.start(), gundam, dagger)
}

fn no_attack_into_a_bigger_blocker() -> (GameState, gcg_engine::Uid) {
    let mut sc = Sc::new();
    let gm = sc.add(0, "ST01-005");
    sc.add(1, "ST04-001");
    sc.filler_shields(0, 6);
    sc.filler_shields(1, 6);
    (sc.start(), gm)
}

fn block_when_the_blocker_wins() -> (GameState, Action) {
    let mut sc = Sc::active(1);
    let dagger = sc.add(1, "ST04-005");
    let blocker = sc.add(0, "GD01-072");
    sc.filler_shields(0, 5);
    sc.filler_shields(1, 5);
    let mut st = sc.start();
    attack_with(&mut st, dagger, PLAYER);
    (st, act1(ActionKind::Block, blocker))
}

// --- WK4 trick waste: hold up -----------------------------------------------------------------

fn hold_up() -> (GameState, gcg_engine::Uid) {
    let mut sc = Sc::new();
    sc.resources(0, 4, 3, 0);
    sc.add_with(0, "GD01-013", Zone::Battle, deployed());
    let ploy = sc.in_hand(0, "ST02-014");
    sc.add(1, "ST01-008");
    sc.resources(1, 2, 0, 0);
    sc.shields(0, &[FILLER, FILLER, FILLER, FILLER]);
    (sc.start(), ploy)
}

// --- WK6 mulligan -----------------------------------------------------------------------------

const CHEAP: [&str; 10] = [
    "ST01-005", "ST01-008", "ST02-009", "GD01-008", "GD01-011", "GD01-021", "GD01-022", "EB01-051",
    "GD01-079", "GD01-085",
];
const HEAVY: [&str; 5] = ["GD01-065", "EB01-001", "EB01-002", "EB01-004", "EB01-005"];

/// `_find_opening` (`tests/ai/test_puzzles.py:296-303`, as `ai_puzzles.rs` ports it).
fn find_opening(heavy_at_least: usize, heavy_at_most: usize) -> GameState {
    let main: Vec<&str> = CHEAP
        .iter()
        .flat_map(|n| [*n; 4])
        .chain(HEAVY.iter().flat_map(|n| [*n; 2]))
        .collect();
    let res = ["R-001"; 10];
    let reg = Registry::embedded();
    for seed in 0..2000 {
        let mut st = gcg_engine::new_game_by_number(
            &reg,
            [(&main[..], &res[..]), (&main[..], &res[..])],
            seed,
            NewGameOptions::default(),
        )
        .expect("MULLIGAN_DECK is legal");
        let first = st.pending().expect("a decision").options[0];
        apply_ok(&mut st, first);
        let p = pending_player(&st);
        let n = st
            .zone(p, Zone::Hand)
            .iter()
            .filter(|&&u| HEAVY.contains(&st.def_of(u).card_number.as_ref()))
            .count();
        if (heavy_at_least..=heavy_at_most).contains(&n) {
            return st;
        }
    }
    panic!("no opening hand found");
}

// --- WK7 target choices -----------------------------------------------------------------------

fn removal_target() -> (GameState, Action) {
    let mut sc = Sc::new();
    sc.resources(0, 2, 0, 0);
    let combat = sc.in_hand(0, "ST03-013");
    let aile = sc.add_with(
        1,
        "ST04-001",
        Zone::Battle,
        AddOptions {
            damage: 2,
            ..AddOptions::default()
        },
    );
    sc.add(1, "ST04-005");
    sc.add(1, "ST04-003");
    let mut st = sc.start();
    play_card(&mut st, combat, None);
    (st, select(aile))
}

fn excess_weakest() -> (GameState, Action) {
    let mut sc = Sc::new();
    sc.resources(0, 2, 0, 0);
    for _ in 0..5 {
        sc.add(0, "GD01-013");
    }
    let weak = sc.add(0, "ST01-008");
    let strike = sc.in_hand(0, "ST04-005");
    let mut st = sc.start();
    play_card(&mut st, strike, None);
    (st, select(weak))
}

/// The options of `st`'s pending decision that `keep` accepts.
fn options_where(st: &GameState, keep: impl Fn(Action) -> bool) -> Vec<Action> {
    st.pending()
        .expect("a decision")
        .options
        .iter()
        .copied()
        .filter(|&a| keep(a))
        .collect()
}

fn scenarios() -> Vec<Frozen> {
    let mut out = Vec::new();
    let mut push = |id, category, source, state: GameState, check| {
        out.push(Frozen {
            id,
            category,
            source,
            state,
            check,
        });
    };
    // WK1
    push(
        "wk1_lethal_attack_the_exposed_player",
        "WK1",
        "tests/ai/test_puzzles.py::test_lethal_attack_the_exposed_player",
        lethal_exposed_player(),
        wins(0),
    );
    push(
        "wk1_lethal_goes_face_instead_of_taking_a_free_kill",
        "WK1",
        "tests/ai/test_puzzles.py::test_lethal_goes_face_instead_of_taking_a_free_kill",
        lethal_face_over_free_kill(),
        wins(0),
    );
    push(
        "wk1_lethal_rests_the_blocker_before_attacking",
        "WK1",
        "tests/ai/test_puzzles.py::test_lethal_rests_the_blocker_before_attacking",
        lethal_rests_the_blocker(),
        wins(0),
    );
    push(
        "wk1_pair_the_linking_pilot",
        "WK1",
        "tests/ai/test_puzzles.py::test_lethal_pairs_the_linking_pilot_to_attack_on_deploy_turn",
        pair_the_linking_pilot(),
        wins(0),
    );
    push(
        "wk1_burst_aware_attack_order",
        "WK1",
        "tests/ai/test_puzzles.py::test_burst_aware_attack_order",
        burst_aware_attack_order(),
        wins(0),
    );
    push(
        "wk1_four_attacks_through_one_blocker",
        "WK1",
        "crates/gcg-search/tests/scenarios/p6b_w16_fast_lethal.rs (W-16)",
        crate::p6b_w16_fast_lethal::four_through_one("GD01-021", 2),
        wins(0),
    );
    push(
        "wk1_lethal_past_a_blocker_that_cannot_block",
        "WK1",
        "crates/gcg-search/tests/scenarios/p6b_w32_cant_block.rs (W-32 b)",
        crate::p6b_w32_cant_block::cant_block(3, 3, 2),
        wins(0),
    );
    // WK2
    let (st, want) = blocks_to_survive();
    push(
        "wk2_blocks_to_survive",
        "WK2",
        "tests/ai/test_puzzles.py::test_blocks_to_survive",
        st,
        choice(&[want]),
    );
    push(
        "wk2_uses_an_action_step_trick_to_survive",
        "WK2",
        "tests/ai/test_puzzles.py::test_uses_an_action_step_trick_to_survive",
        trick_to_survive(),
        turn(None, &[], &[]),
    );
    let (st, blocker) = crate::p6b_w03_last_blocker::last_blocker();
    push(
        "wk2_keeps_the_last_blocker",
        "WK2",
        "crates/gcg-search/tests/scenarios/p6b_w03_last_blocker.rs (W-03)",
        st,
        turn(None, &[], &[blocker]),
    );
    // WK3
    let (st, gundam, dagger) = favourable_trade();
    push(
        "wk3_takes_the_favourable_trade",
        "WK3",
        "tests/ai/test_puzzles.py::test_takes_the_favourable_trade",
        st,
        turn(None, &[(dagger, Zone::Trash), (gundam, Zone::Battle)], &[]),
    );
    let (st, gm) = no_attack_into_a_bigger_blocker();
    push(
        "wk3_does_not_attack_into_a_bigger_blocker",
        "WK3",
        "tests/ai/test_puzzles.py::test_does_not_attack_into_a_bigger_blocker",
        st,
        turn(None, &[(gm, Zone::Battle)], &[]),
    );
    let (st, want) = block_when_the_blocker_wins();
    push(
        "wk3_blocks_when_the_blocker_wins_the_fight",
        "WK3",
        "tests/ai/test_puzzles.py::test_blocks_when_the_blocker_wins_the_fight",
        st,
        choice(&[want]),
    );
    // WK4
    let (st, ploy) = hold_up();
    push(
        "wk4_hold_up",
        "WK4",
        "tests/ai/test_puzzles.py::test_holds_removal_for_next_turns_lethal (first turn)",
        st,
        turn(None, &[(ploy, Zone::Hand)], &[]),
    );
    // WK5
    let st = crate::p6e_w09_ex_payment::ex_payment();
    let want = options_where(&st, |a| {
        a.kind == ActionKind::PlayCommand && ex_used(a) >= 1
    });
    push(
        "wk5_ex_variant",
        "WK5",
        "crates/gcg-search/tests/scenarios/p6e_w09_ex_payment.rs (W-09)",
        st,
        choice(&want),
    );
    // WK6
    push(
        "wk6_mulligans_a_hand_of_uncastable_cards",
        "WK6",
        "tests/ai/test_puzzles.py::test_mulligans_a_hand_of_uncastable_cards",
        find_opening(4, 5),
        choice(&[Action::of(ActionKind::Redraw)]),
    );
    push(
        "wk6_keeps_a_hand_with_early_plays",
        "WK6",
        "tests/ai/test_puzzles.py::test_keeps_a_hand_with_early_plays",
        find_opening(0, 0),
        choice(&[Action::of(ActionKind::Keep)]),
    );
    // WK7
    let (st, want) = rest_the_active_unit();
    push(
        "wk7_rest_the_active_unit",
        "WK7",
        "tests/ai/test_puzzles_targets.py::rest_the_active_unit",
        st,
        choice(&[want]),
    );
    let (st, want) = damage_the_unit_that_cannot_repair();
    push(
        "wk7_damage_the_unit_that_cannot_repair",
        "WK7",
        "tests/ai/test_puzzles_targets.py::damage_the_unit_that_cannot_repair (W-14)",
        st,
        choice(&[want]),
    );
    let (st, want) = removal_target();
    push(
        "wk7_removal_targets_the_most_valuable_unit_it_can_destroy",
        "WK7",
        "tests/ai/test_puzzles.py::test_removal_targets_the_most_valuable_unit_it_can_destroy",
        st,
        choice(&[want]),
    );
    let (st, want) = excess_weakest();
    push(
        "wk7_excess_trashes_the_weakest_unit",
        "WK7",
        "tests/ai/test_puzzles.py::test_excess_trashes_the_weakest_unit",
        st,
        choice(&[want]),
    );
    // WK8
    let st = crate::p6a_w01_two_turn_plan::position();
    let want = options_where(&st, |a| a.kind != ActionKind::Attack || a.b < 0);
    push(
        "wk8_two_turn_plan",
        "WK8",
        "crates/gcg-search/tests/scenarios/p6a_w01_two_turn_plan.rs (W-01)",
        st,
        choice(&want),
    );
    out
}

fn document(frozen: &[Frozen]) -> Value {
    let items: Vec<Value> = frozen
        .iter()
        .map(|f| {
            let snap = snapshot(&f.state);
            let dec = f.state.pending().expect("a decision");
            json!({
                "id": f.id,
                "category": f.category,
                "source": f.source,
                "player": dec.hdr.player,
                "decision": dec.hdr.kind.as_str(),
                "check": f.check,
                "snapshot": {
                    "state": snap.state,
                    "continuous": snap.continuous,
                    "filter_sets": snap.filter_sets,
                    "rules_checked": snap.rules_checked,
                },
            })
        })
        .collect();
    json!({
        "format": "gcg-eval-weakness-scenarios",
        "version": 1,
        "generated_by": "crates/gcg-search/tests/scenarios/gapa1_weakness_scenarios.rs",
        "plan": "docs/port/WEAKNESSES.md W-17 step (2)",
        "categories": {
            "WK1": "lethal detection",
            "WK2": "lethal avoidance",
            "WK3": "attack and block",
            "WK4": "trick waste (hold up)",
            "WK5": "resource choices (EX variant)",
            "WK6": "mulligan",
            "WK7": "target choices",
            "WK8": "long precise lines",
        },
        "scenarios": items,
    })
}

#[test]
fn the_frozen_weakness_scenarios_are_current() {
    let frozen = scenarios();
    let reg = Registry::embedded();
    for f in &frozen {
        let snap = snapshot(&f.state);
        let back = from_canonical(&reg, &snap).unwrap_or_else(|e| panic!("{}: {e:?}", f.id));
        assert_eq!(state_digest(&back), state_digest(&f.state), "{}", f.id);
        let p = pending_player(&f.state);
        let want = with_cx(|cx| reference("standard", 1).choose(&f.state.clone(), p, cx));
        let got = with_cx(|cx| reference("standard", 1).choose(&back, p, cx));
        assert_eq!(
            want.ok(),
            got.ok(),
            "{}: the reloaded state decides alike",
            f.id
        );
        if f.check["kind"] == "choice" {
            let options = f.state.pending().expect("a decision").options.to_vec();
            let wanted = f.check["want"].as_array().expect("want");
            assert!(!wanted.is_empty(), "{}: something to want", f.id);
            for w in wanted {
                assert!(
                    options.iter().any(|&a| action_json(a) == *w),
                    "{}: {w} is an option",
                    f.id
                );
            }
        }
        assert_ne!(
            f.state.pending().map(|d| d.hdr.kind),
            Some(DecisionKind::ChooseFirst),
            "{}: the harness answers CHOOSE_FIRST",
            f.id
        );
    }
    let mut ids: Vec<&str> = frozen.iter().map(|f| f.id).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), frozen.len(), "unique ids");
    let text = serde_json::to_string_pretty(&document(&frozen)).expect("json") + "\n";
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(FILE);
    if std::env::var_os(WRITE).is_some() {
        std::fs::create_dir_all(path.parent().expect("a directory")).expect("mkdir");
        std::fs::write(&path, &text).expect("write the scenarios");
    }
    let committed = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{FILE}: {e}; run with {WRITE}=1 to write it"));
    assert!(
        committed == text,
        "{FILE} is stale; run with {WRITE}=1 to rewrite it"
    );
}

/// `gcg_search::gapa1::proved_lethal` and `lethal_on_state` (the missed-lethal rule of
/// `gcg-eval weakness audit`): they prove the WK1 lethals from the decider's information, and
/// find none where the decider has no lethal (negative controls).
#[test]
fn the_audit_solver_proves_the_wk1_lethals_and_no_others() {
    let cfg = gcg_search::preset_config(gcg_search::Preset::Standard);
    let seeds = [11, 12, 13, 14];
    let frozen = scenarios();
    let mut proved = 0;
    for f in &frozen {
        let p = pending_player(&f.state);
        let found = gcg_search::gapa1::proved_lethal(&f.state, p, cfg, &seeds).expect("solve");
        let on_state = gcg_search::gapa1::lethal_on_state(&f.state, p, cfg).expect("solve");
        if f.id == "wk1_four_attacks_through_one_blocker"
            || f.id == "wk1_lethal_attack_the_exposed_player"
            || f.id == "wk1_lethal_goes_face_instead_of_taking_a_free_kill"
        {
            assert!(found.is_some(), "{}: a proved lethal", f.id);
            assert!(on_state.is_some(), "{}: lethal on the state", f.id);
            proved += 1;
        }
        if f.category != "WK1"
            && f.state
                .pending()
                .is_some_and(|d| d.hdr.kind == DecisionKind::Main)
        {
            assert_eq!(found, None, "{}: no lethal outside WK1", f.id);
        }
        assert_eq!(
            gcg_search::gapa1::proved_lethal(&f.state, 1 - p, cfg, &seeds).expect("solve"),
            None,
            "{}: only the decider can have a lethal at its decision",
            f.id
        );
    }
    assert_eq!(proved, 3);
}
