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
mod contracts;
mod records;
pub mod reports;

pub use contracts::*;
pub use records::log_entry_from_json;
