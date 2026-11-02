//! The `gcg-eval` harness: `reference-benchmark` (the only route to the reference search),
//! `head-to-head`, `compare`, `preset-benchmark`, `env`, the Phase 6 tools and the opt-in user
//! tools of A30 (docs/port/PLAN.md §2.1, §2.3, §10.4 F).
//!
//! Workstream F implements it (PLAN §10.1). The contract types (seed roles, the play/draw
//! protocol, deck and pair selections, agent specs and mirrored game specs) live in
//! `contracts.rs` (lead-owned) and are re-exported here; the frozen function signatures are
//! in docs/port/contracts.md. The `gcg-eval` binary target lands with workstream F.

pub mod ablation;
pub mod agent_spec;
pub mod bench_cmd;
pub mod build_info;
pub mod calibrate;
pub mod cli;
pub mod compare;
pub mod compare_runs;
mod contracts;
pub mod decks;
pub mod env;
pub mod evidence;
pub mod factories;
pub mod fidelity;
pub mod gates;
pub mod head_to_head;
pub mod ledger;
pub mod overlap;
pub mod pyconv;
pub mod regret;
pub mod rows;
pub mod search_config;
pub mod selfplay;
pub mod specs;
pub mod stats;
pub mod trace;
pub mod tune;

pub use contracts::*;
