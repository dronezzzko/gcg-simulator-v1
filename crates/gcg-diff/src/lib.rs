//! Differential runner and gate tool of the gcg-sim port (never shipped, A31): trace reader,
//! replay of game, test and search traces with readable diffs, fuzzing, the paranoid games,
//! corpus, coverage, traceability, `tests.json`, schema and lint checks
//! (docs/port/PLAN.md §2.1, §8.7, §9.9-§9.10, §10.3.7).
//!
//! Workstream G implements it (PLAN §10.1). The trace record types of format version 1 (PLAN
//! §8.2) and the runner's interface types live in `contracts.rs` (lead-owned) and are
//! re-exported here; the frozen function signatures are in docs/port/contracts.md.

mod contracts;

pub mod cli;
pub mod corpus;
pub mod floathex;
pub mod json;
pub mod lints;
pub mod manifest;
pub mod mock;
pub mod options;
pub mod reader;
pub mod replay;
pub mod selftest;
pub mod testsjson;
pub mod weaknesses;

pub use contracts::*;
pub use mock::MockEngine;
pub use options::options_sha16;
pub use reader::{parse_record, read_trace};
pub use replay::replay_trace;
