//! Benchmark reports (`reports/*.py`): `results.json` (validated against the versioned schema
//! by the in-house validator, PLAN §9.5), `summary.md`, `games.ndjson`, `replays/*.json` and
//! `timing.json`, written atomically with `results.json` last.
//!
//! Byte compatibility with Python comes from `gcg_core::pyjson` (sorted keys, CPython float
//! `repr`, per-file `ensure_ascii`) and `gcg_core::pyfloat` (`round`, fixed-precision
//! formatting); docs/port/inventory/02-reports.md lists every rule.
//!
//! The values gcg-data provides (card facts, `versions()`, conflict resolutions) enter
//! through [`ResultsData`] ([`ResultsData::embedded`] reads the packaged sources; tests pass
//! exported values), and replay documents through the caller's re-simulation
//! ([`Resimulation`]), so every report is a pure function of a [`crate::BenchmarkRun`] and
//! that data. [`build_results`] and [`write_reports`] are the frozen entry points over the
//! embedded data.

pub mod aggregate;
pub mod hypotheses;
pub mod pyregex;
pub mod replay_doc;
pub mod results;
pub mod schema;
pub mod stats;
pub mod summary;
pub mod writer;

pub use replay_doc::{Resimulation, replay_document_with};
pub use results::{
    CardFacts, MissingCard, ResultsData, build_results, build_results_with, replay_name,
    select_replays,
};
pub use schema::{json_from_value, validate_results};
pub use summary::render_summary;
pub use writer::{
    TimingHost, games_ndjson, reproduce_command, timing_json, to_json_text, write_atomic,
    write_reports, write_reports_with,
};
