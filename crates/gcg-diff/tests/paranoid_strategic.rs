//! W-110 on a `paranoid` build (`cargo test -p gcg-diff --features paranoid`): fuzz games over
//! the generated decks with strategic agents (greedy, whose look-aheads share the cached view
//! across clones, and greedy against random) pass the invariants after every action and every
//! engine self-check, and each self-check ran. The gate's 1,000 strategic games (T-056: 400
//! greedy, 400 `search:fast`, 200 `mixed:greedy,search:fast`) are the `gcg-diff fuzz --paranoid`
//! runs of `scripts/verify_all_rust.sh`.
#![cfg(feature = "paranoid")]

use std::num::NonZeroUsize;

use gcg_diff::fuzz::{self, AgentKind, AgentSpec, FuzzArgs};
use gcg_engine::paranoid::{Check, counts};

#[test]
fn strategic_fuzz_games_pass_every_paranoid_check() {
    let before = counts();
    for agent in [
        AgentSpec::Both(AgentKind::Greedy),
        AgentSpec::Mixed(AgentKind::Greedy, AgentKind::Random),
    ] {
        // One worker: the games run on this thread, whose check counts are read below.
        let args = FuzzArgs {
            games: 6,
            seed: 2026,
            workers: NonZeroUsize::MIN,
            agent: agent.clone(),
        };
        let games = fuzz::run(&args).expect("runs");
        let failed = fuzz::failure_lines(&games, 3);
        assert!(failed.is_empty(), "{agent:?}: {}", failed.join("\n"));
        assert_eq!(games.len(), 6);
    }
    let after = counts();
    for (i, check) in Check::ALL.iter().enumerate() {
        assert!(after[i] > before[i], "{} never ran", check.name());
    }
}
