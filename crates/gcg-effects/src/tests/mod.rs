//! The B1 gate of docs/port/PLAN.md §3.4 (tests 1-5), the loader's error paths and the IR
//! fragments, run against the committed IR. They are unit tests of the crate because they
//! read the decoded tables directly: a `Registry` needs a `&'static CardDb`, which
//! workstream A provides (test 6, `db_fingerprints`, runs with it).

mod content_ids;
mod fragments;
mod golden;
mod ir_current;
mod loader;
mod registry_counts;
mod repr_vectors;

use std::path::PathBuf;
use std::sync::OnceLock;

use serde_json::Value;

use crate::ir::Tables;

/// The IR checked into the repository.
pub(super) const EMBEDDED_IR: &str = include_str!("../../ir/registry.json");

/// The decoded embedded IR.
pub(super) fn tables() -> &'static Tables {
    static TABLES: OnceLock<Tables> = OnceLock::new();
    TABLES.get_or_init(|| Tables::decode(EMBEDDED_IR).unwrap_or_else(|e| panic!("{e}")))
}

/// The embedded IR as JSON.
pub(super) fn raw() -> &'static Value {
    static RAW: OnceLock<Value> = OnceLock::new();
    RAW.get_or_init(|| serde_json::from_str(EMBEDDED_IR).expect("the IR is JSON"))
}

/// The repository root (the crate is `crates/gcg-effects`).
pub(super) fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
