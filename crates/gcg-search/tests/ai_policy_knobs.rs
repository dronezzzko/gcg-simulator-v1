//! `tests/ai/test_policy_knobs.py` (15 cases) in Rust: the policy settings a search
//! configuration carries (`policy_*`), the rule-polarity table, and attacks that cannot deal
//! damage. Agent decisions are compared with Python's records (`aiport::Recorder`); the
//! `card_text` override of `:167` is the committed IR fragment fixture of `ST04-005`.
//!
//! Covered elsewhere: `test_rule_polarity_table_signs_granted_rules` (5 cases) and
//! `test_rule_polarity_table_covers_every_rule_kind` check the polarity of the pool's `Choose`
//! sites and of single rules; `crates/gcg-ai/tests/policy_registry.rs` compares the polarity at
//! every `Choose` instruction of the IR, with and without the table, with Python's
//! (`choice_polarity_matches_python_at_every_choose`) and the table's coverage of every rule
//! kind (`the_rule_table_covers_every_rule_kind`). The `_board(...).blocked_from_lethal` reads
//! of `test_zero_ap_attacker_does_not_count_against_the_blockers` use a private Python helper
//! whose Rust counterpart is private too; the test's score assertion is reproduced, and
//! `policy_states.rs` compares `option_scores`, which read the board, bit for bit.

mod aiport;

use aiport::positions::{assert_pending, attack_with, burst, ok, play_card, protection, select};
use aiport::{FILLER, PLAYER, Recorder, Sc, agent, apply_ok, ids, mcts, registry_with};
use gcg_ai::{BASELINE_KNOBS, PolicyKnobs, choose, option_scores, pick};
use gcg_ai_reference::{BASELINE_PRESETS, BASELINE_STANDARD, BASELINE_WEIGHTS, ReferenceConfig};
use gcg_core::rng::SplitMix64;
use gcg_engine::{Action, ActionKind, DecisionKind, GameState, ScenarioOptions, Uid, Zone};

const M: &str = "test_policy_knobs";
const SAMPLES: usize = 1000;
/// `RULE_TABLE`, `ZERO_AP_FACE` (`test_policy_knobs.py:30-31`).
const RULE_TABLE: PolicyKnobs = PolicyKnobs {
    burst_yes_score: 1.0,
    rule_polarity_table: true,
    zero_ap_face: false,
};
const ZERO_AP_FACE: PolicyKnobs = PolicyKnobs {
    burst_yes_score: 1.0,
    rule_polarity_table: false,
    zero_ap_face: true,
};
/// Python tests checked by other Rust tests (module docs).
const COVERED_ELSEWHERE: [&str; 6] = [
    "test_rule_polarity_table_signs_granted_rules[cant_receive_damage-friendly-1--1]",
    "test_rule_polarity_table_signs_granted_rules[cant_be_attacked-friendly-1--1]",
    "test_rule_polarity_table_signs_granted_rules[stays_rested_in_start_phase-enemy--1-1]",
    "test_rule_polarity_table_signs_granted_rules[cant_block-enemy--1--1]",
    "test_rule_polarity_table_signs_granted_rules[may_attack_active-friendly-1-1]",
    "test_rule_polarity_table_covers_every_rule_kind",
];

