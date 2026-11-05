//! Agent interface and the shared exact AI components of gcg-sim: move keys,
//! canonicalisation, pruning, merging, fingerprints, the no-effect check, pre-game choices,
//! playout policy, values, targeting, evaluation, playouts, and the random and greedy players
//! (docs/dev/PLAN.md §2.1, §6.1-§6.3).
//!
//! Workstreams D1 and D2 implement it (PLAN §10.1). The contract types (PLAN §6.1, §6.6,
//! §10.3.5) live in `contracts.rs` (lead-owned) and are re-exported here; the frozen function
//! signatures, with the module each one lands in (`policy`, `values`, `targeting`,
//! `evaluation`, `playout`, `greedy`, `random` for D1; `agent`, `log`, `actions`,
//! `fingerprint`, `noop`, `setup`, `scratch` for D2), are in docs/dev/contracts.md.

pub mod actions;
pub mod agent;
mod contracts;
pub mod evaluation;
mod fingerprint;
mod greedy;
pub mod log;
pub mod math;
pub mod noop;
pub mod playout;
pub mod policy;
mod random;
mod scratch;
pub mod setup;
pub mod targeting;
pub mod values;

pub use actions::{
    RestPrune, action_key, attack_can_matter, canonical, ex_used, prune_dominated, rest_prune,
};
pub use agent::{apply_unchecked, uid_argument};
pub use contracts::*;
pub use evaluation::{
    burst_density, evaluate, evaluate_with, features, logistic, logistic_with, role_features, score,
};
pub use fingerprint::merge_equivalent;
pub use greedy::make_greedy_agent;
pub use log::{card_label, describe, log_entry};
pub use math::FloatMath;
pub use noop::{SPENDING, without_effect, without_effect_with};
pub use playout::{play_to_end, play_to_end_with, play_until_turn};
pub use policy::{choose, choose_with, option_scores, pick, pick_with};
pub use random::make_random_agent;
pub use setup::{first_player_choice, first_player_choice_with, mulligan_choice, opening_score};
