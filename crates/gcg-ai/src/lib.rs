//! Agent interface and the shared exact AI components of the gcg-sim port: move keys,
//! canonicalisation, pruning, merging, fingerprints, the no-effect check, pre-game choices,
//! playout policy, values, targeting, evaluation, playouts, and the random and greedy players
//! (docs/port/PLAN.md §2.1, §6.1-§6.3).
//!
//! Workstreams D1 and D2 implement it (PLAN §10.1). The contract types (PLAN §6.1, §6.6,
//! §10.3.5) live in `contracts.rs` (lead-owned) and are re-exported here; the frozen function
//! signatures, with the module each one lands in (`policy`, `values`, `targeting`,
//! `evaluation`, `playout`, `greedy`, `random` for D1; `agent`, `log`, `actions`,
//! `fingerprint`, `noop`, `setup`, `scratch` for D2), are in docs/port/contracts.md.

pub mod actions;
pub mod agent;
mod contracts;
pub mod evaluation;
mod fingerprint;
mod greedy;
pub mod log;
pub mod noop;
pub mod playout;
pub mod policy;
mod random;
mod scratch;
pub mod setup;
pub mod targeting;
pub mod values;

pub use actions::{action_key, attack_can_matter, canonical, ex_used, prune_dominated};
pub use agent::{apply_unchecked, uid_argument};
pub use contracts::*;
pub use evaluation::{burst_density, evaluate, features, logistic, role_features, score};
pub use fingerprint::merge_equivalent;
pub use greedy::make_greedy_agent;
pub use log::{card_label, describe, log_entry};
pub use noop::{SPENDING, without_effect};
pub use playout::{play_to_end, play_until_turn};
pub use policy::{choose, option_scores, pick};
pub use random::make_random_agent;
pub use setup::{first_player_choice, mulligan_choice, opening_score};
