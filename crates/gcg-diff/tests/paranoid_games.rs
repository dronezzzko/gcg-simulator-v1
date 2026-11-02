//! T-054: `gcg-diff paranoid-games`, the games of `tests/rules/test_engine_caches.py:26-46`
//! on a `paranoid` build (`cargo test -p gcg-diff --features paranoid`): 16 fuzz games at seed
//! 11 and two greedy games on the first two tuning decks, with every cached view recomputed,
//! every unlistened event, printed-lethal skip and unchanged-state skip of rules management
//! checked, every view-free reference resolved again, and each check run more often than the
//! Python test requires. The games equal Python's:
//!
//! ```text
//! uv run python -c "
//! from gcg_sim.ai import make_agent; from gcg_sim.ai.selfplay import run_game
//! from gcg_sim.ai.tuning import TUNING_DECKS; from gcg_sim.tools import fuzz
//! res = fuzz.run(16, seed=11, workers=1)
//! print(res.games, res.actions, len(res.failures))
//! for seed in (1, 2):
//!     agents = (make_agent('greedy', seed=seed), make_agent('greedy', seed=seed + 10))
//!     st, n = run_game(agents, (TUNING_DECKS[0], TUNING_DECKS[1]), seed)
//!     print(st.winner, st.turn, n)"
//! 16 1336 0
//! 1 23 104
//! 0 17 89
//! ```
//!
//! Without the feature the checks are compiled out, and `paranoid::run` refuses to claim them.

#[cfg(feature = "paranoid")]
#[test]
fn the_cache_games_pass_every_check_and_equal_pythons_games() {
    let (lines, ok) = gcg_diff::paranoid::run().expect("the games run");
    assert!(ok, "{}", lines.join("\n"));
    assert_eq!(
        lines[..3],
        [
            "fuzz seed 11: 16 games, 1336 actions, 0 failed",
            "greedy seed 1: winner 1 on turn 23 after 104 decisions",
            "greedy seed 2: winner 0 on turn 17 after 89 decisions",
        ]
    );
    let checks: Vec<&String> = lines[3..].iter().collect();
    assert_eq!(checks.len(), gcg_engine::paranoid::Check::ALL.len());
    for (line, check) in checks.iter().zip(gcg_engine::paranoid::Check::ALL) {
        assert!(line.contains(check.name()), "{line}");
        assert!(!line.contains("TOO FEW"), "{line}");
    }
}

#[cfg(not(feature = "paranoid"))]
#[test]
fn without_the_paranoid_build_no_check_is_claimed() {
    let err = gcg_diff::paranoid::run().expect_err("refused");
    assert!(err.contains("needs the paranoid build"), "{err}");
}
