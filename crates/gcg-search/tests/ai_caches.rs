//! W-33: the AI's per-card and per-instruction classifications belong to the registry they are
//! computed from. Python memoises `_chosen_only_to_rest` per `(program_id, pc)`,
//! `choice_polarity` per `(program_id, pc, rule_table)` and `command_harms_enemy_units` per
//! `def_id` in process globals (`ai/actions.py:167-199`, `ai/targeting.py:54-55`, `:93-112`),
//! and `override_script` clears none of them (`testkit.py:212`). The Rust search keeps no
//! process-global AI table (contracts O10): every classification is a pure function of the
//! `&Registry` it is given, and every registry, each override included, has its own generation
//! (`Registry::generation`).
//!
//! The two ST01-014 overrides below give its Command the same program id (626) with different
//! instructions: "choose 1 enemy Unit, rest it" (resting a rested target changes nothing, so the
//! rest-target prune drops it) and "choose 1 enemy Unit, rest it; if you do, draw 1" (the target
//! is kept). A memo keyed by `(program_id, pc)` would answer the second with the first's result;
//! here each registry gets its own answer in every order, and the base registry's harm
//! classification is unchanged by anything computed on an override.

mod aiport;

use aiport::positions::{assert_pending, ok, play_card, select};
use aiport::{FILLER, Sc, registry_with};
use gcg_ai::targeting::command_harms_enemy_units;
use gcg_ai::{PruneFlags, canonical, prune_dominated};
use gcg_effects::Registry;
use gcg_engine::{DecisionKind, ScenarioOptions};

const CARD: &str = "ST01-014";
const REST_TARGETS: PruneFlags = PruneFlags {
    strict_attacks: false,
    rest_targets: true,
};
const REST_ONLY: &str = "choose_and_rest.json";
const REST_THEN_DRAW: &str = "choose_rest_if_you_do_draw.json";

/// Whether the rest-target prune keeps the rested enemy Unit as a target of ST01-014's Command
/// in the registry with `fragment` (the scenario of `test_root_moves.py`'s
/// `test_rest_targets_are_kept_when_if_you_do_follows`).
fn keeps_the_rested_target(fragment: &str) -> bool {
    let reg = registry_with(fragment);
    let mut sc = Sc::on(&reg, ScenarioOptions::default());
    sc.resources(0, 3, 0, 0);
    let card = sc.in_hand(0, CARD);
    let rested = sc.rested(1, "GD04-040");
    sc.add(1, "GD01-035");
    sc.deck(0, &[FILLER; 5]);
    let mut st = sc.start();
    play_card(&mut st, card, None);
    assert_pending(&st, DecisionKind::Select);
    let dec = st.pending().expect("a decision");
    ok(prune_dominated(
        &st,
        &dec,
        canonical(&st, &dec),
        REST_TARGETS,
    ))
    .iter()
    .any(|&(_, a)| a == select(rested))
}

#[test]
fn per_instruction_classifications_follow_each_registrys_program() {
    let program = |fragment: &str| {
        let reg = registry_with(fragment);
        let def = reg.db().get(CARD).expect("a card").def_id;
        let aid = reg.card(def).command_aid.expect("a Command");
        reg.ability(aid).program.expect("a program")
    };
    assert_eq!(program(REST_ONLY), program(REST_THEN_DRAW));
    for order in [
        [REST_ONLY, REST_THEN_DRAW, REST_ONLY],
        [REST_THEN_DRAW, REST_ONLY, REST_THEN_DRAW],
    ] {
        let got = order.map(keeps_the_rested_target);
        let want = order.map(|f| f == REST_THEN_DRAW);
        assert_eq!(got, want, "{order:?}");
    }
}

#[test]
fn every_override_is_a_new_registry_generation() {
    let base = Registry::embedded();
    let def = base.db().get(CARD).expect("a card").def_id;
    let before = command_harms_enemy_units(&base, def);
    let mut generations = vec![base.generation()];
    for fragment in [
        REST_ONLY,
        REST_THEN_DRAW,
        "remove_enemy_ex_resource.json",
        REST_ONLY,
    ] {
        let overridden = registry_with(fragment);
        assert!(
            !generations.contains(&overridden.generation()),
            "{fragment}"
        );
        generations.push(overridden.generation());
        let harms = command_harms_enemy_units(&overridden, def);
        assert_eq!(
            command_harms_enemy_units(&registry_with(fragment), def),
            harms
        );
        assert_eq!(command_harms_enemy_units(&base, def), before, "{fragment}");
    }
}
