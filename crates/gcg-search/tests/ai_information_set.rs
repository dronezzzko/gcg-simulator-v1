//! T-067 (part): `tests/ai/test_information_set.py` (52 cases),
//! `test_information_set_policy.py` (26) and `test_information_set_weights.py` (50), in Rust.
//! The 25 mid-game states come from Rust greedy self-play and must have Python's digests;
//! `permute_hidden` must give Python's states and keep `information_set_key`; and a fresh
//! agent with the same seed must make the same move with the same logged statistics on every
//! permutation, each decision also equal to Python's full record (`aiport::Recorder`). The
//! invariance tests of `test_root_moves.py` (25 + 5) and `test_search_options.py` (28) are in
//! `ai_root_moves.rs` and `ai_search_options.rs`.

mod aiport;

use std::sync::Arc;

use aiport::{
    AGENT_SEEDS, Decision, MIDGAME_STATES, PERMUTATION_SEEDS, PY_SORTED, Recorder, agent, decision,
    ids, midgame, midgame_record, midgame_state, midgame_states, par_map, pending_player,
    python_record, registry,
};
use gcg_ai::ROLE_WEIGHTS;
use gcg_ai_reference::{BASELINE_STANDARD, ReferenceConfig};
use gcg_core::pyjson::dumps;
use gcg_effects::Registry;
use gcg_engine::canon::{canonical_state, from_canonical, state_digest};
use gcg_engine::observe::{information_set_key, permute_hidden, unknown_instances};
use gcg_engine::{GameState, StateSnapshot};
use serde_json::Value;

const M: &str = "test_information_set";
const POLICY: &str = "test_information_set_policy";
const WEIGHTS: &str = "test_information_set_weights";
const KINDS: [&str; 2] = ["mcts", "greedy"];

#[test]
fn every_information_set_test_is_ported() {
    let mut ported = vec![
        "test_enough_distinct_states_and_nontrivial_permutations".to_owned(),
        "test_negative_control_a_search_on_the_true_state_is_not_invariant".to_owned(),
    ];
    let mut weights = Vec::new();
    for kind in KINDS {
        for index in 0..MIDGAME_STATES {
            ported.push(format!(
                "test_decision_invariant_under_hidden_permutation[{index}-{kind}]"
            ));
            weights.push(format!(
                "test_decision_invariant_under_hidden_permutation[{index}-{kind}-roles]"
            ));
        }
    }
    aiport::assert_ported(M, &ported);
    aiport::assert_ported(WEIGHTS, &weights);
    let mut policy = ids(
        "test_decision_with_policy_options_invariant_under_hidden_permutation",
        0..MIDGAME_STATES,
    );
    policy.push("test_policy_options_change_the_search".to_owned());
    aiport::assert_ported(POLICY, &policy);
}

/// `_hidden_ids(st, observer)` (`test_information_set.py:46-47`).
fn hidden_ids(st: &GameState, observer: u8) -> Vec<u16> {
    (0..2)
        .flat_map(|owner| unknown_instances(st, observer, owner))
        .map(|u| st.card(u).def_id.0)
        .collect()
}

#[test]
fn enough_distinct_states_and_nontrivial_permutations() {
    let rec = Recorder::new(M, "test_enough_distinct_states_and_nontrivial_permutations");
    let states = midgame_states();
    assert!(states.len() >= 20);
    let mut kinds: Vec<&str> = states
        .iter()
        .filter_map(|st| st.pending().map(|d| d.hdr.kind.as_str()))
        .collect();
    kinds.sort();
    kinds.dedup();
    assert!(kinds.len() >= 2);
    let mut changed = 0;
    for (i, st) in states.iter().enumerate() {
        let p = pending_player(st);
        let want = &midgame_record(i)["permutations"];
        for (j, s) in PERMUTATION_SEEDS.into_iter().enumerate() {
            let variant = permute_hidden(st, p, s);
            assert_eq!(
                state_digest(&variant),
                want[j]["digest"].as_str().expect("a digest"),
                "state {i}: permute_hidden seed {s}"
            );
            assert!(information_set_key(&variant, p) == information_set_key(st, p));
            let differs = hidden_ids(&variant, p) != hidden_ids(st, p);
            assert_eq!(
                Value::Bool(differs),
                want[j]["changed"],
                "state {i} seed {s}"
            );
            changed += usize::from(differs);
        }
    }
    let total = states.len() * PERMUTATION_SEEDS.len();
    assert!(changed * 10 >= total * 9, "{changed} of {total}");
    rec.finish();
}

