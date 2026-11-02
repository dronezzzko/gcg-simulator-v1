//! Rules engine of the gcg-sim port: `GameState` and its records, the derived view, the
//! interpreter with resumable frames, the 34 custom hooks, battle, the step machine, legal
//! actions, observation (`determinize`, `information_set_key`), invariants and canonical JSON
//! (docs/port/PLAN.md §2.1, §4, §5.3-§5.5).
//!
//! Workstreams C (layers L1, L3, L5) and B2 (L2, L4) implement it (PLAN §10.1). The contract
//! types live in `contracts.rs` and the generated `enums.rs` (both lead-owned) and are
//! re-exported here; the frozen function signatures, with the module each one lands in
//! (`state`, `zones`, `canon`, `view`, `core`, `battle`, `interp`, `game`, `observe`,
//! `invariants`, `procedures`, `testkit`), are in docs/port/contracts.md.

mod battle;
pub mod canon;
mod contracts;
mod core;
pub mod diag;
mod enums;
mod error;
mod game;
mod interp;
pub mod invariants;
pub mod observe;
#[cfg(feature = "paranoid")]
pub mod paranoid;
pub mod procedures;
mod state;
#[cfg(feature = "testkit")]
pub mod testkit;
mod types;
pub mod view;
mod zones;

pub use contracts::*;
pub use diag::CapKind;
pub use enums::*;
pub use game::{
    apply, concede, deck_construction_problems, deck_construction_problems_by_number,
    legal_actions, new_game, new_game_by_number, step,
};
