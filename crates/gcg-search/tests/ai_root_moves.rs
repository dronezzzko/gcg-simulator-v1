//! `tests/ai/test_root_moves.py` (52 cases), ported: the root-move corrections behind the
//! search settings that are off in the baseline (`noop_branch_choices`,
//! `strict_attack_prune`, `prune_noop_rest_targets`, `noop_resource_aware`,
//! `merge_equivalent_moves`), each shown failing in the baseline configuration, the guards of
//! what they must keep, and their information-set honesty (T-067: 5 setting scenarios and the
//! 25 mid-game states). Agent decisions are compared with Python's records
//! (`aiport::Recorder`); the `card_text` and `command_script` overrides are the IR fragments
//! Python exported for them.

mod aiport;

use std::sync::Arc;

use aiport::positions::{
    age_device, amuro_rests, assert_pending, holds_removal, identical_attackers, ok, play_card,
    rests_the_blocker, select, zero_ap_scenario,
};
use aiport::{
    AGENT_SEEDS, Decision, FILLER, MIDGAME_STATES, PERMUTATION_SEEDS, PLAYER, Recorder, Sc, agent,
    decision, ids, midgame_state, par_map, pending_player, registry_with,
};
use gcg_ai::{
    BASELINE_KNOBS, DEFAULT_WEIGHTS, MoveKey, NoopFlags, PruneFlags, canonical, merge_equivalent,
    prune_dominated, without_effect,
};
use gcg_ai_reference::{BASELINE_STANDARD, BASELINE_WEIGHTS, ReferenceConfig};
use gcg_effects::Registry;
use gcg_engine::observe::{information_set_key, permute_hidden, unknown_instances};
use gcg_engine::testkit::{activate, has_action, select_if_asked};
use gcg_engine::{Action, ActionKind, AddOptions, DecisionKind, GameState, ScenarioOptions, Zone};

const M: &str = "test_root_moves";
/// `SEEDS = tuple(range(31001, 31009))`, `NOOP_SEED` (`test_root_moves.py:41-42`).
const SEEDS: [u64; 8] = [31001, 31002, 31003, 31004, 31005, 31006, 31007, 31008];
const NOOP_SEED: u64 = 31999;
const A_PLAY_COMMAND: ActionKind = ActionKind::PlayCommand;

