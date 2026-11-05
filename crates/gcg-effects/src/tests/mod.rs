//! The B1 gate of docs/dev/PLAN.md §3.4 (tests 1-6), the loader's error paths, the IR
//! fragments, the public `Registry` API and the decoding of snapshot side-table nodes, run
//! against the committed IR. They are unit tests of the crate because most of them read the
//! decoded tables directly; `registry` and `inline` go through the public API.

mod content_ids;
mod db_fingerprints;
mod fragments;
mod golden;
mod inline;
mod ir_current;
mod loader;
mod registry;
mod registry_counts;
mod repr_vectors;

use std::path::PathBuf;
use std::sync::OnceLock;

use serde_json::Value;

use crate::ir::Tables;

/// The IR checked into the repository, as `Registry::embedded` loads it.
pub(super) const EMBEDDED_IR: &str = crate::registry::EMBEDDED_IR;

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
