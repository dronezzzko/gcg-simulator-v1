//! P6-J engine hardening (docs/dev/PLAN.md §12.2 P6-J; docs/dev/eval/weakness-status/P6-J.md):
//! the engine-side evidence of W-46, W-47, W-48, W-50, W-53, W-125, W-143, W-159, W-161 and
//! W-162 on the Rust testkit, each pinned to the result of the same position on the Python
//! testkit.
//!
//! The test-only scripts (`*_FRAGMENT` at the end of the file) are IR fragments of a
//! `registry.override_script` with a hand-written DSL script, exported by
//! `oracle/export_ir.py` `build_fragment` in a fresh Python process, as
//! `oracle/p6k_vectors.py` `hp_fragment` does:
//!
//! ```text
//! reg = get_registry(); cdef = reg.db[number]
//! base = json.loads(Path("crates/gcg-effects/ir/registry.json").read_text())
//! override_script(reg, cdef, script); core.clear_listen_cache()
//! fragment = export_ir.build_fragment(reg, base, [cdef.def_id])
//! ```
//!
//! with each constant's `script` in its doc comment; the Python results quoted in the tests
//! come from the same process, on `gcg_sim.testkit` with the same calls.

mod l5_common;

use std::sync::Arc;

use gcg_core::rng::SplitMix64;
use gcg_data::CardDb;
use gcg_effects::{ContRef, Duration, Kw, Registry};
use gcg_engine::canon::{from_canonical, state_digest};
use gcg_engine::testkit::{
    activate, attack, engine as tk, order, play, select, snapshot, to_next_turn, vanilla_deck, yes,
    zone_of,
};
use gcg_engine::{
    Action, ActionKind, AddOptions, ApplyError, CapKind, DecisionKind, EndReason, EngineError,
    GameState, LastingArgs, NO_PROGRESS_STEPS, NewGameOptions, Scenario, ScenarioOptions,
    StateSnapshot, Step, TestkitError, Uid, Zone, apply, legal_actions, new_game, view,
};
use serde_json::Value;

const VANILLA_2_2: &str = "GD01-060"; // Zaku Mariner, Lv2 2/2

fn scenario(reg: &Arc<Registry>) -> Scenario {
    Scenario::new(reg, ScenarioOptions::default())
}

fn add(sc: &mut Scenario, player: u8, number: &str, zone: Zone) -> Uid {
    sc.add(player, number, zone, AddOptions::default())
        .unwrap_or_else(|e| panic!("{number}: {e:?}"))
}

fn with_fragment(text: &str) -> Arc<Registry> {
    let fragment: Value = serde_json::from_str(text).expect("a JSON fragment");
    Arc::new(
        Registry::embedded()
            .with_fragment(&fragment)
            .expect("the fragment applies"),
    )
}

/// `(kind, player, [str(option)...])` of the pending decision, as the Python scripts print it.
fn pending(st: &GameState) -> Option<(DecisionKind, u8, Vec<String>)> {
    st.pending().map(|d| {
        (
            d.hdr.kind,
            d.hdr.player,
            d.options.iter().map(ToString::to_string).collect(),
        )
    })
}

fn strings(xs: &[&str]) -> Vec<String> {
    xs.iter().map(|&s| s.to_owned()).collect()
}

fn option_with(st: &GameState, kind: ActionKind, a: Uid) -> Action {
    *legal_actions(st)
        .iter()
        .find(|o| o.kind == kind && o.a == i32::from(a))
        .unwrap_or_else(|| panic!("no {kind:?}({a}) in {:?}", pending(st)))
}

// ---------------------------------------------------------------------------------------------
// W-46: which safety cap ended the game

/// Both caps end the game as `turn_limit` draws with Python's final state, and `cap_kind`
/// tells them apart. Python, same decks, seed and random play:
///
/// ```text
/// decks = (vanilla_deck("Red"), vanilla_deck("Blue"))
/// st = new_game(decks, 7, max_actions=10); run_random(st, 11)   # or turn_limit=2
/// print(st.winner, st.end_reason.value, st.turn, st.action_count, state_digest(st))
/// decisions -1 turn_limit 4 11 a3500906b0cf96cf3b3102a7b5d0c16335c86426d666c47b5e8a738982f09974
/// turns -1 turn_limit 2 7 1bd04bf20b0419d83aa6b23b6d29ec0c4b2e17a35d8a8ebe4e357d835abb2d4d
/// ```
#[test]
fn w46_both_caps_report_turn_limit_and_name_the_cap() {
    let reg = Registry::embedded();
    let db = reg.db();
    let red = vanilla_deck(db, "Red", "R-001").expect("a red deck");
    let blue = vanilla_deck(db, "Blue", "R-001").expect("a blue deck");
    let cases = [
        (
            CapKind::Decisions,
            NewGameOptions {
                max_actions: 10,
                ..NewGameOptions::default()
            },
            (4, 11),
            "a3500906b0cf96cf3b3102a7b5d0c16335c86426d666c47b5e8a738982f09974",
        ),
        (
            CapKind::Turns,
            NewGameOptions {
                turn_limit: 2,
                ..NewGameOptions::default()
            },
            (2, 7),
            "1bd04bf20b0419d83aa6b23b6d29ec0c4b2e17a35d8a8ebe4e357d835abb2d4d",
        ),
    ];
    for (cap, opts, (turn, actions), digest) in cases {
        let mut st = new_game(&reg, [&red, &blue], 7, opts).expect("a game");
        assert_eq!(st.cap_kind(), None, "a running game");
        gcg_engine::testkit::run_random(&mut st, 11, 5000).expect("plays");
        assert_eq!(st.winner(), Some(-1));
        assert_eq!(st.end_reason(), Some(EndReason::TurnLimit));
        assert_eq!((st.turn(), st.action_count()), (turn, actions));
        assert_eq!(state_digest(&st), digest, "{cap:?}: Python's final state");
        assert_eq!(st.cap_kind(), Some(cap));
        let reloaded = from_canonical(&reg, &snapshot(&st)).expect("reloads");
        assert_eq!(reloaded.cap_kind(), Some(cap), "the saved state keeps it");
    }
}