#[test]
fn every_root_moves_test_is_ported() {
    let mut ported: Vec<String> = [
        "test_branching_keeps_a_command_whose_policy_target_is_already_rested",
        "test_lethal_rests_the_blocker_while_a_bigger_unit_is_rested",
        "test_branching_still_prunes_removal_that_should_wait",
        "test_branching_changes_nothing_when_no_option_is_better",
        "test_strict_prune_drops_a_0ap_attack_another_units_trigger_cannot_help",
        "test_strict_prune_keeps_a_0ap_attack_with_a_castable_trick",
        "test_strict_prune_keeps_a_0ap_attack_that_triggers_another_units_effect",
        "test_rest_targets_are_kept_when_a_card_in_play_reacts_to_resting",
        "test_rest_targets_are_kept_when_resting_switches_an_effect_on",
        "test_rest_targets_are_kept_when_the_effect_uses_the_card_again",
        "test_rest_targets_are_kept_when_if_you_do_follows",
        "test_rest_targets_are_kept_while_a_once_per_turn_effect_is_unused",
        "test_resource_aware_check_keeps_placing_an_ex_resource",
        "test_resource_aware_check_keeps_removing_an_enemy_ex_resource",
        "test_resource_aware_check_still_prunes_removal_that_should_wait",
        "test_two_identical_attackers_give_one_attack_move",
        "test_a_uid_specific_difference_keeps_both_attackers",
        "test_merging_keeps_both_moves_on_different_cards",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    ported.extend(ids(
        "test_strict_prune_keeps_a_0ap_attack_with_its_own_attack_trigger",
        ["own-attack-boost", "pilot-attack-trigger"],
    ));
    ported.extend(ids(
        "test_rest_effect_rests_the_active_unit",
        ["plain", "hidden-hand"],
    ));
    ported.extend(ids(
        "test_each_setting_is_invariant_under_hidden_permutation_where_it_applies",
        SETTINGS.iter().map(|(name, ..)| *name),
    ));
    ported.extend(ids(
        "test_root_settings_keep_decisions_invariant_under_hidden_permutation",
        0..MIDGAME_STATES,
    ));
    aiport::assert_ported(M, &ported);
}

// ---------------------------------------------------------------------------------------------
// Helpers (test_root_moves.py:45-71)

/// `config(**flags)`: `dataclasses.replace(BASE, **flags)`.
fn config(set: impl FnOnce(&mut ReferenceConfig)) -> ReferenceConfig {
    let mut cfg = BASELINE_STANDARD.clone();
    set(&mut cfg);
    cfg.check().expect("a valid configuration");
    cfg
}

/// `root_moves(st, **flags)`.
fn root_moves(st: &GameState, f: PruneFlags) -> Vec<Action> {
    let dec = st.pending().expect("a decision");
    ok(prune_dominated(st, &dec, canonical(st, &dec), f))
        .into_iter()
        .map(|(_, a)| a)
        .collect()
}

/// `flagged(st, **flags)`: the spending moves the no-effect check drops.
fn flagged(st: &GameState, f: NoopFlags) -> Vec<Action> {
    let dec = st.pending().expect("a decision");
    let moves = ok(prune_dominated(
        st,
        &dec,
        canonical(st, &dec),
        PruneFlags::default(),
    ));
    let idle: Vec<MoveKey> = ok(without_effect(
        st,
        dec.hdr.player,
        &dec,
        &moves,
        NOOP_SEED,
        &BASELINE_KNOBS,
        f,
        DEFAULT_WEIGHTS,
    ));
    moves
        .into_iter()
        .filter(|(k, _)| idle.contains(k))
        .map(|(_, a)| a)
        .collect()
}

fn kinds(actions: &[Action]) -> Vec<ActionKind> {
    actions.iter().map(|a| a.kind).collect()
}

const BRANCH: NoopFlags = NoopFlags {
    branch_choices: true,
    resource_aware: false,
};
const RESOURCE_AWARE: NoopFlags = NoopFlags {
    branch_choices: false,
    resource_aware: true,
};
const STRICT: PruneFlags = PruneFlags {
    strict_attacks: true,
    rest_targets: false,
};
const REST_TARGETS: PruneFlags = PruneFlags {
    strict_attacks: false,
    rest_targets: true,
};

/// `play_turn(st, cfg, seed)` (`test_root_moves.py:62-71`).
fn play_turn(rec: &mut Recorder, st: &mut GameState, cfg: &ReferenceConfig, seed: u64) {
    let mut players = [0u64, 1].map(|p| {
        agent(
            "mcts",
            seed + 100 * p,
            false,
            Some(cfg),
            Some(BASELINE_WEIGHTS),
        )
    });
    aiport::play_turns(rec, st, &mut players, 1);
}

// ---------------------------------------------------------------------------------------------
// noop_branch_choices

#[test]
fn branching_keeps_a_command_whose_policy_target_is_already_rested() {
    let rec = Recorder::new(
        M,
        "test_branching_keeps_a_command_whose_policy_target_is_already_rested",
    );
    let st = rests_the_blocker(true, 0);
    assert_eq!(kinds(&flagged(&st, NoopFlags::default())), [A_PLAY_COMMAND]);
    assert_eq!(flagged(&st, BRANCH), []);
    rec.finish();
}

#[test]
fn lethal_rests_the_blocker_while_a_bigger_unit_is_rested() {
    let mut rec = Recorder::new(
        M,
        "test_lethal_rests_the_blocker_while_a_bigger_unit_is_rested",
    );
    let mut wins = |cfg: &ReferenceConfig| {
        let mut won = 0;
        for seed in SEEDS {
            let mut st = rests_the_blocker(true, 0);
            play_turn(&mut rec, &mut st, cfg, seed);
            won += u32::from(st.winner() == Some(0));
        }
        won
    };
    assert!(wins(&config(|c| c.noop_branch_choices = true)) >= 7);
    assert!(wins(&BASELINE_STANDARD) <= 1);
    rec.finish();
}

#[test]
fn branching_still_prunes_removal_that_should_wait() {
    let rec = Recorder::new(M, "test_branching_still_prunes_removal_that_should_wait");
    let st = holds_removal();
    assert_eq!(kinds(&flagged(&st, BRANCH)), [A_PLAY_COMMAND]);
    rec.finish();
}

#[test]
fn branching_changes_nothing_when_no_option_is_better() {
    let rec = Recorder::new(M, "test_branching_changes_nothing_when_no_option_is_better");
    let st = rests_the_blocker(false, 0);
    assert_eq!(flagged(&st, NoopFlags::default()), flagged(&st, BRANCH));
    assert_eq!(flagged(&st, BRANCH), []);
    rec.finish();
}

// ---------------------------------------------------------------------------------------------
// strict_attack_prune

/// `attackers(st, strict=...)` (`test_root_moves.py:163-164`).
fn attackers(st: &GameState, strict: bool) -> Vec<i32> {
    let f = if strict {
        STRICT
    } else {
        PruneFlags::default()
    };
    let mut out: Vec<i32> = root_moves(st, f)
        .into_iter()
        .filter(|a| a.kind == ActionKind::Attack)
        .map(|a| a.a)
        .collect();
    out.sort();
    out.dedup();
    out
}

#[test]
fn strict_prune_drops_a_0ap_attack_another_units_trigger_cannot_help() {
    let rec = Recorder::new(
        M,
        "test_strict_prune_drops_a_0ap_attack_another_units_trigger_cannot_help",
    );
    let (st, worker) = zero_ap_scenario(2, 0);
    assert!(attackers(&st, false).contains(&i32::from(worker)));
    assert!(!attackers(&st, true).contains(&i32::from(worker)));
    rec.finish();
}

#[test]
fn strict_prune_keeps_a_0ap_attack_with_a_castable_trick() {
    let rec = Recorder::new(
        M,
        "test_strict_prune_keeps_a_0ap_attack_with_a_castable_trick",
    );
    let (st, worker) = zero_ap_scenario(3, 0);
    assert!(attackers(&st, true).contains(&i32::from(worker)));
    rec.finish();
}

/// The `card_text` fixture of `test_root_moves.py:188` (`crates/gcg-effects/ir/fragments/`).
fn repair_worker() -> Arc<Registry> {
    let path = "../gcg-effects/ir/fragments/ST05-003.json";
    let fixture: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path))
            .expect("the fixture"),
    )
    .expect("JSON");
    assert_eq!(fixture["text"], "<Repair 1>");
    registry_with(path)
}

