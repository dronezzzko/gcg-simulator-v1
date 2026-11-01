//! The packaged data files the Python runtime reads (Inv 03 §0), embedded byte for byte from
//! their single source of truth under `src/gcg_sim/data/` (docs/port/PLAN.md §2.4). Python
//! reads them with `read_text(encoding="utf-8")` (`cards/db.py:50-54`); `include_str!` fails
//! the build on a missing or non-UTF-8 file, so W-157's "missing file" case cannot reach run
//! time.

macro_rules! data_file {
    ($path:literal) => {
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../src/gcg_sim/data/",
            $path
        ))
    };
}

/// `gcgapi/cards.ndjson` (`cards/db.py:58`).
pub(crate) const CARDS_NDJSON: &str = data_file!("gcgapi/cards.ndjson");
/// `overrides.json` (`cards/db.py:65`, `cli.py:252`, `reports/versions.py:129`).
pub(crate) const OVERRIDES_JSON: &str = data_file!("overrides.json");
/// `curated_conflicts.json` (`cli.py:254`, `reports/versions.py:85`).
pub(crate) const CURATED_CONFLICTS_JSON: &str = data_file!("curated_conflicts.json");
/// `gcgapi/manifest.json` (`reports/versions.py:57,79`).
pub(crate) const MANIFEST_JSON: &str = data_file!("gcgapi/manifest.json");
/// `official/deck_construction.json` (`deck/official.py:76`, `reports/versions.py:69`).
pub(crate) const DECK_CONSTRUCTION_JSON: &str = data_file!("official/deck_construction.json");
/// `official/banlist.json` (`deck/official.py:114`, `reports/versions.py:68`).
pub(crate) const BANLIST_JSON: &str = data_file!("official/banlist.json");
/// `official/rules_version.json` (`reports/versions.py:58`).
pub(crate) const RULES_VERSION_JSON: &str = data_file!("official/rules_version.json");
/// `official/bo3_match_rules.json` (`reports/versions.py:72`).
pub(crate) const BO3_MATCH_RULES_JSON: &str = data_file!("official/bo3_match_rules.json");

/// Parses an embedded JSON file. The bytes are fixed at compile time and `cargo test -p
/// gcg-data` parses every file, so a failure is a build defect (docs/port/contracts.md).
pub(crate) fn json(name: &str, text: &str) -> serde_json::Value {
    serde_json::from_str(text).unwrap_or_else(|e| panic!("embedded {name} is not valid JSON: {e}"))
}
