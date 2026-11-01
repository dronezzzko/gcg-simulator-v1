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

mod contracts;

pub use contracts::*;