/// Games that end any other way have no cap kind.
#[test]
fn w46_other_endings_have_no_cap_kind() {
    let reg = Registry::embedded();
    let db = reg.db();
    let red = vanilla_deck(db, "Red", "R-001").expect("a red deck");
    let mut st = new_game(&reg, [&red, &red], 3, NewGameOptions::default()).expect("a game");
    gcg_engine::testkit::run_random(&mut st, 5, 20_000).expect("plays");
    assert!(st.game_over());
    assert_ne!(st.end_reason(), Some(EndReason::TurnLimit));
    assert_eq!(st.cap_kind(), None);
    let mut st = new_game(&reg, [&red, &red], 3, NewGameOptions::default()).expect("a game");
    gcg_engine::concede(&mut st, 0);
    assert_eq!(st.end_reason(), Some(EndReason::Concede));
    assert_eq!(st.cap_kind(), None);
}

// ---------------------------------------------------------------------------------------------
// W-47: a decision-free loop inside advance

/// Two delayed triggers re-trigger each other ("when a friendly Unit is rested, set it as
/// active" and "when it is set as active, rest it"), then the Command rests a Unit: no
/// decision is ever asked. Python loops forever (its run was stopped after 2,000 RESTED
/// events with `action_count` 1); Rust returns `NoProgress` after `NO_PROGRESS_STEPS`
/// iterations of `advance`.
#[test]
fn w47_mutually_retriggering_delayed_triggers_end_in_no_progress() {
    let reg = with_fragment(LOOP_FRAGMENT);
    let mut sc = scenario(&reg);
    let unit = add(&mut sc, 0, VANILLA_2_2, Zone::Battle);
    let cmd = sc.hand(0, &["GD05-111"]).expect("the command")[0];
    sc.resources(0, 2, 0, 0).expect("resources");
    sc.start(Step::Main).expect("starts");
    assert_eq!((unit, cmd), (0, 1), "Python's uids");
    let st = sc.state_mut();
    assert_eq!(
        play(st, cmd, None, 0),
        Err(TestkitError::Apply(ApplyError::Engine(
            EngineError::NoProgress {
                steps: NO_PROGRESS_STEPS
            }
        )))
    );
    assert_eq!(st.action_count(), 1, "the play was the last decision");
}

// ---------------------------------------------------------------------------------------------
// W-48: the fixpoint cap

/// GD02-076 (AP 4, "while this Unit has 5 or more AP, it gains <Blocker>") with a lasting
/// AP+1 needs a confirming pass. Python: `before 4 []`, `after 5 ['Blocker']`.
#[test]
fn w48_gd02_076_with_a_lasting_ap_plus_one_gains_blocker() -> Result<(), TestkitError> {
    let reg = Registry::embedded();
    let mut sc = scenario(&reg);
    let u = add(&mut sc, 0, "GD02-076", Zone::Battle);
    sc.start(Step::Main)?;
    let st = sc.state_mut();
    let blocker = |st: &GameState| {
        gcg_engine::testkit::keywords(st, u).map(|k| k.iter().any(|&(kw, _)| kw == Kw::Blocker))
    };
    assert_eq!((gcg_engine::testkit::ap(st, u)?, blocker(st)?), (4, false));
    let seq = st.card(u).zone_seq;
    tk::add_lasting_effect(
        st,
        ContRef::StatMod { ap: 1, hp: 0 },
        0,
        i32::from(u),
        &[(u, seq)],
        Duration::ThisTurn,
        LastingArgs::default(),
    )
    .map_err(TestkitError::Engine)?;
    let before = gcg_engine::diag::non_converged_views();
    assert_eq!((gcg_engine::testkit::ap(st, u)?, blocker(st)?), (5, true));
    assert!(view::derived(st).map_err(TestkitError::Engine)?.converged());
    assert_eq!(gcg_engine::diag::non_converged_views(), before);
    Ok(())
}

/// A test-only constant ability "while this Unit has 5 or more AP, it gets AP-1" on a 5-AP
/// Unit oscillates: the view keeps the last of the six passes, as Python does (Python: AP 5,
/// HP 4), and reports that it did not converge.
#[test]
fn w48_an_oscillating_constant_is_reported() -> Result<(), TestkitError> {
    let reg = with_fragment(OSCILLATE_FRAGMENT);
    let mut sc = scenario(&reg);
    let u = add(&mut sc, 0, "ST07-003", Zone::Battle);
    sc.start(Step::Main)?;
    let st = sc.state_mut();
    gcg_engine::testkit::engine::touch(st);
    let before = gcg_engine::diag::non_converged_views();
    let dv = view::derived(st).map_err(TestkitError::Engine)?;
    assert!(!dv.converged());
    assert!(gcg_engine::diag::non_converged_views() > before);
    assert_eq!((view::ap_of(st, &dv, u), view::hp_of(st, &dv, u)), (5, 4));
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// W-50: a card added after start listens

/// A card added after `Scenario.start` joins its owner's decklists, so the events it listens
/// for are listened from then on. Python: the listened events are `['destroys_by_battle',
/// 'turn_end']` before the add and gain `'deployed'` after it; after two turns ST04-002
/// ("【Deploy】Draw 1. Then, discard 1.") deploys, the deck goes from 19 to 18 cards and a
/// DISCARD decision with 2 options is pending.
#[test]
fn w50_a_card_added_after_start_triggers() -> Result<(), TestkitError> {
    let reg = Registry::embedded();
    let mut sc = scenario(&reg);
    sc.resources(0, 6, 0, 0)?;
    sc.start(Step::Main)?;
    let x = add(&mut sc, 0, "ST04-002", Zone::Hand);
    let st = sc.state_mut();
    to_next_turn(st)?;
    to_next_turn(st)?;
    assert_eq!(st.active(), 0);
    let deck = st.zone(0, Zone::Deck).len();
    play(st, x, None, 0)?;
    assert_eq!((deck, st.zone(0, Zone::Deck).len()), (19, 18));
    assert_eq!(zone_of(st, x), Zone::Battle);
    let (kind, _, options) = pending(st).expect("a decision");
    assert_eq!((kind, options.len()), (DecisionKind::Discard, 2));
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// W-51: the HP shortcut flag

/// The override of `tests/rules/test_damage.py:791` (DINN, GD01-064: "【Deploy】Choose 1 enemy
/// Unit. It gets HP-1 during this turn."), which makes the registry's `hp_reduction_possible`
/// true (`crates/gcg-effects/ir/fragments/GD01-064.json`).
fn hp_lowering_registry() -> Arc<Registry> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../gcg-effects/ir/fragments/GD01-064.json"
    );
    let fixture: Value =
        serde_json::from_str(&std::fs::read_to_string(path).expect("the fixture")).expect("JSON");
    let reg = Arc::new(
        Registry::embedded()
            .with_fragment(&fixture["fragment"])
            .expect("applies"),
    );
    assert!(reg.hp_reduction_possible() && !Registry::embedded().hp_reduction_possible());
    reg
}

