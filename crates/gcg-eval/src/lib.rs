//! The `gcg-eval` harness: `reference-benchmark` (the only route to the reference search),
//! `head-to-head`, `compare`, `preset-benchmark`, `env`, the Phase 6 tools and the opt-in user
//! tools of A30 (docs/port/PLAN.md §2.1, §2.3, §10.4 F).
//!
//! Workstream F implements it (PLAN §10.1). The contract types (seed roles, the play/draw
//! protocol, deck and pair selections, agent specs and mirrored game specs) live in
//! `contracts.rs` (lead-owned) and are re-exported here; the frozen function signatures are
//! in docs/port/contracts.md. The `gcg-eval` binary target lands with workstream F.

mod contracts;

pub use contracts::*;