/// One invariance case: `decide(st)` and `decide(permute_hidden(st, p, s))` for the three
/// permutation seeds must be equal; returns the decision on `st`.
fn invariant(
    rec: &mut Recorder,
    st: &GameState,
    decide: impl Fn(&mut Recorder, &GameState) -> Decision,
) -> Decision {
    let p = pending_player(st);
    let expected = decide(rec, st);
    for s in PERMUTATION_SEEDS {
        let got = decide(rec, &permute_hidden(st, p, s));
        assert_eq!(got, expected, "permutation seed {s}");
    }
    expected
}

#[test]
fn decision_invariant_under_hidden_permutation() {
    let cases: Vec<(usize, &str)> = KINDS
        .into_iter()
        .flat_map(|k| (0..MIDGAME_STATES).map(move |i| (i, k)))
        .collect();
    par_map(cases.len(), |c| {
        let (index, kind) = cases[c];
        let mut rec = Recorder::new(
            M,
            &format!("test_decision_invariant_under_hidden_permutation[{index}-{kind}]"),
        );
        let seed = AGENT_SEEDS[index % AGENT_SEEDS.len()];
        invariant(&mut rec, &midgame_state(index), |rec, st| {
            decision(rec, &mut *agent(kind, seed, true, None, None), st)
        });
        rec.finish();
    });
}

#[test]
fn weights_decision_invariant_under_hidden_permutation() {
    let cases: Vec<(usize, &str)> = KINDS
        .into_iter()
        .flat_map(|k| (0..MIDGAME_STATES).map(move |i| (i, k)))
        .collect();
    par_map(cases.len(), |c| {
        let (index, kind) = cases[c];
        let mut rec = Recorder::new(
            WEIGHTS,
            &format!("test_decision_invariant_under_hidden_permutation[{index}-{kind}-roles]"),
        );
        let seed = AGENT_SEEDS[index % AGENT_SEEDS.len()];
        invariant(&mut rec, &midgame_state(index), |rec, st| {
            decision(
                rec,
                &mut *agent(kind, seed, true, None, Some(&ROLE_WEIGHTS)),
                st,
            )
        });
        rec.finish();
    });
}

// ---------------------------------------------------------------------------------------------
// The negative control (test_information_set.py:79-103)

/// A copy of `st` in which every card is known to both players (`known = 0b11`), rebuilt from
/// its canonical form with the side tables of mid-game state `index`'s Python snapshot (a
/// permutation changes no continuous effect or filter set): every determinization of it keeps
/// the true identities, so a search on it sees the true state.
fn revealed(reg: &Arc<Registry>, st: &GameState, index: usize) -> GameState {
    let text = dumps(&canonical_state(st), PY_SORTED);
    let mut state: Value = serde_json::from_str(&text).expect("canonical JSON");
    for card in state["cards"].as_array_mut().expect("cards") {
        card[9] = Value::from(0b11);
    }
    let snap = &midgame_record(index)["snapshot"];
    let snapshot = StateSnapshot {
        state,
        continuous: snap["continuous"].clone(),
        filter_sets: snap["filter_sets"].clone(),
        rules_checked: st.rules_checked(),
    };
    from_canonical(reg, &snapshot).unwrap_or_else(|e| panic!("state {index}: {e}"))
}

