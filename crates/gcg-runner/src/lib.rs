//! Benchmark runner of the gcg-sim port: configuration, seeds, BO1/BO3 matches, game
//! records, every report (`results.json`, `summary.md`, `games.ndjson`, `replays/*.json`,
//! `timing.json`), statistics, the schema-subset validator, replay verification, the test
//! agents and the argparse-compatible parser that `gcg-sim` and `gcg-eval` share
//! (docs/port/PLAN.md §2.1, §7, §10.3.6).
//!
//! Workstream E implements it (PLAN §10.1). The contract types (PLAN §10.3.6) live in
//! `contracts.rs` (lead-owned) and are re-exported here; the frozen function signatures, with
//! the module each one lands in (`config`, `game`, `matches`, `run`, `records`, `instrument`,
//! `reports`, `replay`, `test_agents`, `argparse`, `clock`), are in docs/port/contracts.md.

pub mod argparse;
pub mod cli_support;
mod clock;
pub mod config;
mod contracts;
pub mod game;
mod instrument;
pub mod matches;
mod records;
pub mod replay;
pub mod reports;
pub mod run;
pub mod test_agents;

pub use config::{agent_seed, dut_seat, game_seed, own_turn, seat_decks, workers_used};
pub use contracts::*;
pub use game::{play_game, replay};
pub use matches::play_match;
pub use records::log_entry_from_json;
pub use run::run_benchmark;
