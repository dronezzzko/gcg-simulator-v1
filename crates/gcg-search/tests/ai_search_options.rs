//! `tests/ai/test_search_options.py` (33 cases), ported: more iterations for close
//! effect-target calls (`target_extension`) change only the searches they should and only
//! after the baseline budget, and with observation-keyed nodes every setting keeps decisions
//! inside the information set (T-067: the 25 mid-game states, the two target puzzles and
//! `opponent_may_hold_a_trick`). Every search and decision is compared with Python's record
//! (`aiport::Recorder`).
//!
//! Not ported natively: the tree assertions of
//! `test_observation_keys_reach_searcher_nodes_the_same_way_after_an_opponent_pass` (pass
//! splits, the visits and nodes of opponent-pass children) and
//! `test_observation_keys_keep_main_and_action_step_plays_apart` (`mcts._add_children` child
//! counts). Both read the private tree of the reference search, which `gcg-ai-reference` does
//! not expose. The first test's two searches per seed (baseline and observation keys) are
//! compared here with Python's bit for bit, so the Rust search builds the tree whose
//! statistics Python's test inspected; the second makes no search.

mod aiport;

use aiport::positions::{
    PUZZLES, assert_pending, attack_with, kill_the_damaged_unit, opponent_may_hold_a_trick,
    target_positions,
};
use aiport::{
    AGENT_SEEDS, Decision, FILLER, MIDGAME_STATES, PERMUTATION_SEEDS, PLAYER, Recorder, Sc, agent,
    bits, decision, ids, midgame, midgame_state, par_map, pending_player,
};
use gcg_ai_reference::{BASELINE_STANDARD, BASELINE_WEIGHTS, ReferenceConfig, is_target_decision};
use gcg_engine::canon::state_digest;
use gcg_engine::observe::permute_hidden;
use gcg_engine::{DecisionKind, GameState};

const M: &str = "test_search_options";
/// `NOT_PORTED`: Python tests whose assertions need the reference's private tree.
const NOT_PORTED: [&str; 1] = ["test_observation_keys_keep_main_and_action_step_plays_apart"];

#[test]
fn every_search_options_test_is_ported() {
    let mut ported = vec![
        "test_extension_leaves_other_decisions_unchanged".to_owned(),
        "test_extension_only_adds_iterations_after_the_baseline_budget".to_owned(),
    ];
    ported.extend(ids(
        "test_observation_keys_reach_searcher_nodes_the_same_way_after_an_opponent_pass",
        [1, 2],
    ));
    ported.extend(ids(
        "test_settings_keep_decisions_inside_the_information_set",
        0..INVARIANCE_STATES,
    ));
    ported.extend(NOT_PORTED.iter().map(|s| (*s).to_owned()));
    aiport::assert_ported(M, &ported);
}

/// `EXTENDED`, `OBSERVED`, `ALL_ON` (`test_search_options.py:29-32`).
fn with(set: impl FnOnce(&mut ReferenceConfig)) -> ReferenceConfig {
    let mut cfg = BASELINE_STANDARD.clone();
    set(&mut cfg);
    cfg.check().expect("a valid configuration");
    cfg
}

fn extended() -> ReferenceConfig {
    with(|c| c.target_extension = 3.0)
}

fn observed() -> ReferenceConfig {
    with(|c| c.observation_keys = true)
}

fn all_on() -> ReferenceConfig {
    with(|c| {
        c.target_extension = 3.0;
        c.observation_keys = true;
    })
}

/// `_search(st, config, seed)` (`test_search_options.py:36-38`), through the recorder.
fn search(
    rec: &mut Recorder,
    st: &GameState,
    cfg: &ReferenceConfig,
    seed: u64,
) -> gcg_ai::SearchResult {
    rec.search(st, pending_player(st), cfg, BASELINE_WEIGHTS, seed)
}

/// `SearchResult.__eq__`: action, alternatives and iterations.
fn same(a: &gcg_ai::SearchResult, b: &gcg_ai::SearchResult) -> bool {
    a.action == b.action
        && bits(&a.alternatives) == bits(&b.alternatives)
        && a.stats.iterations == b.stats.iterations
}

// ---------------------------------------------------------------------------------------------
// target extension

#[test]
fn extension_leaves_other_decisions_unchanged() {
    let mut rec = Recorder::new(M, "test_extension_leaves_other_decisions_unchanged");
    let (ext, base) = (extended(), BASELINE_STANDARD.clone());
    let mut checked = 0;
    for index in 0..8 {
        let st = midgame_state(index);
        if is_target_decision(&st.pending().expect("a decision")) {
            continue;
        }
        let seed = AGENT_SEEDS[index % AGENT_SEEDS.len()];
        let e = search(&mut rec, &st, &ext, seed);
        let b = search(&mut rec, &st, &base, seed);
        assert!(same(&e, &b), "state {index}");
        checked += 1;
    }
    assert!(checked > 0);
    rec.finish();
}

