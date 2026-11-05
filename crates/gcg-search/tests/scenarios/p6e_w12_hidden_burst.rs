//! W-12 (T-112): root pruning judged in one determinisation. The "hidden-dependent effect" of
//! the entry's verification: the searcher's only Unit, GD04-031 (an (Academy) 3/2), can attack
//! the player; Witches from Earth (GD04-108, "During this turn, reduce the next damage it
//! receives by 2") matters only when the Shield the attack destroys holds Battle of Aces
//! (GD01-111, 【Burst】"Choose 1 enemy Unit. Deal 2 damage to it."), which would destroy the
//! Unit. Half of the defender's unseen cards are Battle of Aces, so about half of the worlds
//! hold one in the top Shield; in the others the protection expires unused and leaves no trace
//! by the next turn, and the reference's one-world no-effect check prunes the Command.

use gcg_ai::{MoveKey, NoopFlags, PruneFlags, ROLE_WEIGHTS, canonical, prune_dominated};
use gcg_engine::{ActionKind, GameState};

use crate::aiport::{FILLER, Sc};
use crate::p6e_common::{keeps, reference_agent, strengthened};

const WITCHES: &str = "GD04-108";
const BATTLE_OF_ACES: &str = "GD01-111";

/// The position: player 0's main phase, the Unit active, Witches in hand with four Resources.
pub fn hidden_burst() -> GameState {
    let mut sc = Sc::new();
    sc.resources(0, 4, 0, 0);
    sc.add(0, "GD04-031");
    sc.in_hand(0, WITCHES);
    sc.filler_shields(0, 5);
    sc.deck(0, &[FILLER; 20]);
    // The defender: no Units; 20 of its 28 unseen cards are Battle of Aces.
    let pool: Vec<&str> = (0..28)
        .map(|i| if i % 7 < 5 { BATTLE_OF_ACES } else { FILLER })
        .collect();
    sc.shields(1, &pool[0..5]);
    for &n in &pool[5..8] {
        sc.hidden_in_hand(1, n);
    }
    sc.deck(1, &pool[8..]);
    sc.start()
}

fn is_witches(st: &GameState) -> impl Fn(&MoveKey) -> bool + '_ {
    let def = st
        .registry()
        .db()
        .get(WITCHES)
        .expect("Witches from Earth")
        .def_id;
    move |k: &MoveKey| k.kind == ActionKind::PlayCommand && k.a == (1 << 24) + i64::from(def.0)
}

#[test]
fn the_no_effect_check_keeps_a_move_that_matters_in_some_worlds() {
    let st = hidden_burst();
    let dec = st.pending().expect("the main phase");
    let cfg = gcg_ai_reference::STANDARD.clone();
    let moves = prune_dominated(
        &st,
        &dec,
        canonical(&st, &dec),
        PruneFlags {
            strict_attacks: true,
            rest_targets: true,
        },
    )
    .expect("prune");
    let witches = is_witches(&st);
    assert!(moves.iter().any(|(k, _)| witches(k)));
    // The builder's check: the reference's no-effect decision differs between worlds.
    let pruned_in = (0..100u64)
        .filter(|&seed| {
            gcg_ai::without_effect(
                &st,
                0,
                &dec,
                &moves,
                seed,
                &gcg_search::preset_config(gcg_search::Preset::Fast).knobs(),
                NoopFlags {
                    branch_choices: cfg.noop_branch_choices,
                    resource_aware: cfg.noop_resource_aware,
                },
                &ROLE_WEIGHTS,
            )
            .expect("without_effect")
            .iter()
            .any(&witches)
        })
        .count();
    let reference = keeps(&st, 16, &witches, reference_agent);
    let null = keeps(&st, 16, &witches, |seed| {
        gcg_search::make_agent(&strengthened(&[]), seed, false)
    });
    let k_worlds = keeps(&st, 16, &witches, |seed| {
        gcg_search::make_agent(&strengthened(&[("prune_worlds", "4")]), seed, false)
    });
    eprintln!(
        "W-12 hidden burst: the reference check prunes Witches in {pruned_in}/100 worlds; \
         kept by the reference {reference}/16, null {null}/16, prune_worlds=4 {k_worlds}/16"
    );
    // Recorded 2026-11-02: pruned in 52/100 worlds; kept by the reference 8/16, the null
    // configuration 8/16, prune_worlds=4 14/16.
    assert!(
        0 < pruned_in && pruned_in < 100,
        "the prune decision differs between worlds"
    );
    assert_eq!(pruned_in, 52, "the builder's 100-world check (pinned)");
    assert_eq!(
        reference, 8,
        "the reference keeps the Command in half the seeds (pinned)"
    );
    assert!(null < 14, "the null configuration misses the bar: {null}");
    assert!(
        k_worlds >= 14,
        "prune_worlds keeps it in at least 14 of 16 seeds: {k_worlds}"
    );
}
