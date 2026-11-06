//! `tests/ai/test_search_side_effects.py` (2 tests) in Rust: determinized and permuted copies
//! and a whole reference MCTS decision leave the true state, and so its digest, exactly as it
//! was. The decision is compared with Python's record (`aiport::Recorder`).

mod aiport;

use aiport::{Recorder, apply_ok, new_game, pending_player};
use gcg_ai::Agent;
use gcg_ai_reference::{ReferenceAgent, ReferenceConfig};
use gcg_core::rng::SplitMix64;
use gcg_engine::GameState;
use gcg_engine::canon::state_digest;
use gcg_engine::observe::{determinize, is_hidden_from, permute_hidden};

const M: &str = "test_search_side_effects";

#[test]
fn every_search_side_effects_test_is_ported() {
    aiport::assert_ported(
        M,
        &[
            "test_search_copies_leave_the_true_history_untouched".to_owned(),
            "test_mcts_choice_leaves_the_true_state_unchanged".to_owned(),
        ],
    );
}

/// `_first_state(game_seed, wanted)` (`test_search_side_effects.py:25-32`): the first decision
/// of a random-move game that satisfies `wanted`.
fn first_state(game_seed: u64, wanted: impl Fn(&GameState) -> bool) -> GameState {
    let mut st = new_game(game_seed);
    let mut rng = SplitMix64::new(game_seed);
    while let Some(dec) = st.pending() {
        if wanted(&st) {
            break;
        }
        let a = dec.options[rng.randrange_usize(dec.options.len())];
        apply_ok(&mut st, a);
    }
    assert!(st.pending().is_some());
    st
}

/// `_hidden_history(st, observer)` (`test_search_side_effects.py:35-39`).
fn hidden_history(st: &GameState, observer: u8) -> Vec<usize> {
    st.history()
        .iter()
        .enumerate()
        .filter(|(_, h)| {
            u16::try_from(h.uid).is_ok_and(|u| h.uid >= 0 && is_hidden_from(st, u, observer))
        })
        .map(|(i, _)| i)
        .collect()
}

#[test]
fn search_copies_leave_the_true_history_untouched() {
    let rec = Recorder::new(M, "test_search_copies_leave_the_true_history_untouched");
    let st = first_state(4, |s| !hidden_history(s, 1 - s.active()).is_empty());
    let before = state_digest(&st);
    let mut rewritten = 0;
    for observer in 0..2u8 {
        for seed in 0..6u64 {
            let copies = [
                determinize(&st, observer, seed).expect("a determinization"),
                permute_hidden(&st, observer, seed),
            ];
            for copy in &copies {
                for i in hidden_history(&st, observer) {
                    let h = &copy.history()[i];
                    let uid = u16::try_from(h.uid).expect("a uid");
                    assert_eq!(h.def_id, i32::from(copy.card(uid).def_id.0));
                    rewritten += usize::from(h.def_id != st.history()[i].def_id);
                }
            }
        }
    }
    assert!(rewritten > 0);
    assert_eq!(state_digest(&st), before);
    rec.finish();
}

#[test]
fn mcts_choice_leaves_the_true_state_unchanged() {
    let mut rec = Recorder::new(M, "test_mcts_choice_leaves_the_true_state_unchanged");
    let st = first_state(4, |s| {
        s.pending().is_some_and(|d| d.options.len() > 1)
            && !hidden_history(s, pending_player(s)).is_empty()
    });
    let before = state_digest(&st);
    // SMALL_SEARCH = SearchConfig(name="small", max_iterations=16, min_iterations=16,
    // iterations_per_option=1) (test_search_side_effects.py:20-22); MctsAgent(5, config=...).
    let small = ReferenceConfig::new("small", 16, 16, 1);
    let mut agent: Box<dyn Agent> = Box::new(ReferenceAgent::new(
        small,
        5,
        false,
        gcg_ai::DEFAULT_WEIGHTS.clone(),
    ));
    rec.choose(&mut *agent, &st, pending_player(&st));
    assert_eq!(state_digest(&st), before);
    rec.finish();
}