/// The invariance test can fail. Python's control monkeypatches `mcts.determinize` and
/// `noop.determinize` to return `st.clone()`; the Rust search has no such seam, so it searches
/// [`revealed`] copies of `st` and of `permute_hidden(st, p, 1)` instead, whose
/// determinizations keep the true identities. The same bar applies (at least half of the 25
/// states change), every decision equals Python's on the same revealed states
/// (`midgame.json`, `revealed_decisions`), and Python's own control passed with its 50
/// decisions.
#[test]
fn negative_control_a_search_on_the_true_state_is_not_invariant() {
    let python = python_record(
        M,
        "test_negative_control_a_search_on_the_true_state_is_not_invariant",
    );
    assert_eq!(python["outcome"], "passed");
    assert_eq!(python["decisions"], 2 * MIDGAME_STATES);
    let control = midgame()["revealed"].as_array().expect("the control");
    let decisions = midgame()["revealed_decisions"]
        .as_array()
        .expect("the control's decisions");
    assert_eq!(decisions.len(), 2 * MIDGAME_STATES);
    let reg = registry();
    let differs = par_map(MIDGAME_STATES, |index| {
        let mut rec = Recorder::from_values(
            format!("negative control state {index}"),
            decisions[2 * index..2 * index + 2].to_vec(),
        );
        let st = midgame_state(index);
        let p = pending_player(&st);
        let seed = AGENT_SEEDS[index % AGENT_SEEDS.len()];
        let pair = [
            revealed(&reg, &st, index),
            revealed(&reg, &permute_hidden(&st, p, PERMUTATION_SEEDS[0]), index),
        ];
        let want = &control[index];
        let mut got = Vec::new();
        for (j, variant) in pair.iter().enumerate() {
            assert_eq!(
                state_digest(variant),
                want["digests"][j].as_str().expect("a digest"),
                "state {index} variant {j}"
            );
            got.push(decision(
                &mut rec,
                &mut *agent("mcts", seed, true, None, None),
                variant,
            ));
        }
        rec.finish();
        let d = got[0] != got[1];
        assert_eq!(Value::Bool(d), want["differs"], "state {index}");
        usize::from(d)
    });
    let changed: usize = differs.into_iter().sum();
    println!("true-state search changed on {changed} of {MIDGAME_STATES} states");
    assert!(changed >= MIDGAME_STATES / 2, "{changed}");
}

// ---------------------------------------------------------------------------------------------
// tests/ai/test_information_set_policy.py

/// `POLICY_OPTIONS` (`test_information_set_policy.py:29-34`).
fn policy_options() -> ReferenceConfig {
    let cfg = ReferenceConfig {
        policy_burst_yes_score: 4.0,
        policy_rule_polarity_table: true,
        policy_zero_ap_face: true,
        ..BASELINE_STANDARD.clone()
    };
    cfg.check().expect("a valid configuration");
    cfg
}

/// `_decision(config, seed, st)` (`test_information_set_policy.py:37-41`).
fn config_decision(
    rec: &mut Recorder,
    cfg: &ReferenceConfig,
    seed: u64,
    st: &GameState,
) -> Decision {
    decision(rec, &mut *agent("mcts", seed, true, Some(cfg), None), st)
}

#[test]
fn decision_with_policy_options_invariant_under_hidden_permutation() {
    let cfg = policy_options();
    par_map(MIDGAME_STATES, |index| {
        let mut rec = Recorder::new(
            POLICY,
            &format!(
                "test_decision_with_policy_options_invariant_under_hidden_permutation[{index}]"
            ),
        );
        let seed = AGENT_SEEDS[index % AGENT_SEEDS.len()];
        invariant(&mut rec, &midgame_state(index), |rec, st| {
            config_decision(rec, &cfg, seed, st)
        });
        rec.finish();
    });
}

#[test]
fn policy_options_change_the_search() {
    let cfg = policy_options();
    let test = "test_policy_options_change_the_search";
    let changed: usize = par_map(MIDGAME_STATES, |index| {
        let mut rec = Recorder::part(POLICY, test, 2 * index, 2);
        let st = midgame_state(index);
        let seed = AGENT_SEEDS[index % AGENT_SEEDS.len()];
        let with = config_decision(&mut rec, &cfg, seed, &st);
        let without = config_decision(&mut rec, &BASELINE_STANDARD, seed, &st);
        rec.finish();
        usize::from(with != without)
    })
    .into_iter()
    .sum();
    assert_eq!(
        python_record(POLICY, test)["decisions"]
            .as_array()
            .map(Vec::len),
        Some(2 * MIDGAME_STATES)
    );
    println!("policy options changed the search on {changed} of {MIDGAME_STATES} states");
    assert!(changed >= MIDGAME_STATES / 4, "{changed}");
}
