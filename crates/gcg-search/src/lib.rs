//! The strengthened search behind the two `gcg-sim` presets `standard` and `fast`
//! (docs/dev/PLAN.md §2.1, §6.4, §12.3): the reference search's core loop (`mcts.rs`) with the
//! Phase 6 extension points of `mech/`, configured by [`StrengthenedConfig`] and its
//! [`MechanismFlags`]. Its null configuration (every mechanism off) replays the reference bit
//! for bit; the search sees only an [`Observation`] (W-135).
//!
//! The contract types (PLAN §10.3.5) live in `contracts.rs` (lead-owned) and are re-exported
//! here; the frozen function signatures (`search`, `preset_config`, `make_agent` and the
//! `Agent` impl in `agent.rs`) are in docs/dev/contracts.md.

mod agent;
mod contracts;
mod flags;
pub mod mcts;
mod mech;
mod observation;
mod presets;
mod tree;

pub use agent::{make_agent, make_search_agent};
pub use contracts::*;
pub use flags::{FlagValue, MechanismFlags, OpponentList};
pub use mcts::{pregame, search, search_with_effort};
pub use mech::gapa1::api as gapa1;
pub use mech::p6g::api as p6g;
pub use observation::Observation;
pub use presets::{preset_config, reference_mapping, search_settings, weights_id};