#[test]
fn every_policy_knobs_test_is_ported() {
    let mut ported: Vec<String> = [
        "test_baseline_configurations_use_the_baseline_policy",
        "test_search_settings_map_to_policy_knobs",
        "test_burst_score_sets_how_often_rollouts_accept_a_burst",
        "test_rule_polarity_table_locks_down_the_most_valuable_enemy",
        "test_rule_polarity_table_protects_the_unit_in_the_battle",
        "test_rule_polarity_table_aims_a_battle_lock_at_the_best_enemy",
        "test_zero_ap_face_attack_keeps_its_score_when_a_trick_could_make_it_matter",
        "test_zero_ap_attacker_does_not_count_against_the_blockers",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    ported.extend(ids(
        "test_rule_polarity_table_search_saves_the_attacked_unit",
        [1, 2],
    ));
    ported.extend(ids(
        "test_zero_ap_face_attack_scores_below_ending_the_main_phase",
        ["True", "False"],
    ));
    ported.extend(COVERED_ELSEWHERE.iter().map(|s| (*s).to_owned()));
    aiport::assert_ported(M, &ported);
}

/// `_replace(**changes)`: `knobs_for(dataclasses.replace(BASELINE_PRESETS["standard"], ...))`.
fn replaced(set: impl FnOnce(&mut ReferenceConfig)) -> PolicyKnobs {
    let mut cfg = BASELINE_STANDARD.clone();
    set(&mut cfg);
    cfg.check().expect("a valid configuration");
    cfg.knobs()
}

#[test]
fn baseline_configurations_use_the_baseline_policy() {
    let rec = Recorder::new(M, "test_baseline_configurations_use_the_baseline_policy");
    assert!(
        BASELINE_PRESETS
            .iter()
            .all(|(_, c)| c.knobs() == BASELINE_KNOBS)
    );
    rec.finish();
}

#[test]
fn search_settings_map_to_policy_knobs() {
    let rec = Recorder::new(M, "test_search_settings_map_to_policy_knobs");
    assert_eq!(
        replaced(|c| c.policy_rule_polarity_table = true),
        RULE_TABLE
    );
    assert_eq!(replaced(|c| c.policy_zero_ap_face = true), ZERO_AP_FACE);
    rec.finish();
}

#[test]
fn burst_score_sets_how_often_rollouts_accept_a_burst() {
    let rec = Recorder::new(M, "test_burst_score_sets_how_often_rollouts_accept_a_burst");
    let st = burst();
    let dec = st.pending().expect("a decision");
    let declines = |knobs: &PolicyKnobs| {
        let mut rng = SplitMix64::new(7);
        let scores = ok(option_scores(&st, &dec, dec.options, knobs));
        (0..SAMPLES)
            .filter(|_| {
                let i = pick(dec.options, &scores, Some(&mut rng), 0.5);
                dec.options[i].kind == ActionKind::No
            })
            .count()
    };
    let sure = replaced(|c| c.policy_burst_yes_score = 4.0);
    assert_eq!(
        sure,
        PolicyKnobs {
            burst_yes_score: 4.0,
            ..PolicyKnobs::default()
        }
    );
    assert!(declines(&BASELINE_KNOBS) > 80);
    assert!(declines(&sure) <= 2);
    rec.finish();
}

// ---------------------------------------------------------------------------------------------
// rule polarity

#[test]
fn rule_polarity_table_locks_down_the_most_valuable_enemy() {
    let rec = Recorder::new(
        M,
        "test_rule_polarity_table_locks_down_the_most_valuable_enemy",
    );
    let mut sc = Sc::new();
    sc.resources(0, 4, 0, 0);
    let moment = sc.in_hand(0, "GD04-102");
    let aile = sc.rested(1, "ST04-001");
    let gm = sc.rested(1, "ST01-005");
    let mut st = sc.start();
    play_card(&mut st, moment, None);
    assert_pending(&st, DecisionKind::Select);
    assert_eq!(ok(choose(&st, None, 0.0, &BASELINE_KNOBS)), select(gm));
    assert_eq!(ok(choose(&st, None, 0.0, &RULE_TABLE)), select(aile));
    rec.finish();
}

#[test]
fn rule_polarity_table_protects_the_unit_in_the_battle() {
    let rec = Recorder::new(
        M,
        "test_rule_polarity_table_protects_the_unit_in_the_battle",
    );
    for attacked_is_big in [true, false] {
        let (mut st, target) = protection(attacked_is_big);
        let card = st.zone(0, Zone::Hand)[0];
        play_card(&mut st, card, None);
        assert_pending(&st, DecisionKind::Select);
        assert_eq!(ok(choose(&st, None, 0.0, &RULE_TABLE)), select(target));
        let prefix_protects_it = ok(choose(&st, None, 0.0, &BASELINE_KNOBS)) == select(target);
        assert_ne!(prefix_protects_it, attacked_is_big);
    }
    rec.finish();
}

#[test]
fn rule_polarity_table_aims_a_battle_lock_at_the_best_enemy() {
    let rec = Recorder::new(
        M,
        "test_rule_polarity_table_aims_a_battle_lock_at_the_best_enemy",
    );
    let path = "../gcg-effects/ir/fragments/ST04-005.json";
    let fixture: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path))
            .expect("the fixture"),
    )
    .expect("JSON");
    assert_eq!(
        fixture["text"],
        "【Attack】Choose 1 enemy Unit. It can't block during this battle."
    );
    let reg = registry_with(path);
    let mut sc = Sc::on(&reg, ScenarioOptions::default());
    let dagger = sc.add(0, "ST04-005");
    let aile = sc.add(1, "ST04-001");
    sc.add(1, "ST01-005");
    sc.shields(1, &[FILLER]);
    let mut st = sc.start();
    attack_with(&mut st, dagger, PLAYER);
    assert_pending(&st, DecisionKind::Select);
    assert_eq!(ok(choose(&st, None, 0.0, &RULE_TABLE)), select(aile));
    assert_eq!(ok(choose(&st, None, 0.0, &BASELINE_KNOBS)), select(aile));
    rec.finish();
}

