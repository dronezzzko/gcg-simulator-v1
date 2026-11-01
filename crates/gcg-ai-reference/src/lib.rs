//! Exact port of the reference ISMCTS (`ai/mcts.py`, `ai/config.py`, `ai/baseline.py`) with
//! the reference presets `fast`, `standard`, `strong`, the baseline presets and `TUNING_CONFIG`
//! (docs/port/PLAN.md §2.1, §6.2, §6.4). Reachable only through `gcg-eval` and the gate tool
//! `gcg-diff`, never through `gcg-sim` (§2.2, §6.5).
//!
//! Workstream D2 implements it (PLAN §10.1). The contract types and the preset data (PLAN
//! §6.4, §10.3.5) live in `contracts.rs` (lead-owned) and are re-exported here; the frozen
//! function signatures (`search`, `make_reference_agent` and the `Agent` impl of
//! `ReferenceAgent`, landing in `mcts.rs` and `agent.rs`) are in docs/port/contracts.md.

mod agent;
mod contracts;
pub mod mcts;

pub use agent::{AGENT_KINDS, make_agent, make_reference_agent};
pub use contracts::*;
pub use mcts::{is_target_decision, search};
