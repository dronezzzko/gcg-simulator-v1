//! What makes two runs comparable (W-91) and what a replay was recorded with (W-156), read
//! from `results.json`.
//!
//! A run's **AI identity** is the agents that played: `ai.factory`, `ai.preset`,
//! `ai.dut_agent`, `ai.bench_agent` and, in results 1.2.0, `ai.search`. Its **data and code
//! version** is `versions` without `results_schema` (the report format, not the game). Two runs
//! whose identity or version differ measure different things; comparison tools refuse to pair
//! or pool them unless the user passes `--allow-mixed`, and then they still print why.

use gcg_core::pyjson::{DumpOptions, Json, dumps};
use serde_json::Value;

use super::json_from_value;

/// Compact sorted JSON with UTF-8 text, for messages.
const COMPACT: DumpOptions = DumpOptions {
    indent: None,
    sort_keys: true,
    compact: true,
    ensure_ascii: false,
};

/// The `ai` fields of the AI identity (results 1.1.0 and 1.2.0).
pub const AI_FIELDS: [&str; 5] = ["factory", "preset", "dut_agent", "bench_agent", "search"];

/// The `versions` fields that decide how a recorded game replays and what a run measures:
/// every `versions` field except `results_schema` (`reports/versions.py:55-75`).
pub const VERSION_FIELDS: [&str; 10] = [
    "package",
    "code_digest",
    "dataset_version",
    "data_source_commit",
    "data_built_at",
    "rules_version",
    "rules_effective_date",
    "banlist_effective_date",
    "deck_rules_effective_date",
    "bo3_rules_effective_date",
];

/// One differing field: its dotted name and the two values as compact JSON (`null` when
/// absent).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Difference {
    pub field: String,
    pub a: String,
    pub b: String,
}

impl std::fmt::Display for Difference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {} vs {}", self.field, self.a, self.b)
    }
}

fn shown(v: Option<&Value>) -> String {
    v.map_or_else(
        || "null".to_owned(),
        |v| dumps(&json_from_value(v), COMPACT),
    )
}

fn differences(
    a: Option<&Value>,
    b: Option<&Value>,
    prefix: &str,
    fields: &[&str],
) -> Vec<Difference> {
    fields
        .iter()
        .filter_map(|f| {
            let (x, y) = (a.and_then(|o| o.get(*f)), b.and_then(|o| o.get(*f)));
            (x != y).then(|| Difference {
                field: format!("{prefix}.{f}"),
                a: shown(x),
                b: shown(y),
            })
        })
        .collect()
}

/// The AI-identity and version differences of two `results.json` documents, AI fields first.
#[must_use]
pub fn run_differences(a: &Value, b: &Value) -> Vec<Difference> {
    let mut out = differences(a.get("ai"), b.get("ai"), "ai", &AI_FIELDS);
    out.extend(differences(
        a.get("versions"),
        b.get("versions"),
        "versions",
        &VERSION_FIELDS,
    ));
    out
}

/// The comparison guard of W-91: `Ok(notes)` when the runs share their AI identity and
/// version (no notes), or when they differ and `allow_mixed` is set (one note per
/// difference); otherwise the refusal, naming every difference and `--allow-mixed`.
///
/// # Errors
/// The refusal message.
pub fn check_comparable(
    a: &Value,
    b: &Value,
    names: [&str; 2],
    allow_mixed: bool,
) -> Result<Vec<String>, String> {
    let diffs = run_differences(a, b);
    if diffs.is_empty() {
        return Ok(Vec::new());
    }
    let lines: Vec<String> = diffs.iter().map(|d| format!("  {d}")).collect();
    if allow_mixed {
        let mut notes = vec![format!(
            "note: {} and {} differ in AI identity or version (--allow-mixed given):",
            names[0], names[1]
        )];
        notes.extend(lines);
        return Ok(notes);
    }
    Err(format!(
        "{} and {} were played by different AIs or with different code or data, so their \
         results are not comparable (W-91):\n{}\npass --allow-mixed to compare them anyway",
        names[0],
        names[1],
        lines.join("\n")
    ))
}

/// The version comparison of W-156: every [`VERSION_FIELDS`] field whose recorded value
/// (`results.json` `versions`) differs from the running build's (`current`, as
/// `gcg_data::versions` builds it), as `(field, recorded, current)`.
#[must_use]
pub fn version_differences(recorded: &Value, current: &Json) -> Vec<Difference> {
    let current = serde_json::from_str::<Value>(&dumps(current, COMPACT)).unwrap_or(Value::Null);
    VERSION_FIELDS
        .iter()
        .filter_map(|f| {
            let (x, y) = (recorded.get(*f), current.get(*f));
            (x != y).then(|| Difference {
                field: (*f).to_owned(),
                a: shown(x),
                b: shown(y),
            })
        })
        .collect()
}