/// `_saves_the_attacked_unit(config, seed, attacked_is_big)` (`test_policy_knobs.py:186-196`).
fn saves_the_attacked_unit(
    rec: &mut Recorder,
    cfg: &ReferenceConfig,
    seed: u64,
    attacked_is_big: bool,
) -> bool {
    let (mut st, target) = protection(attacked_is_big);
    let mut agents = [
        agent("mcts", seed, false, Some(cfg), Some(BASELINE_WEIGHTS)),
        mcts(seed + 100),
    ];
    while let Some(dec) = st.pending() {
        if st.battle().is_none() {
            break;
        }
        let p = dec.hdr.player;
        let a = rec.choose(&mut *agents[usize::from(p)], &st, p);
        apply_ok(&mut st, a);
    }
    st.card(target).zone == Zone::Battle
}

#[test]
fn rule_polarity_table_search_saves_the_attacked_unit() {
    let mut table = BASELINE_STANDARD.clone();
    table.policy_rule_polarity_table = true;
    for seed in [1, 2] {
        let mut rec = Recorder::new(
            M,
            &format!("test_rule_polarity_table_search_saves_the_attacked_unit[{seed}]"),
        );
        for attacked_is_big in [true, false] {
            assert!(saves_the_attacked_unit(
                &mut rec,
                &table,
                seed,
                attacked_is_big
            ));
        }
        assert!(!saves_the_attacked_unit(
            &mut rec,
            &BASELINE_STANDARD,
            seed,
            true
        ));
        rec.finish();
    }
}

// ---------------------------------------------------------------------------------------------
// attacks that cannot deal damage

/// `_face_minus_end(st, attacker, knobs)` (`test_policy_knobs.py:211-217`).
fn face_minus_end(st: &GameState, attacker: Uid, knobs: &PolicyKnobs) -> f64 {
    let dec = st.pending().expect("a decision");
    assert_eq!(dec.hdr.kind, DecisionKind::Main);
    let scores = ok(option_scores(st, &dec, dec.options, knobs));
    let index = |a: Action| dec.options.iter().position(|o| *o == a).expect("an option");
    let face = scores[index(Action::new(
        ActionKind::Attack,
        i32::from(attacker),
        PLAYER,
        -1,
    ))];
    face - scores[index(Action::of(ActionKind::EndMain))]
}

#[test]
fn zero_ap_face_attack_scores_below_ending_the_main_phase() {
    for (label, exposed) in [("True", true), ("False", false)] {
        let rec = Recorder::new(
            M,
            &format!("test_zero_ap_face_attack_scores_below_ending_the_main_phase[{label}]"),
        );
        let mut sc = Sc::new();
        let worker = sc.add(0, "ST05-003");
        if !exposed {
            sc.shields(1, &[FILLER]);
        }
        let st = sc.start();
        assert!(face_minus_end(&st, worker, &BASELINE_KNOBS) > 2.0);
        assert!(face_minus_end(&st, worker, &ZERO_AP_FACE) < 0.0);
        rec.finish();
    }
}

#[test]
fn zero_ap_face_attack_keeps_its_score_when_a_trick_could_make_it_matter() {
    let rec = Recorder::new(
        M,
        "test_zero_ap_face_attack_keeps_its_score_when_a_trick_could_make_it_matter",
    );
    let mut sc = Sc::new();
    sc.resources(0, 3, 0, 0);
    sc.hand(0, &["ST01-014"]);
    let worker = sc.add(0, "ST05-003");
    sc.rested(1, "ST01-005");
    let st = sc.start();
    assert_eq!(
        face_minus_end(&st, worker, &ZERO_AP_FACE).to_bits(),
        face_minus_end(&st, worker, &BASELINE_KNOBS).to_bits()
    );
    rec.finish();
}

#[test]
fn zero_ap_attacker_does_not_count_against_the_blockers() {
    let rec = Recorder::new(
        M,
        "test_zero_ap_attacker_does_not_count_against_the_blockers",
    );
    let mut sc = Sc::new();
    sc.add(0, "ST05-003");
    let gm = sc.add(0, "ST01-005");
    sc.add(1, "ST01-008");
    let st = sc.start();
    let dec = st.pending().expect("a decision");
    assert_eq!(dec.hdr.kind, DecisionKind::Main);
    let face = Action::new(ActionKind::Attack, i32::from(gm), PLAYER, -1);
    let scores = ok(option_scores(&st, &dec, dec.options, &ZERO_AP_FACE));
    let i = dec
        .options
        .iter()
        .position(|o| *o == face)
        .expect("an option");
    // pytest.approx(2.0): relative tolerance 1e-6.
    assert!((scores[i] - 2.0).abs() <= 2.0e-6, "{}", scores[i]);
    rec.finish();
}
