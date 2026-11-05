//! Effects layer of gcg-sim: the effect DSL node types (generated from the Python
//! dataclasses by `oracle/gen_dsl.py`), program instructions, the embedded JSON IR and the
//! `Registry` loaded from it, continuous-effect identity (`ContRef`), CPython `repr` and content
//! ids, the golden-format writer and the 34 custom-hook ids with typed parameters
//! (docs/dev/PLAN.md §3, §5.1-§5.3, §5.6-§5.7, §10.3.2).
//!
//! The contract types live in `contracts.rs` (lead-owned) and the generated `dsl.rs`; both are
//! re-exported here. Workstream B1 implements the functions frozen in docs/dev/contracts.md.

mod codec;
mod contracts;
pub mod dsl;
mod error;
mod golden;
mod hooks;
mod ir;
mod pyrepr;
mod registry;
mod view;

#[cfg(test)]
mod tests;

pub use contracts::*;
pub use dsl::*;