/// The d9d41af regression, as `test_a_unit_whose_hp_an_effect_lowers_to_its_damage_is_destroyed`
/// runs it in Python: with the override, the 2/2 with 1 damage whose HP DINN lowers is
/// destroyed by rules management.
#[test]
fn w51_an_hp_lowering_override_leaves_no_lethal_damaged_unit() -> Result<(), TestkitError> {
    let reg = hp_lowering_registry();
    let mut sc = scenario(&reg);
    sc.resources(0, 2, 0, 0)?;
    let dinn = add(&mut sc, 0, "GD01-064", Zone::Hand);
    let zaku = sc.add(
        1,
        VANILLA_2_2,
        Zone::Battle,
        AddOptions {
            damage: 1,
            ..AddOptions::default()
        },
    )?;
    sc.start(Step::Main)?;
    let st = sc.state_mut();
    play(st, dinn, None, 0)?;
    gcg_engine::testkit::select_if_asked(st, &[zaku], None)?;
    assert_eq!(zone_of(st, zaku), Zone::Trash);
    Ok(())
}

/// Under `paranoid`, the per-game flag (W-51) drives the printed-HP check where the registry
/// flag disables the shortcut: a game without DINN cannot lower HP, so every rules scan that
/// the shortcut would skip is shown to destroy nothing; a game with DINN in a decklist gets no
/// such check.
#[cfg(feature = "paranoid")]
#[test]
fn w51_the_per_game_flag_drives_the_printed_hp_check() -> Result<(), TestkitError> {
    use gcg_engine::paranoid::{Check, counts};
    let slot = Check::ALL
        .iter()
        .position(|&c| c == Check::NoPrintedLethalDamage)
        .expect("the check");
    let reg = hp_lowering_registry();
    for with_dinn in [false, true] {
        let mut sc = scenario(&reg);
        for p in 0..2 {
            sc.add(
                p,
                VANILLA_2_2,
                Zone::Battle,
                AddOptions {
                    damage: 1,
                    ..AddOptions::default()
                },
            )?;
        }
        if with_dinn {
            add(&mut sc, 0, "GD01-064", Zone::Hand);
        }
        let before = counts()[slot];
        sc.start(Step::Main)?;
        to_next_turn(sc.state_mut())?;
        let ran = counts()[slot] - before;
        if with_dinn {
            assert_eq!(ran, 0, "DINN can lower HP in this game");
        } else {
            assert!(ran > 0, "the per-game flag allows the check");
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// W-53: saved states reload in another process

const W53_DIR: &str = "P6J_W53_DIR";

/// A mid-game state of a random game between two example decks (one with lasting effects,
/// once keys or a resolving program, when the game has one), its snapshot, the actions that
/// finish the game, and the final digest of the uninterrupted game.
fn w53_game() -> (StateSnapshot, Vec<Action>, String) {
    let rec = l5_common::engine_records("setup_touch")[0];
    let mut st = l5_common::start_game(rec, NewGameOptions::default()).expect("a game");
    let mut rng = SplitMix64::new(53);
    let mut saved: Option<(StateSnapshot, usize)> = None;
    let mut actions = Vec::new();
    while let Some(a) = {
        let options = legal_actions(&st);
        (!options.is_empty()).then(|| options[rng.randrange_usize(options.len())])
    } {
        let rich =
            !st.lasting().is_empty() || !st.once_used().is_empty() || !st.frames().is_empty();
        if saved.is_none() && st.action_count() >= 40 && rich {
            saved = Some((snapshot(&st), actions.len()));
        }
        actions.push(a);
        apply(&mut st, a, true).expect("a legal action");
    }
    let (snap, at) = saved.expect("a mid-game state with effects");
    (snap, actions.split_off(at), state_digest(&st))
}

/// Loads `snap` into a registry built from the IR text in this process, plays `actions`
/// and returns the final digest.
fn w53_continue(snap: &StateSnapshot, actions: &[Action]) -> String {
    let ir = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../gcg-effects/ir/registry.json"
    ))
    .expect("the IR");
    let reg = Arc::new(Registry::from_ir(&ir, CardDb::embedded()).expect("a fresh registry"));
    assert_ne!(reg.generation(), Registry::embedded().generation());
    let mut st = from_canonical(&reg, snap).expect("the saved state loads");
    for &a in actions {
        apply(&mut st, a, true).expect("the same actions stay legal");
    }
    state_digest(&st)
}

fn action_json(a: &Action) -> Value {
    serde_json::json!([a.kind.as_str(), a.a, a.b, a.c])
}

fn action_of(v: &Value) -> Action {
    let kind = ActionKind::from_value(v[0].as_str().expect("a kind")).expect("an action kind");
    let int = |i: usize| i32::try_from(v[i].as_i64().expect("an int")).expect("an i32");
    Action::new(kind, int(1), int(2), int(3))
}

/// The saved form names registry entries by content id (programs, continuous effects, filter
/// sets) and spells value effects inline, so a state saved here reloads in a child process
/// whose registry is a separate instance built from the IR text, and the reloaded game ends
/// in the uninterrupted game's final state. The child is this test binary running
/// [`w53_reload_in_this_process`] with the saved files.
#[test]
fn w53_a_saved_state_reloads_in_a_fresh_process() {
    let (snap, actions, want) = w53_game();
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("p6j_w53");
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let doc = serde_json::json!({
        "state": snap.state,
        "continuous": snap.continuous,
        "filter_sets": snap.filter_sets,
        "rules_checked": snap.rules_checked,
        "actions": actions.iter().map(action_json).collect::<Vec<_>>(),
    });
    std::fs::write(dir.join("saved.json"), doc.to_string()).expect("writes");
    let _ = std::fs::remove_file(dir.join("final.txt"));
    let out = std::process::Command::new(std::env::current_exe().expect("this test binary"))
        .args(["w53_reload_in_this_process", "--exact", "--test-threads=1"])
        .env(W53_DIR, &dir)
        .output()
        .expect("the child runs");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let got = std::fs::read_to_string(dir.join("final.txt")).expect("the child's digest");
    assert_eq!(got, want);
}

/// With `P6J_W53_DIR` set (the child of [`w53_a_saved_state_reloads_in_a_fresh_process`]):
/// reloads the saved state there and writes the final digest. Without it: the same round
/// trip inside this process, through a second registry instance.
#[test]
fn w53_reload_in_this_process() {
    if let Some(dir) = std::env::var_os(W53_DIR) {
        let dir = std::path::PathBuf::from(dir);
        let doc: Value = serde_json::from_str(
            &std::fs::read_to_string(dir.join("saved.json")).expect("the saved state"),
        )
        .expect("JSON");
        let snap = StateSnapshot {
            state: doc["state"].clone(),
            continuous: doc["continuous"].clone(),
            filter_sets: doc["filter_sets"].clone(),
            rules_checked: doc["rules_checked"].as_i64().expect("rules_checked"),
        };
        let actions: Vec<Action> = doc["actions"]
            .as_array()
            .expect("actions")
            .iter()
            .map(action_of)
            .collect();
        std::fs::write(dir.join("final.txt"), w53_continue(&snap, &actions)).expect("writes");
    } else {
        let (snap, actions, want) = w53_game();
        assert_eq!(w53_continue(&snap, &actions), want);
    }
}

// ---------------------------------------------------------------------------------------------
// W-125: Action is a small value type

#[test]
fn w125_actions_compare_and_hash_by_value() {
    use std::hash::{BuildHasher, BuildHasherDefault, DefaultHasher};
    fn copy<T: Copy>(x: T) -> (T, T) {
        (x, x)
    }
    let a = Action::new(ActionKind::Attack, 5, -2, -1);
    let b = Action::new(ActionKind::Attack, 5, -2, -1);
    let c = Action::new(ActionKind::Attack, 5, -2, 0);
    let hash = |x: &Action| BuildHasherDefault::<DefaultHasher>::default().hash_one(x);
    assert_eq!(a, b);
    assert_eq!(hash(&a), hash(&b));
    assert_ne!(a, c);
    let (x, y) = copy(a);
    assert_eq!(x, y);
    assert_eq!(std::mem::size_of::<Action>(), 16);
}

// ---------------------------------------------------------------------------------------------
// W-143: battle-damage redirect chains and cycles

/// Player 0: Minerva Squad Units (GD04-062) and two GD04-095 Lunamaria Hawke in hand, 8
/// Resources; player 1: a rested GD01-060.
fn w143_position(n_units: usize) -> (Scenario, Vec<Uid>, Vec<Uid>, Uid) {
    let reg = Registry::embedded();
    let mut sc = scenario(&reg);
    let units: Vec<Uid> = (0..n_units)
        .map(|_| add(&mut sc, 0, "GD04-062", Zone::Battle))
        .collect();
    let lunas = sc.hand(0, &["GD04-095", "GD04-095"]).expect("hand");
    let enemy = sc
        .add(
            1,
            VANILLA_2_2,
            Zone::Battle,
            AddOptions {
                rested: true,
                ..AddOptions::default()
            },
        )
        .expect("the enemy");
    sc.resources(0, 8, 0, 0).expect("resources");
    sc.start(Step::Main).expect("starts");
    (sc, units, lunas, enemy)
}

/// GD04-095's 【When Linked】 on Unit X choosing Unit Y: Y's battle damage goes to X.
fn link(st: &mut GameState, luna: Uid, onto: Uid, choose: Uid) -> Result<(), TestkitError> {
    play(st, luna, Some(onto), 0)?;
    assert_eq!(
        pending(st).map(|p| p.0),
        Some(DecisionKind::Select),
        "When Linked asks for its Unit"
    );
    select(st, &[choose], None)
}

/// Python (`sys.setrecursionlimit(1000)`, the same calls): pairing each Lunamaria asks
/// `('select', 0, ['select(0)', 'select(1)'])`, and the attack raises `RecursionError:
/// maximum recursion depth exceeded`. Rust names the cycle A -> B -> A instead.
#[test]
fn w143_two_lunamarias_build_a_cycle_that_fails_the_game() -> Result<(), TestkitError> {
    let (mut sc, units, lunas, enemy) = w143_position(2);
    let (a, b) = (units[0], units[1]);
    assert_eq!((a, b, lunas[0], lunas[1], enemy), (0, 1, 2, 3, 4));
    let st = sc.state_mut();
    link(st, lunas[0], a, b)?;
    link(st, lunas[1], b, a)?;
    let err = attack(st, a, i32::from(enemy)).expect_err("the cycle fails");
    let TestkitError::Apply(ApplyError::Engine(e)) = err else {
        panic!("{err:?}");
    };
    assert_eq!(
        e,
        EngineError::RedirectCycle {
            uids: Box::new([a, b, a])
        }
    );
    assert_eq!(e.py_class(), "RecursionError");
    Ok(())
}

/// A chain without a cycle lands where Python's recursion lands: A's battle damage goes to
/// B, and B's to C. Python: `damage {0: ('BATTLE', 0), 1: ('BATTLE', 0), 2: ('BATTLE', 2), 5:
/// ('TRASH', 0)}`, and the next decision is player 0's main phase.
#[test]
fn w143_a_redirect_chain_lands_on_its_last_unit() -> Result<(), TestkitError> {
    let (mut sc, units, lunas, enemy) = w143_position(3);
    let (a, b, c) = (units[0], units[1], units[2]);
    assert_eq!((a, b, c, enemy), (0, 1, 2, 5));
    let st = sc.state_mut();
    link(st, lunas[0], b, a)?;
    link(st, lunas[1], c, b)?;
    attack(st, a, i32::from(enemy))?;
    assert_eq!(
        pending(st).map(|p| (p.0, p.1)),
        Some((DecisionKind::Main, 0))
    );
    let seen: Vec<(Zone, i32)> = [a, b, c, enemy]
        .iter()
        .map(|&u| (zone_of(st, u), st.card(u).damage))
        .collect();
    assert_eq!(
        seen,
        [
            (Zone::Battle, 0),
            (Zone::Battle, 0),
            (Zone::Battle, 2),
            (Zone::Trash, 0)
        ]
    );
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// W-159: turn-history conditions read the card's current state

/// GD05-089 deployed by its Burst "as an (AP3･HP3) Unit", then destroyed by GD05-057
/// Gyunei's Jagd Doga (a Neo Zeon card's effect), then GD05-129 Axis: "If one of your Units
/// has been destroyed by one of your (Neo Zeon) card's effects during this turn, deploy 1
/// (Neo Zeon) Unit card that is Lv.3 or lower from your hand." The history entry records the
/// event, but the condition matches its UNIT filter against the card as it is now, a Pilot
/// in the trash, so Axis deploys nothing. The control destroys a printed Unit instead, and
/// Axis deploys GD02-050. Python, the same calls:
///
/// ```text
/// as unit:  victim TRASH history [('destroyed', 6, 759, 10)]  ...  neo zone HAND
/// control:  victim TRASH history [('destroyed', 11, 208, 10)] ...  neo zone BATTLE
/// ```
///
/// Rust keeps this behaviour exactly (a rules question for the Python conflict workflow).
#[test]
fn w159_a_unit_that_is_a_pilot_again_does_not_count() -> Result<(), TestkitError> {
    for (as_unit, neo_zone) in [(true, Zone::Hand), (false, Zone::Battle)] {
        let reg = Registry::embedded();
        let mut sc = scenario(&reg);
        sc.resources(1, 5, 0, 0)?;
        let attacker = add(&mut sc, 0, VANILLA_2_2, Zone::Battle);
        let asia = sc.shields(1, &["GD05-089"])?[0];
        sc.trash(1, &["GD05-042", "GD05-043", "GD05-042"])?;
        let gyunei = add(&mut sc, 1, "GD05-057", Zone::Battle);
        let other = add(&mut sc, 1, VANILLA_2_2, Zone::Battle);
        let neo = sc.hand(1, &["GD02-050"])?[0];
        sc.start(Step::Main)?;
        attack(sc.state_mut(), attacker, gcg_engine::PLAYER_TARGET)?;
        yes(sc.state_mut())?;
        assert_eq!(
            pending(sc.state()).map(|p| p.0),
            Some(DecisionKind::YesNo),
            "deploy it as a Unit instead?"
        );
        yes(sc.state_mut())?;
        assert_eq!(zone_of(sc.state(), asia), Zone::Battle);
        assert_eq!(sc.state().card(asia).def_id.0, 1152, "the Unit variant");
        let axis = add(&mut sc, 1, "GD05-129", Zone::Base);
        let st = sc.state_mut();
        to_next_turn(st)?;
        assert_eq!(st.active(), 1);
        let victim = if as_unit { asia } else { other };
        activate(st, gyunei, None, 0)?;
        assert_eq!(
            pending(st).map(|p| p.2),
            Some(strings(&["select(11)", "select(6)"]))
        );
        select(st, &[victim], None)?;
        assert_eq!(zone_of(st, victim), Zone::Trash);
        let destroyed: Vec<(Uid, i32, i32)> = st
            .history()
            .iter()
            .filter(|h| h.kind == gcg_engine::HistKind::Destroyed)
            .map(|h| (u16::try_from(h.uid).expect("a uid"), h.def_id, h.source))
            .collect();
        let def = if as_unit { 759 } else { 208 };
        assert_eq!(destroyed, [(victim, def, i32::from(gyunei))]);
        activate(st, axis, None, 0)?;
        assert_eq!(zone_of(st, neo), neo_zone, "as unit: {as_unit}");
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// W-161: ToDeck with a targeted Unit's Pilot also targeted

/// A test-only Command returns an enemy Unit and its paired Pilot to the top of the deck
/// (`ToDeck(Union((Var('t1'), PairedPilotOf(Var('t1')))), bottom=False)`): the Pilot is both a
/// target and the Unit's Pilot, so the owner's ARRANGE offers it twice, and choosing the Unit
/// first asks a second ARRANGE whose two options are the same card. Python, the same calls:
///
/// ```text
/// first:  arrange 1 ['select(3)', 'select(4)', 'select(4)']
/// unit:   arrange [..., ('arrange', 1, ['select(4)', 'select(4)'])], action_count 3, deck top [4, 3, 25]
/// pilot:  arrange [one decision], action_count 2, deck top [4, 3, 25]
/// ```
#[test]
fn w161_duplicate_arrange_options_are_pythons() -> Result<(), TestkitError> {
    let reg = with_fragment(TODECK_FRAGMENT);
    for unit_first in [true, false] {
        let mut sc = scenario(&reg);
        let cmd = sc.hand(0, &["GD05-111"])?[0];
        sc.resources(0, 2, 0, 0)?;
        let unit = sc.add(
            1,
            VANILLA_2_2,
            Zone::Battle,
            AddOptions {
                pilot: Some("GD05-091"),
                ..AddOptions::default()
            },
        )?;
        let pilot = Uid::try_from(sc.state().card(unit).pair).expect("paired");
        assert_eq!((cmd, unit, pilot), (0, 3, 4));
        sc.start(Step::Main)?;
        let st = sc.state_mut();
        play(st, cmd, None, 0)?;
        let mut decisions = Vec::new();
        while let Some((DecisionKind::Arrange, player, options)) = pending(st) {
            decisions.push((player, options));
            let want = if unit_first && decisions.len() == 1 {
                unit
            } else {
                pilot
            };
            apply(st, option_with(st, ActionKind::Select, want), true)
                .map_err(TestkitError::Apply)?;
        }
        let mut want = vec![(1, strings(&["select(3)", "select(4)", "select(4)"]))];
        if unit_first {
            want.push((1, strings(&["select(4)", "select(4)"])));
        }
        assert_eq!(decisions, want);
        assert_eq!(pending(st).map(|p| p.0), Some(DecisionKind::Main));
        assert_eq!(st.zone(1, Zone::Deck)[..3], [4, 3, 25]);
        assert_eq!(st.action_count(), if unit_first { 3 } else { 2 });
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// W-162: a trigger whose host moved again before it resolved

/// Two copies of a test-only ST07-003 "【Destroyed】Choose 1 Unit card from your trash.
/// Return it to your hand." with lethal damage are destroyed together; the first trigger
/// returns the second copy, whose own trigger then resolves from the hand. Python, the same
/// calls: `order_trigger [order(0), order(1)]`, then `select [select(0), select(1)]`, then
/// main; both copies end in the hand with `zone_seq` 2. One copy alone returns itself, with no
/// ORDER_TRIGGER decision. Under `paranoid`, the side table records the second copy (queued at
/// `zone_seq` 1 in the trash, resolved at 2 in the hand) and nothing for one copy.
#[test]
fn w162_a_host_that_moves_before_its_trigger_resolves() -> Result<(), TestkitError> {
    let reg = with_fragment(MOVED_FRAGMENT);
    for n in [1usize, 2] {
        let mut sc = scenario(&reg);
        let us: Vec<Uid> = (0..n)
            .map(|_| {
                sc.add(
                    0,
                    "ST07-003",
                    Zone::Battle,
                    AddOptions {
                        damage: 4,
                        ..AddOptions::default()
                    },
                )
                .expect("adds")
            })
            .collect();
        #[cfg(feature = "paranoid")]
        let before = gcg_engine::paranoid::moved_hosts().len();
        sc.start(Step::Main)?;
        let st = sc.state_mut();
        let mut trace = vec![pending(st)];
        while let Some((kind, _, _)) = pending(st) {
            if kind == DecisionKind::Main {
                break;
            }
            if kind == DecisionKind::OrderTrigger {
                order(st, 0)?;
            } else {
                let other = *legal_actions(st)
                    .iter()
                    .find(|o| o.a != i32::from(us[0]))
                    .expect("the other copy");
                apply(st, other, true).map_err(TestkitError::Apply)?;
            }
            trace.push(pending(st));
        }
        let kinds: Vec<DecisionKind> = trace.iter().flatten().map(|p| p.0).collect();
        if n == 1 {
            assert_eq!(kinds, [DecisionKind::Main]);
        } else {
            assert_eq!(
                kinds,
                [
                    DecisionKind::OrderTrigger,
                    DecisionKind::Select,
                    DecisionKind::Main
                ]
            );
            assert_eq!(
                trace[1].as_ref().map(|p| p.2.clone()),
                Some(strings(&["select(0)", "select(1)"]))
            );
        }
        for &u in &us {
            assert_eq!((zone_of(st, u), st.card(u).zone_seq), (Zone::Hand, 2));
        }
        #[cfg(feature = "paranoid")]
        {
            let moved = gcg_engine::paranoid::moved_hosts();
            let new = &moved[before..];
            if n == 1 {
                assert!(new.is_empty(), "{new:?}");
            } else {
                assert_eq!(
                    new,
                    [gcg_engine::paranoid::MovedHost {
                        card_number: "ST07-003".into(),
                        queued_seq: 1,
                        resolved_seq: 2,
                    }]
                );
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Test-only scripts (see the module docs)

/// `GD05-111` overridden with `CardScript(card_number='GD05-111', abilities=(Command(timing=<Timing.MAIN: 'main'>, steps=(DelayedTrigger(trigger=Trigger(event=<Ev.RESTED: 'rested'>, self_only=False, subject=Sel(side=<Side.FRIENDLY: 'friendly'>, loc=<Loc.BATTLE: 'battle'>, filters=(IsKind(kinds=(<CardKind.UNIT: 'unit'>,)),), top_n=0), whose_turn=None, pilot_filters=(), target_filters=(), battle_only=None, by_enemy=None, include_self=True), steps=(SetActive(ref=EventCard(key='subject')),), duration=<Duration.THIS_TURN: 'this_turn'>, bind_vars=()), DelayedTrigger(trigger=Trigger(event=<Ev.SET_ACTIVE: 'set_active'>, self_only=False, subject=Sel(side=<Side.FRIENDLY: 'friendly'>, loc=<Loc.BATTLE: 'battle'>, filters=(IsKind(kinds=(<CardKind.UNIT: 'unit'>,)),), top_n=0), whose_turn=None, pilot_filters=(), target_filters=(), battle_only=None, by_enemy=None, include_self=True), steps=(Rest(ref=EventCard(key='subject')),), duration=<Duration.THIS_TURN: 'this_turn'>, bind_vars=()), Rest(ref=All(sel=Sel(side=<Side.FRIENDLY: 'friendly'>, loc=<Loc.BATTLE: 'battle'>, filters=(IsKind(kinds=(<CardKind.UNIT: 'unit'>,)),), top_n=0)))), cond=And(conds=())),), unit_abilities=(), source='binding', notes='')`.
const LOOP_FRAGMENT: &str = r#"{"abilities":[{"ability":4484,"card_number":"GD05-111","cost_program_id":-1,"def_id":781,"index":0,"program_id":626,"trigger_event":null,"unit_text":false}],"aliases":[],"always_events":["destroys_by_battle","rested","set_active","turn_end"],"base_fingerprint":"a61a7e50899ab6ff6259b0a2a082be5af45cf3fa28b5012c95597a789688748b","cards":[{"binding_module":null,"burst_aid":-1,"card_number":"GD05-111","command_aid":1260,"def_id":781,"delayed":[[0,4479,627],[1,4482,628]],"events":["rested","set_active"],"field_entries":[1260],"gated":false,"hand_entries":[],"has_burst":false,"name":"Airframe\u200b Seizure","own":[1260],"paired_entries":[],"printed_keywords":[],"script":4485,"source":"binding","template":null,"text_hash":"14a32d8f48ac7e27","timings":["main"],"trash_entries":[],"unit":[],"unit_entries":[]}],"continuous":[],"events":[[781,["rested","set_active"]]],"filter_sets":[],"fragment_version":1,"hp_reduction_possible":false,"node_content_ids":["352c40854dfb6aa7","87285dc74a0ddcb3","63e5e8e564f44b35","cbf0ac5e1dad79ba","4b9ef234cca39e46","3baefbbb0b4d8349","697ebbfa8064366d","a82c47cbe93a15f6","a4678f87c3d195bf"],"nodes":[{"_":"Trigger","battle_only":null,"by_enemy":null,"event":"rested","include_self":true,"pilot_filters":[],"self_only":false,"subject":{"$":64},"target_filters":[],"whose_turn":null},{"_":"SetActive","ref":{"$":197}},{"_":"DelayedTrigger","bind_vars":[],"duration":"this_turn","steps":[{"$":4478}],"trigger":{"$":4477}},{"_":"Trigger","battle_only":null,"by_enemy":null,"event":"set_active","include_self":true,"pilot_filters":[],"self_only":false,"subject":{"$":64},"target_filters":[],"whose_turn":null},{"_":"Rest","ref":{"$":197}},{"_":"DelayedTrigger","bind_vars":[],"duration":"this_turn","steps":[{"$":4481}],"trigger":{"$":4480}},{"_":"Rest","ref":{"$":1004}},{"_":"Command","cond":{"$":13},"steps":[{"$":4479},{"$":4482},{"$":4483}],"timing":"main"},{"_":"CardScript","abilities":[{"$":4484}],"card_number":"GD05-111","notes":"","source":"binding","unit_abilities":[]}],"printable_non_ascii":[],"printed_keywords":[],"programs":[{"content_id":"12650f4b80f2e7a7","instrs":[4479,4482,4483],"label":"GD05-111#a0"},{"content_id":"33209f811240690f","instrs":[4478],"label":"GD05-111#delayed"},{"content_id":"a05aa534620e1096","instrs":[4481],"label":"GD05-111#delayed"}],"rule_mods":[],"strings":["GD05-111#delayed"],"var_names":[]}"#;

/// `ST07-003` overridden with `CardScript(card_number='ST07-003', abilities=(Constant(effects=(StatMod(ap=-1, hp=0),), scope=This(), cond=RefMatches(ref=This(), filters=(StatCmp(stat=<Stat.AP: 'ap'>, op=<Op.GE: '>='>, value=5),)), where=<Where.FIELD: 'field'>, gate=<Gate.NONE: 'none'>, gate_filters=()),), unit_abilities=(), source='binding', notes='')`.
const OSCILLATE_FRAGMENT: &str = r#"{"abilities":[{"ability":4477,"card_number":"ST07-003","cost_program_id":-1,"def_id":1005,"index":0,"program_id":-1,"trigger_event":null,"unit_text":false}],"aliases":[],"always_events":["destroys_by_battle","turn_end"],"base_fingerprint":"a61a7e50899ab6ff6259b0a2a082be5af45cf3fa28b5012c95597a789688748b","cards":[{"binding_module":null,"burst_aid":-1,"card_number":"ST07-003","command_aid":-1,"def_id":1005,"delayed":[],"events":[],"field_entries":[1260],"gated":false,"hand_entries":[],"has_burst":false,"name":"Gundam Virtue","own":[1260],"paired_entries":[],"printed_keywords":[],"script":4478,"source":"binding","template":null,"text_hash":"3973e022e93220f9","timings":[],"trash_entries":[],"unit":[],"unit_entries":[]}],"continuous":[],"events":[],"filter_sets":[],"fragment_version":1,"hp_reduction_possible":false,"node_content_ids":["20cd1079c5c56efc","acf8261df8e59151"],"nodes":[{"_":"Constant","cond":{"$":814},"effects":[{"$":904}],"gate":"none","gate_filters":[],"scope":{"$":17},"where":"field"},{"_":"CardScript","abilities":[{"$":4477}],"card_number":"ST07-003","notes":"","source":"binding","unit_abilities":[]}],"printable_non_ascii":[],"printed_keywords":[],"programs":[],"rule_mods":[],"strings":[],"var_names":[]}"#;

/// `GD05-111` overridden with `CardScript(card_number='GD05-111', abilities=(Command(timing=<Timing.MAIN: 'main'>, steps=(Choose(var='t1', sel=Sel(side=<Side.ENEMY: 'enemy'>, loc=<Loc.BATTLE: 'battle'>, filters=(IsKind(kinds=(<CardKind.UNIT: 'unit'>,)),), top_n=0), count=1, min_count=None, optional=False, chooser=<P.YOU: 'you'>, distinct_from=(), targeting=True, random=False, after_then=False), ToDeck(ref=Union(refs=(Var(name='t1'), PairedPilotOf(ref=Var(name='t1')))), bottom=False, shuffle=False)), cond=And(conds=())),), unit_abilities=(), source='binding', notes='')`.
const TODECK_FRAGMENT: &str = r#"{"abilities":[{"ability":4480,"card_number":"GD05-111","cost_program_id":-1,"def_id":781,"index":0,"program_id":626,"trigger_event":null,"unit_text":false}],"aliases":[],"always_events":["destroys_by_battle","turn_end"],"base_fingerprint":"a61a7e50899ab6ff6259b0a2a082be5af45cf3fa28b5012c95597a789688748b","cards":[{"binding_module":null,"burst_aid":-1,"card_number":"GD05-111","command_aid":1260,"def_id":781,"delayed":[],"events":[],"field_entries":[1260],"gated":false,"hand_entries":[],"has_burst":false,"name":"Airframe\u200b Seizure","own":[1260],"paired_entries":[],"printed_keywords":[],"script":4481,"source":"binding","template":null,"text_hash":"14a32d8f48ac7e27","timings":["main"],"trash_entries":[],"unit":[],"unit_entries":[]}],"continuous":[],"events":[],"filter_sets":[],"fragment_version":1,"hp_reduction_possible":false,"node_content_ids":["796d579e0af5bd9b","b0f2c544ec627da0","6e0468d4513f3df1","41da6692059c00a8","c0bc8c97729b07d1"],"nodes":[{"_":"PairedPilotOf","ref":{"$":8}},{"_":"Union","refs":[{"$":8},{"$":4477}]},{"_":"ToDeck","bottom":false,"ref":{"$":4478},"shuffle":false},{"_":"Command","cond":{"$":13},"steps":[{"$":23},{"$":4479}],"timing":"main"},{"_":"CardScript","abilities":[{"$":4480}],"card_number":"GD05-111","notes":"","source":"binding","unit_abilities":[]}],"printable_non_ascii":[],"printed_keywords":[],"programs":[{"content_id":"3c6c707b3f5c0066","instrs":[23,4479],"label":"GD05-111#a0"}],"rule_mods":[],"strings":[],"var_names":[]}"#;

/// `ST07-003` overridden with `CardScript(card_number='ST07-003', abilities=(Triggered(trigger=Trigger(event=<Ev.DESTROYED: 'destroyed'>, self_only=True, subject=None, whose_turn=None, pilot_filters=(), target_filters=(), battle_only=None, by_enemy=None, include_self=True), steps=(Choose(var='t1', sel=Sel(side=<Side.FRIENDLY: 'friendly'>, loc=<Loc.TRASH: 'trash'>, filters=(IsKind(kinds=(<CardKind.UNIT: 'unit'>,)),), top_n=0), count=1, min_count=None, optional=False, chooser=<P.YOU: 'you'>, distinct_from=(), targeting=True, random=False, after_then=False), ReturnToHand(ref=Var(name='t1'))), cond=And(conds=()), once_per_turn=False, where=<Where.FIELD: 'field'>, gate=<Gate.NONE: 'none'>, gate_filters=()),), unit_abilities=(), source='binding', notes='')`.
const MOVED_FRAGMENT: &str = r#"{"abilities":[{"ability":4478,"card_number":"ST07-003","cost_program_id":-1,"def_id":1005,"index":0,"program_id":626,"trigger_event":"destroyed","unit_text":false}],"aliases":[],"always_events":["destroys_by_battle","turn_end"],"base_fingerprint":"a61a7e50899ab6ff6259b0a2a082be5af45cf3fa28b5012c95597a789688748b","cards":[{"binding_module":null,"burst_aid":-1,"card_number":"ST07-003","command_aid":-1,"def_id":1005,"delayed":[],"events":["destroyed"],"field_entries":[1260],"gated":false,"hand_entries":[],"has_burst":false,"name":"Gundam Virtue","own":[1260],"paired_entries":[],"printed_keywords":[],"script":4479,"source":"binding","template":null,"text_hash":"3973e022e93220f9","timings":["destroyed"],"trash_entries":[],"unit":[],"unit_entries":[]}],"continuous":[],"events":[[1005,["destroyed"]]],"filter_sets":[],"fragment_version":1,"hp_reduction_possible":false,"node_content_ids":["11cc56235fd3b5ed","b951c6ccd40e13c1","5fc2382242712899"],"nodes":[{"_":"Choose","after_then":false,"chooser":"you","count":1,"distinct_from":[],"min_count":null,"optional":false,"random":false,"sel":{"$":4085},"targeting":true,"var":"t1"},{"_":"Triggered","cond":{"$":13},"gate":"none","gate_filters":[],"once_per_turn":false,"steps":[{"$":4477},{"$":273}],"trigger":{"$":110},"where":"field"},{"_":"CardScript","abilities":[{"$":4478}],"card_number":"ST07-003","notes":"","source":"binding","unit_abilities":[]}],"printable_non_ascii":[],"printed_keywords":[],"programs":[{"content_id":"503c017bf59ed7e0","instrs":[4477,273],"label":"ST07-003#a0"}],"rule_mods":[],"strings":["ST07-003#a0"],"var_names":[]}"#;
