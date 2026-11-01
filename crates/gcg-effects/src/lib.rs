//! Effects layer of the gcg-sim port: the effect DSL node types (generated from the Python
//! dataclasses by `port/oracle/gen_dsl.py`), program instructions, the embedded JSON IR and the
//! `Registry` loaded from it, continuous-effect identity (`ContRef`), CPython `repr` and content
//! ids, the golden-format writer and the 34 custom-hook ids with typed parameters
//! (docs/port/PLAN.md §3, §5.1-§5.3, §5.6-§5.7, §10.3.2; docs/port/inventory/04-effects-ir.md).
//!
//! The contract types live in `contracts.rs` (lead-owned) and the generated `dsl.rs`; both are
//! re-exported here. Workstream B1 implements the functions frozen in docs/port/contracts.md.

mod contracts;
pub mod dsl;

pub use contracts::*;
pub use dsl::*;
