//! W-135: the strengthened agent decides from its observation alone. On the 25 mid-game states
//! of `tests/ai/test_information_set.py` (greedy self-play, Python's digests) and three hidden
//! permutations each, a fresh `search-fast` agent with the same seed makes the same move with
//! the same logged statistics, and leaves the true state exactly as it was. Unlike the
//! reference's permutation test this needs no negative control: the search has no way to read
//! the true state (`src/observation.rs` compile-fail doctests, lint §9.9 item 5). The worlds the
//! search builds are the reference's (`src/observation.rs` unit tests and `null_config.rs`).

mod aiport;

use aiport::{MIDGAME_STATES, PERMUTATION_SEEDS, midgame_state, par_map, pending_player, with_cx};
use gcg_ai::{Agent, LogAlternative};
use gcg_engine::canon::state_digest;
use gcg_engine::observe::permute_hidden;
use gcg_engine::{Action, GameState};
use gcg_search::{Preset, make_agent, preset_config};

const AGENT_SEED: u64 = 17;

fn decide(st: &GameState) -> (Action, Vec<LogAlternative>) {
    let before = state_digest(st);
    let p = pending_player(st);
    let mut agent: Box<dyn Agent> = make_agent(preset_config(Preset::Fast), AGENT_SEED, true);
    let a = with_cx(|cx| agent.choose(st, p, cx)).unwrap_or_else(|e| panic!("search-fast: {e}"));
    assert_eq!(state_digest(st), before, "the true state is untouched");
    let entry = agent.decision_log().last().expect("a logging agent");
    (a, entry.alternatives.clone())
}

#[test]
fn the_strengthened_decision_is_invariant_under_hidden_permutation() {
    let searched = par_map(MIDGAME_STATES, |index| {
        let st = midgame_state(index);
        let p = pending_player(&st);
        let expected = decide(&st);
        for s in PERMUTATION_SEEDS {
            let permuted = permute_hidden(&st, p, s);
            assert_ne!(state_digest(&permuted), state_digest(&st));
            assert_eq!(
                decide(&permuted),
                expected,
                "state {index}, permutation seed {s}"
            );
        }
        expected.1.len() > 1
    });
    // Most of these decisions are real searches with several root moves.
    let n = searched.iter().filter(|&&s| s).count();
    assert!(n * 2 >= MIDGAME_STATES, "{n} of {MIDGAME_STATES}");
}