#[test]
fn strict_prune_keeps_a_0ap_attack_that_triggers_another_units_effect() {
    let rec = Recorder::new(
        M,
        "test_strict_prune_keeps_a_0ap_attack_that_triggers_another_units_effect",
    );
    let scenario = |reg: &Arc<Registry>| {
        let mut sc = Sc::on(reg, ScenarioOptions::default());
        let worker = sc.add(0, "ST05-003");
        sc.add_with(
            0,
            "GD03-002",
            Zone::Battle,
            AddOptions {
                pilot: Some("ST01-010"),
                ..AddOptions::default()
            },
        );
        sc.rested(1, "ST04-005");
        sc.shields(1, &[FILLER, FILLER, FILLER]);
        (sc.start(), worker)
    };
    let (st, worker) = scenario(&repair_worker());
    assert!(attackers(&st, true).contains(&i32::from(worker)));
    let (st, worker) = scenario(&aiport::registry());
    assert!(!attackers(&st, true).contains(&i32::from(worker)));
    rec.finish();
}

#[test]
fn strict_prune_keeps_a_0ap_attack_with_its_own_attack_trigger() {
    for (id, unit, pilot) in [
        ("own-attack-boost", "EB01-013", None),
        ("pilot-attack-trigger", "ST05-003", Some("GD05-096")),
    ] {
        let rec = Recorder::new(
            M,
            &format!("test_strict_prune_keeps_a_0ap_attack_with_its_own_attack_trigger[{id}]"),
        );
        let mut sc = Sc::new();
        let attacker = sc.add_with(
            0,
            unit,
            Zone::Battle,
            AddOptions {
                pilot,
                ..AddOptions::default()
            },
        );
        sc.shields(1, &[FILLER, FILLER, FILLER]);
        let st = sc.start();
        assert!(attackers(&st, true).contains(&i32::from(attacker)), "{id}");
        rec.finish();
    }
}

