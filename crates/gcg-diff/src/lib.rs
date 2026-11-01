//! Differential runner and gate tool of the gcg-sim port (never shipped, A31): trace reader,
//! replay of game, test and search traces with readable diffs, fuzzing, the paranoid games,
//! corpus, coverage, traceability, `tests.json`, schema and lint checks
//! (docs/port/PLAN.md §2.1, §8.7, §9.9-§9.10, §10.3.7).
//!
//! Workstream G implements it (PLAN §10.1). The trace record types of format version 1 (PLAN
//! §8.2) and the runner's interface types live in `contracts.rs` (lead-owned) and are
//! re-exported here; the frozen function signatures are in docs/port/contracts.md. The
//! `gcg-diff` binary target lands with workstream G.

mod contracts;

pub use contracts::*;
