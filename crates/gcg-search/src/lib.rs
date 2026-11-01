//! The strengthened search behind the two `gcg-sim` presets `standard` and `fast`
//! (docs/port/PLAN.md §2.1, §6.4, §12.3). Phase 3 ships it as a copy of the reference search
//! whose null configuration replays the reference bit for bit.
//!
//! Workstream D2 writes the v0 copy (PLAN §10.1). The contract types (PLAN §10.3.5) live in
//! `contracts.rs` (lead-owned) and are re-exported here; the frozen function signatures
//! (`search`, `preset_config`, `make_agent` and the `Agent` impl in `agent.rs`) are in
//! docs/port/contracts.md.

mod agent;
mod contracts;
pub mod mcts;
mod presets;

pub use agent::make_agent;
pub use contracts::*;
pub use mcts::search;
pub use presets::preset_config;