// ---------------------------------------------------------------------------------------------
// prune_noop_rest_targets

#[test]
fn rest_effect_rests_the_active_unit() {
    for (id, extra, hidden) in [("plain", &[][..], 0), ("hidden-hand", &["ST01-005"][..], 4)] {
        let mut rec = Recorder::new(M, &format!("test_rest_effect_rests_the_active_unit[{id}]"));
        let (st, rested, zaku) = amuro_rests(extra, hidden, None);
        assert!(root_moves(&st, PruneFlags::default()).contains(&select(rested)));
        assert_eq!(root_moves(&st, REST_TARGETS), [select(zaku)]);
        let mut correct = |cfg: &ReferenceConfig| {
            let mut good = 0;
            for seed in SEEDS {
                let mut a = agent("mcts", seed, false, Some(cfg), Some(BASELINE_WEIGHTS));
                good += usize::from(rec.choose(&mut *a, &st.clone(), 0) == select(zaku));
            }
            good
        };
        assert_eq!(
            correct(&config(|c| c.prune_noop_rest_targets = true)),
            SEEDS.len()
        );
        assert!(correct(&BASELINE_STANDARD) < SEEDS.len());
        rec.finish();
    }
}

#[test]
fn rest_targets_are_kept_when_a_card_in_play_reacts_to_resting() {
    let rec = Recorder::new(
        M,
        "test_rest_targets_are_kept_when_a_card_in_play_reacts_to_resting",
    );
    let (st, rested, _) = amuro_rests(&[], 0, Some("GD03-128"));
    assert!(root_moves(&st, REST_TARGETS).contains(&select(rested)));
    rec.finish();
}

#[test]
fn rest_targets_are_kept_when_resting_switches_an_effect_on() {
    let rec = Recorder::new(
        M,
        "test_rest_targets_are_kept_when_resting_switches_an_effect_on",
    );
    let mut sc = Sc::new();
    sc.resources(0, 4, 0, 0);
    let zaku = sc.add(0, "GD01-035");
    sc.add(0, "GD01-013");
    let amuro = sc.in_hand(0, "ST01-010");
    let rested = sc.rested(1, "GD04-040");
    let freedom = sc.add(1, "GD03-070");
    sc.filler_shields(0, 5);
    sc.shields(1, &[FILLER]);
    let mut st = sc.start();
    play_card(&mut st, amuro, Some(zaku));
    assert_pending(&st, DecisionKind::Select);
    assert_eq!(
        root_moves(&st, REST_TARGETS),
        [select(rested), select(freedom)]
    );
    rec.finish();
}