#[test]
fn extension_only_adds_iterations_after_the_baseline_budget() {
    let mut rec = Recorder::new(
        M,
        "test_extension_only_adds_iterations_after_the_baseline_budget",
    );
    let ext = extended();
    let (mut extended_n, mut unchanged) = (0, 0);
    let mut states = target_positions();
    states.push(kill_the_damaged_unit());
    for st in &states {
        assert!(is_target_decision(&st.pending().expect("a decision")));
        for seed in 1..=8 {
            let base = search(&mut rec, st, &BASELINE_STANDARD, seed);
            let e = search(&mut rec, st, &ext, seed);
            let budget = BASELINE_STANDARD.budget(base.alternatives.len());
            if e.stats.iterations == base.stats.iterations {
                assert!(same(&e, &base));
                unchanged += 1;
            } else {
                assert!(base.stats.iterations < e.stats.iterations);
                assert!(e.stats.iterations <= 3 * budget);
                extended_n += 1;
            }
        }
    }
    assert!(extended_n > 0 && unchanged > 0, "{extended_n} {unchanged}");
    rec.finish();
}

// ---------------------------------------------------------------------------------------------
// observation keys

#[test]
fn observation_keys_after_an_opponent_pass_search_as_in_python() {
    for seed in [1, 2] {
        let mut rec = Recorder::new(
            M,
            &format!(
                "test_observation_keys_reach_searcher_nodes_the_same_way_after_an_opponent_pass[{seed}]"
            ),
        );
        let st = opponent_may_hold_a_trick();
        // `_tree(st, BASELINE, ...)` and `_tree(st, OBSERVED, ...)`: each one search, compared
        // with Python's; the pass-split counts of the trees are not ported (module docs).
        search(&mut rec, &st, &BASELINE_STANDARD, seed);
        search(&mut rec, &st, &observed(), seed);
        rec.finish();
    }
}

/// `one_play_in_two_decision_kinds()` (`test_search_options.py:158-171`): the preconditions
/// of `test_observation_keys_keep_main_and_action_step_plays_apart`, whose child counts read
/// the private tree (`NOT_PORTED`).
#[test]
fn one_play_in_two_decision_kinds_reaches_both_kinds() {
    let rec = Recorder::new(
        M,
        "test_observation_keys_keep_main_and_action_step_plays_apart",
    );
    let mut sc = Sc::new();
    sc.resources(0, 3, 0, 0);
    let gm = sc.add(0, "ST01-005");
    sc.in_hand(0, "ST01-014");
    sc.rested(1, FILLER);
    sc.filler_shields(0, 5);
    sc.filler_shields(1, 5);
    let main = sc.start();
    let mut battle = main.clone();
    attack_with(&mut battle, gm, PLAYER);
    assert_pending(&main, DecisionKind::Main);
    assert_pending(&battle, DecisionKind::ActionStep);
    assert_eq!(pending_player(&battle), 0);
    rec.finish();
}

// ---------------------------------------------------------------------------------------------
// information set (T-067)

/// `len(_invariance_states())` (`test_search_options.py:229-230`).
const INVARIANCE_STATES: usize = MIDGAME_STATES + PUZZLES.len() + 1;

/// `_invariance_states()[index]`, the extra states checked against Python's digests.
fn invariance_state(index: usize) -> GameState {
    if index < MIDGAME_STATES {
        return midgame_state(index);
    }
    let st = if index < MIDGAME_STATES + PUZZLES.len() {
        (PUZZLES[index - MIDGAME_STATES].1)().0
    } else {
        opponent_may_hold_a_trick()
    };
    assert_eq!(
        state_digest(&st),
        midgame()["extra_states"][index - MIDGAME_STATES]
            .as_str()
            .expect("a digest"),
        "invariance state {index}"
    );
    st
}

#[test]
fn settings_keep_decisions_inside_the_information_set() {
    let cfg = all_on();
    par_map(INVARIANCE_STATES, |index| {
        let mut rec = Recorder::new(
            M,
            &format!("test_settings_keep_decisions_inside_the_information_set[{index}]"),
        );
        let st = invariance_state(index);
        let p = pending_player(&st);
        let seed = AGENT_SEEDS[index % AGENT_SEEDS.len()];
        let decide = |rec: &mut Recorder, s: &GameState| -> Decision {
            let mut a = agent("mcts", seed, true, Some(&cfg), Some(BASELINE_WEIGHTS));
            decision(rec, &mut *a, s)
        };
        let expected = decide(&mut rec, &st);
        for s in PERMUTATION_SEEDS {
            assert_eq!(
                decide(&mut rec, &permute_hidden(&st, p, s)),
                expected,
                "state {index} seed {s}"
            );
        }
        rec.finish();
    });
}