#[test]
fn rest_targets_are_kept_when_the_effect_uses_the_card_again() {
    let rec = Recorder::new(
        M,
        "test_rest_targets_are_kept_when_the_effect_uses_the_card_again",
    );
    let mut sc = Sc::new();
    sc.resources(0, 7, 0, 0);
    let falcon = sc.in_hand(0, "ST14-005");
    let rested = sc.rested(1, "GD04-040");
    sc.add(1, "GD01-035");
    let mut st = sc.start();
    play_card(&mut st, falcon, None);
    assert_pending(&st, DecisionKind::Select);
    assert!(root_moves(&st, REST_TARGETS).contains(&select(rested)));
    rec.finish();
}

#[test]
fn rest_targets_are_kept_when_if_you_do_follows() {
    let rec = Recorder::new(M, "test_rest_targets_are_kept_when_if_you_do_follows");
    for (fragment, kept) in [
        ("choose_and_rest.json", false),
        ("choose_rest_if_you_do_draw.json", true),
    ] {
        let reg = registry_with(fragment);
        let mut sc = Sc::on(&reg, ScenarioOptions::default());
        sc.resources(0, 3, 0, 0);
        let card = sc.in_hand(0, "ST01-014");
        let rested = sc.rested(1, "GD04-040");
        sc.add(1, "GD01-035");
        sc.deck(0, &[FILLER; 5]);
        let mut st = sc.start();
        play_card(&mut st, card, None);
        assert_pending(&st, DecisionKind::Select);
        assert_eq!(
            root_moves(&st, REST_TARGETS).contains(&select(rested)),
            kept,
            "{fragment}"
        );
    }
    rec.finish();
}

#[test]
fn rest_targets_are_kept_while_a_once_per_turn_effect_is_unused() {
    let rec = Recorder::new(
        M,
        "test_rest_targets_are_kept_while_a_once_per_turn_effect_is_unused",
    );
    let mut sc = Sc::new();
    sc.resources(0, 4, 0, 0);
    sc.base(0, Some("GD03-124"));
    let host = sc.add(0, "GD01-013");
    let pilot = sc.in_hand(0, "GD01-089");
    let rested = sc.rested(1, "GD04-040");
    sc.add(1, "GD01-035");
    let mut st = sc.start();
    play_card(&mut st, pilot, Some(host));
    assert_pending(&st, DecisionKind::Select);
    assert!(root_moves(&st, REST_TARGETS).contains(&select(rested)));
    rec.finish();
}

// ---------------------------------------------------------------------------------------------
// noop_resource_aware

#[test]
fn resource_aware_check_keeps_placing_an_ex_resource() {
    let rec = Recorder::new(M, "test_resource_aware_check_keeps_placing_an_ex_resource");
    let st = age_device(0);
    assert_eq!(kinds(&flagged(&st, NoopFlags::default())), [A_PLAY_COMMAND]);
    assert_eq!(flagged(&st, RESOURCE_AWARE), []);
    rec.finish();
}

#[test]
fn resource_aware_check_keeps_removing_an_enemy_ex_resource() {
    let rec = Recorder::new(
        M,
        "test_resource_aware_check_keeps_removing_an_enemy_ex_resource",
    );
    let reg = registry_with("remove_enemy_ex_resource.json");
    let mut sc = Sc::on(&reg, ScenarioOptions::default());
    sc.resources(0, 3, 0, 0);
    sc.hand(0, &["ST01-014"]);
    sc.resources(1, 2, 0, 1);
    sc.shields(0, &[FILLER, FILLER, FILLER]);
    sc.shields(1, &[FILLER, FILLER, FILLER]);
    let st = sc.start();
    assert_eq!(kinds(&flagged(&st, NoopFlags::default())), [A_PLAY_COMMAND]);
    assert_eq!(flagged(&st, RESOURCE_AWARE), []);
    rec.finish();
}

#[test]
fn resource_aware_check_still_prunes_removal_that_should_wait() {
    let rec = Recorder::new(
        M,
        "test_resource_aware_check_still_prunes_removal_that_should_wait",
    );
    let st = holds_removal();
    assert_eq!(kinds(&flagged(&st, RESOURCE_AWARE)), [A_PLAY_COMMAND]);
    rec.finish();
}

// ---------------------------------------------------------------------------------------------
// merge_equivalent_moves

/// `merged(st)` (`test_root_moves.py:405-408`).
fn merged(st: &GameState) -> Vec<Action> {
    let dec = st.pending().expect("a decision");
    ok(merge_equivalent(st, &dec, canonical(st, &dec)))
        .into_iter()
        .map(|(_, a)| a)
        .collect()
}

/// `attacks(moves)` (`test_root_moves.py:411-412`).
fn attacks(moves: &[Action]) -> Vec<(i32, i32)> {
    moves
        .iter()
        .filter(|a| a.kind == ActionKind::Attack)
        .map(|a| (a.a, a.b))
        .collect()
}

#[test]
fn two_identical_attackers_give_one_attack_move() {
    let rec = Recorder::new(M, "test_two_identical_attackers_give_one_attack_move");
    let (st, [gm1, gm2, dagger1, dagger2]) = identical_attackers(0);
    let dec = st.pending().expect("a decision");
    let mut every: Vec<(i32, i32)> = [gm1, gm2]
        .iter()
        .flat_map(|&gm| {
            [PLAYER, i32::from(dagger1), i32::from(dagger2)].map(|t| (i32::from(gm), t))
        })
        .collect();
    let mut got = attacks(
        &canonical(&st, &dec)
            .into_iter()
            .map(|(_, a)| a)
            .collect::<Vec<_>>(),
    );
    got.sort();
    every.sort();
    assert_eq!(got, every);
    assert_eq!(
        attacks(&merged(&st)),
        [
            (i32::from(gm1), PLAYER),
            (i32::from(gm1), i32::from(dagger1))
        ]
    );
    rec.finish();
}

#[test]
fn a_uid_specific_difference_keeps_both_attackers() {
    let rec = Recorder::new(M, "test_a_uid_specific_difference_keeps_both_attackers");
    let mut sc = Sc::new();
    let used = sc.add(0, "GD05-079");
    let fresh = sc.add(0, "GD05-079");
    let target = sc.rested(1, "ST04-005");
    sc.shields(1, &[FILLER, FILLER, FILLER]);
    let mut st = sc.start();
    ok(activate(&mut st, used, None, 0));
    ok(select_if_asked(&mut st, &[target], None));
    assert!(has_action(
        &st,
        ActionKind::Activate,
        Some(i32::from(fresh)),
        None
    ));
    assert!(!has_action(
        &st,
        ActionKind::Activate,
        Some(i32::from(used)),
        None
    ));
    let mut face: Vec<i32> = attacks(&merged(&st))
        .into_iter()
        .filter(|&(_, b)| b == PLAYER)
        .map(|(a, _)| a)
        .collect();
    face.sort();
    face.dedup();
    assert_eq!(face, [i32::from(used), i32::from(fresh)]);
    rec.finish();
}

#[test]
fn merging_keeps_both_moves_on_different_cards() {
    let rec = Recorder::new(M, "test_merging_keeps_both_moves_on_different_cards");
    let mut sc = Sc::new();
    sc.add(0, "ST01-005");
    sc.add_with(
        0,
        "ST01-005",
        Zone::Battle,
        AddOptions {
            damage: 1,
            ..AddOptions::default()
        },
    );
    sc.shields(1, &[FILLER, FILLER, FILLER]);
    let st = sc.start();
    assert_eq!(attacks(&merged(&st)).len(), 2);
    rec.finish();
}

// ---------------------------------------------------------------------------------------------
// information-set honesty (T-067)

/// `ROOT_SETTINGS` (`test_root_moves.py:470-476`).
fn root_settings() -> ReferenceConfig {
    config(|c| {
        c.noop_branch_choices = true;
        c.noop_resource_aware = true;
        c.strict_attack_prune = true;
        c.prune_noop_rest_targets = true;
        c.merge_equivalent_moves = true;
    })
}

/// `decision(st, cfg, seed)` (`test_root_moves.py:479-484`).
fn cfg_decision(rec: &mut Recorder, st: &GameState, cfg: &ReferenceConfig, seed: u64) -> Decision {
    let mut a = agent("mcts", seed, true, Some(cfg), Some(BASELINE_WEIGHTS));
    decision(rec, &mut *a, st)
}

/// The `_hidden_ids` of `test_information_set.py:46-47`.
fn hidden_ids(st: &GameState, observer: u8) -> Vec<u16> {
    (0..2)
        .flat_map(|owner| unknown_instances(st, observer, owner))
        .map(|u| st.card(u).def_id.0)
        .collect()
}

type Setting = (&'static str, fn() -> GameState, fn(&mut ReferenceConfig));

/// `SETTING_SCENARIOS` (`test_root_moves.py:487-493`) in dict order, with the flag each sets.
const SETTINGS: [Setting; 5] = [
    (
        "noop_branch_choices",
        || rests_the_blocker(true, 4),
        |c| c.noop_branch_choices = true,
    ),
    (
        "noop_resource_aware",
        || age_device(4),
        |c| c.noop_resource_aware = true,
    ),
    (
        "strict_attack_prune",
        || zero_ap_scenario(2, 4).0,
        |c| c.strict_attack_prune = true,
    ),
    (
        "prune_noop_rest_targets",
        || amuro_rests(&[], 4, None).0,
        |c| c.prune_noop_rest_targets = true,
    ),
    (
        "merge_equivalent_moves",
        || identical_attackers(4).0,
        |c| c.merge_equivalent_moves = true,
    ),
];

#[test]
fn each_setting_is_invariant_under_hidden_permutation_where_it_applies() {
    par_map(SETTINGS.len(), |i| {
        let (name, build, set) = SETTINGS[i];
        let mut rec = Recorder::new(
            M,
            &format!(
                "test_each_setting_is_invariant_under_hidden_permutation_where_it_applies[{name}]"
            ),
        );
        let st = build();
        let player = pending_player(&st);
        let cfg = config(set);
        let expected = cfg_decision(&mut rec, &st, &cfg, SEEDS[0]);
        assert_ne!(
            expected,
            cfg_decision(&mut rec, &st, &BASELINE_STANDARD, SEEDS[0]),
            "{name}"
        );
        let variants: Vec<GameState> = PERMUTATION_SEEDS
            .iter()
            .map(|&s| permute_hidden(&st, player, s))
            .collect();
        assert!(
            variants
                .iter()
                .any(|v| hidden_ids(v, player) != hidden_ids(&st, player))
        );
        for v in &variants {
            assert!(information_set_key(v, player) == information_set_key(&st, player));
            assert_eq!(
                cfg_decision(&mut rec, v, &cfg, SEEDS[0]),
                expected,
                "{name}"
            );
        }
        rec.finish();
    });
}

#[test]
fn root_settings_keep_decisions_invariant_under_hidden_permutation() {
    let cfg = root_settings();
    par_map(MIDGAME_STATES, |index| {
        let mut rec = Recorder::new(
            M,
            &format!(
                "test_root_settings_keep_decisions_invariant_under_hidden_permutation[{index}]"
            ),
        );
        let st = midgame_state(index);
        let player = pending_player(&st);
        let seed = AGENT_SEEDS[index % AGENT_SEEDS.len()];
        let expected = cfg_decision(&mut rec, &st, &cfg, seed);
        for s in PERMUTATION_SEEDS {
            let got = cfg_decision(&mut rec, &permute_hidden(&st, player, s), &cfg, seed);
            assert_eq!(got, expected, "state {index} seed {s}");
        }
        rec.finish();
    });
}
