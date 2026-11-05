//! `gcg-eval data-status [--json] [--sources] [--freshness [--as-of DATE]] [--templates]`
//! (docs/port/PLAN.md A30, §12.2 P6-K, §12.11): `gcg-sim data status` with three opt-in
//! sections. Without an option it prints what `gcg-sim data status` prints (`cli.py:263-331`,
//! [`gcg_runner::cli_support::status_text`]); `gcg-sim` itself is unchanged (A30).
//!
//! - `--sources` (W-88) lists every source of `data/SOURCES.lock.json`, the lock that
//!   `python -m gcg_sim.tools.sources --write` builds from `data/sources.d/*.json`, with its
//!   SHA-256 and whether its content is packaged, text-only or hash-only. The lock is embedded
//!   (it is already a file of `build-support/digest-manifest.txt`), so the listing does not
//!   depend on the current directory. The classes follow the P6-K tripwire
//!   (`crates/gcg-data/tests/p6k_data_tripwires.rs::provenance_listing`): a source is packaged
//!   when its `local_path` names a file of the repository, text-only when its notes name a
//!   stored text extraction with its SHA-256, and hash-only otherwise. `gapc_tools.rs` checks
//!   the embedded classes against the files.
//! - `--freshness [--as-of DATE]` (W-145) lists the products of `gcgapi/products.json` dated
//!   after the snapshot's `built_at` whose set has no card number in the data (the P6-K
//!   `freshness` listing). This tool reads no clock (CLAUDE.md "Determinism"), so without
//!   `--as-of` every later dated product is listed, released or announced; `--as-of` keeps
//!   those released on or before DATE.
//! - `--templates` (W-63, W-67) reports, per general template of `port/oracle/templates/`,
//!   the binding cards it reproduces byte for byte: the committed report of
//!   `port/oracle/templates/check.py` (`docs/port/audit/templates.json`, checked by
//!   `check.py --check`), embedded. The IR's `cards[].template` field stays unset: setting it
//!   changes `registry.json` and so the `ir_sha256` of every corpus trace header
//!   (docs/port/eval/weakness-status/GAP-C.md).
//!
//! With `--json` the document is `gcg-sim data status --json`'s, plus a `sources`,
//! `freshness` or `templates` key per option.

use std::collections::BTreeSet;
use std::io::Write;

use gcg_core::pyjson::{DumpOptions, Json, dumps};
use gcg_data::CardDb;
use gcg_data::legality::IsoDate;
use gcg_runner::argparse::{Action, Converter, Exit, Namespace, Parser, PyStr, Value};
use gcg_runner::cli_support::{data_status, status_text};
use serde_json::Value as JsonValue;

use crate::bench_cmd::CommandError;
use crate::benchmark_opts::SIM_SCHEMA;
use crate::build_info::{CODE_DIGEST, PACKAGE_VERSION};

/// The repository path of the embedded lock, as the listing names it.
pub const LOCK_PATH: &str = "data/SOURCES.lock.json";
/// The repository path of the embedded templates report.
pub const TEMPLATES_REPORT_PATH: &str = "docs/port/audit/templates.json";

const SOURCES_LOCK_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../data/SOURCES.lock.json"
));
const TEMPLATES_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/port/audit/templates.json"
));

/// `local_path` of a source the refresh workflow did not store (`tools/sources.py:68`).
const NOT_PACKAGED: &str = "not-packaged";
/// Where the refresh workflow stores the official pages, PDFs and their text extractions.
const OFFICIAL_RAW: &str = "data/official_raw/";

/// How much of a source the repository holds (W-88).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Provenance {
    /// The source itself is stored at its `local_path`.
    Packaged,
    /// Only a text extraction is stored (the Asia floor-rules PDF, over 3 MB).
    TextOnly,
    /// Only the hash (and the notes) are recorded.
    HashOnly,
}

impl Provenance {
    /// In listing order.
    pub const ALL: [Self; 3] = [Self::Packaged, Self::TextOnly, Self::HashOnly];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Packaged => "packaged",
            Self::TextOnly => "text-only",
            Self::HashOnly => "hash-only",
        }
    }
}

/// A stored text extraction named in a source's notes.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TextExtraction {
    pub path: String,
    pub sha256: String,
}

/// One source of the lock.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Source {
    pub id: String,
    pub kind: String,
    pub sha256: String,
    pub class: Provenance,
    /// The stored file (packaged), the text extraction (text-only) or the URL (hash-only).
    pub location: String,
    /// The text extraction the notes name, if any (also for packaged sources).
    pub text_extraction: Option<TextExtraction>,
}

fn str_field<'a>(v: &'a JsonValue, key: &str) -> &'a str {
    v.get(key).and_then(JsonValue::as_str).unwrap_or("")
}

/// The first `data/official_raw/*.txt` path in `notes`.
fn extraction_path(notes: &str) -> Option<String> {
    let mut rest = notes;
    while let Some(at) = rest.find(OFFICIAL_RAW) {
        let tail = &rest[at..];
        let end = tail
            .find(|c: char| c.is_whitespace() || c == '(' || c == ')' || c == ',')
            .unwrap_or(tail.len());
        let token = tail[..end].trim_end_matches('.');
        if let Some(stem) = token.strip_suffix(".txt")
            && !stem.ends_with('/')
        {
            return Some(token.to_owned());
        }
        rest = &tail[OFFICIAL_RAW.len()..];
    }
    None
}

/// The first `sha256 <64 hex digits>` in `notes`.
fn noted_sha256(notes: &str) -> Option<String> {
    let mut rest = notes;
    while let Some(at) = rest.find("sha256 ") {
        let tail = &rest[at + "sha256 ".len()..];
        let hex: String = tail.chars().take_while(char::is_ascii_hexdigit).collect();
        if hex.len() == 64 {
            return Some(hex.to_ascii_lowercase());
        }
        rest = tail;
    }
    None
}

/// The text extraction that a source's notes name: "text extraction" (any case), a
/// `data/official_raw/*.txt` path and its SHA-256.
#[must_use]
pub fn text_extraction(notes: &str) -> Option<TextExtraction> {
    if !notes.to_ascii_lowercase().contains("text extraction") {
        return None;
    }
    Some(TextExtraction {
        path: extraction_path(notes)?,
        sha256: noted_sha256(notes)?,
    })
}

/// The class of one lock entry.
#[must_use]
pub fn classify(entry: &JsonValue) -> Provenance {
    let packaged = entry
        .get("local_path")
        .and_then(JsonValue::as_str)
        .is_some_and(|p| !p.is_empty() && p != NOT_PACKAGED);
    if packaged {
        Provenance::Packaged
    } else if text_extraction(str_field(entry, "notes")).is_some() {
        Provenance::TextOnly
    } else {
        Provenance::HashOnly
    }
}

/// The sources of a lock document, in its order (`sources`, sorted by id).
///
/// # Errors
/// The text is not JSON or has no `sources` list.
pub fn sources_of(lock_json: &str) -> Result<Vec<Source>, String> {
    let doc: JsonValue =
        serde_json::from_str(lock_json).map_err(|e| format!("{LOCK_PATH}: {e}"))?;
    let entries = doc
        .get("sources")
        .and_then(JsonValue::as_array)
        .ok_or_else(|| format!("{LOCK_PATH}: no sources list"))?;
    Ok(entries
        .iter()
        .map(|e| {
            let class = classify(e);
            let text_extraction = text_extraction(str_field(e, "notes"));
            let location = match (class, &text_extraction) {
                (Provenance::Packaged, _) => str_field(e, "local_path").to_owned(),
                (Provenance::TextOnly, Some(t)) => t.path.clone(),
                _ => str_field(e, "url").to_owned(),
            };
            Source {
                id: str_field(e, "id").to_owned(),
                kind: str_field(e, "kind").to_owned(),
                sha256: str_field(e, "sha256").to_owned(),
                class,
                location,
                text_extraction,
            }
        })
        .collect())
}

/// The sources of the embedded lock.
///
/// # Panics
/// If the embedded lock does not parse (a build defect; this crate's tests parse it).
#[must_use]
pub fn packaged_sources() -> Vec<Source> {
    sources_of(SOURCES_LOCK_JSON).unwrap_or_else(|e| panic!("embedded lock: {e}"))
}

/// One product dated after the snapshot whose set has no card number in the data (W-145).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct MissingProduct {
    pub set_code: String,
    pub product_id: String,
    pub name: String,
    pub release_date: IsoDate,
    /// Card numbers of the set in the data: 0 for every listed product.
    pub card_numbers: usize,
}

/// The `YYYY-MM-DD` date of the snapshot's `built_at` (an ISO 8601 timestamp).
///
/// # Errors
/// `built_at` does not start with a date.
pub fn built_date(built_at: &str) -> Result<IsoDate, String> {
    built_at
        .get(..10)
        .and_then(IsoDate::parse)
        .ok_or_else(|| format!("snapshot built_at {built_at:?} does not start with a date"))
}

/// The products of `releases` dated after `built` (and on or before `as_of`) whose set code
/// is the set of no real card of `db` (`p6k_data_tripwires.rs::freshness`), sorted by set code
/// and date, one entry per set code and date.
#[must_use]
pub fn missing_products(
    db: &CardDb,
    releases: &gcg_data::legality::ReleaseDates,
    built: IsoDate,
    as_of: Option<IsoDate>,
) -> Vec<MissingProduct> {
    let sets: BTreeSet<&str> = db.real_cards().map(|d| &*d.set_code).collect();
    let mut out: Vec<MissingProduct> = Vec::new();
    for p in releases.products() {
        let (Some(set), Some(date)) = (p.set_code.as_deref(), p.release_date) else {
            continue;
        };
        let in_window = date > built && as_of.is_none_or(|a| date <= a);
        if !in_window || sets.contains(set) {
            continue;
        }
        if out
            .iter()
            .any(|m| m.set_code == set && m.release_date == date)
        {
            continue;
        }
        out.push(MissingProduct {
            set_code: set.to_owned(),
            product_id: p.product_id.to_string(),
            name: p.name.to_string(),
            release_date: date,
            card_numbers: db.real_cards().filter(|d| &*d.set_code == set).count(),
        });
    }
    out.sort_by(|a, b| {
        (a.set_code.as_str(), a.release_date).cmp(&(b.set_code.as_str(), b.release_date))
    });
    out
}

/// One template of the templates report.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TemplateRow {
    pub id: String,
    pub entry: String,
    pub family: String,
    pub reproduced: Vec<String>,
}

/// The templates report of `port/oracle/templates/check.py`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TemplatesReport {
    pub binding_cards: u64,
    pub templates: Vec<TemplateRow>,
    pub reproduced_total: u64,
    pub mismatches: Vec<String>,
}

fn strings(v: Option<&JsonValue>) -> Vec<String> {
    v.and_then(JsonValue::as_array)
        .map(|a| {
            a.iter()
                .filter_map(JsonValue::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// A templates report text.
///
/// # Errors
/// The text is not JSON or lacks a field read here.
pub fn templates_of(report_json: &str) -> Result<TemplatesReport, String> {
    let doc: JsonValue =
        serde_json::from_str(report_json).map_err(|e| format!("{TEMPLATES_REPORT_PATH}: {e}"))?;
    let count = |key: &str| {
        doc.get(key)
            .and_then(JsonValue::as_u64)
            .ok_or_else(|| format!("{TEMPLATES_REPORT_PATH}: no count {key}"))
    };
    let rows = doc
        .get("templates")
        .and_then(JsonValue::as_array)
        .ok_or_else(|| format!("{TEMPLATES_REPORT_PATH}: no templates list"))?;
    Ok(TemplatesReport {
        binding_cards: count("binding_cards")?,
        templates: rows
            .iter()
            .map(|t| TemplateRow {
                id: str_field(t, "id").to_owned(),
                entry: str_field(t, "entry").to_owned(),
                family: str_field(t, "family").to_owned(),
                reproduced: strings(t.get("reproduced_bindings")),
            })
            .collect(),
        reproduced_total: count("reproduced_total")?,
        mismatches: strings(doc.get("mismatches")),
    })
}

/// The embedded templates report.
///
/// # Panics
/// If the embedded report does not parse (a build defect; this crate's tests parse it).
#[must_use]
pub fn packaged_templates() -> TemplatesReport {
    templates_of(TEMPLATES_JSON).unwrap_or_else(|e| panic!("embedded templates report: {e}"))
}

/// The parsed `data-status` arguments.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct DataStatusArgs {
    pub json: bool,
    pub sources: bool,
    pub freshness: bool,
    pub as_of: Option<IsoDate>,
    pub templates: bool,
}

const DESCRIPTION: &str = "gcg-sim data status, with the provenance of every source, the \
     products missing from the card data and the bindings the general templates reproduce.";

/// The `data-status` subcommand parser.
#[must_use]
pub fn parser(prog: &str) -> Parser {
    let mut p = Parser::new(&format!("{prog} data-status"), Some(DESCRIPTION));
    p.push(Action::flag(
        "--json",
        "json",
        "print JSON instead of text".to_owned(),
    ));
    p.push(Action::flag(
        "--sources",
        "sources",
        "also list every source with its hash: packaged, text-only or hash-only (W-88)".to_owned(),
    ));
    p.push(Action::flag(
        "--freshness",
        "freshness",
        "also list the products dated after the snapshot with no card numbers in the data \
         (W-145)"
            .to_owned(),
    ));
    p.push(Action::option(
        "--as-of",
        "as_of",
        Converter::Str,
        None,
        Value::None,
        "with --freshness: only products released on or before this date (YYYY-MM-DD)".to_owned(),
    ));
    p.push(Action::flag(
        "--templates",
        "templates",
        "also report the binding cards each general template reproduces (W-63, W-67)".to_owned(),
    ));
    p
}

fn flag(ns: &Namespace, dest: &str) -> bool {
    matches!(ns.get(dest), Some(Value::Bool(true)))
}

/// The typed arguments of a parsed `data-status` namespace.
///
/// # Errors
/// A usage error (exit 2) for a date that is not `YYYY-MM-DD`, or `--as-of` without
/// `--freshness`.
pub fn args(ns: &Namespace, sub: &Parser, columns: i64) -> Result<DataStatusArgs, Exit> {
    let usage = |msg: &str| sub.error(&PyStr::from(msg), columns);
    let as_of = match ns.get("as_of") {
        Some(Value::Str(s)) => {
            let text = s
                .to_string_opt()
                .unwrap_or_else(|| String::from_utf8_lossy(&s.to_surrogateescape_bytes()).into());
            Some(IsoDate::parse(&text).ok_or_else(|| {
                usage(&format!(
                    "argument --as-of: invalid date value: {text:?} (expected YYYY-MM-DD)"
                ))
            })?)
        }
        _ => None,
    };
    let out = DataStatusArgs {
        json: flag(ns, "json"),
        sources: flag(ns, "sources"),
        freshness: flag(ns, "freshness"),
        as_of,
        templates: flag(ns, "templates"),
    };
    if out.as_of.is_some() && !out.freshness {
        return Err(usage("argument --as-of: needs --freshness"));
    }
    Ok(out)
}

fn sources_json(sources: &[Source]) -> Json {
    let mut counts = Json::object();
    for c in Provenance::ALL {
        counts.set(c.as_str(), sources.iter().filter(|s| s.class == c).count());
    }
    let mut out = Json::object();
    out.set("lock", LOCK_PATH);
    out.set("counts", counts);
    out.set(
        "entries",
        Json::Array(
            sources
                .iter()
                .map(|s| {
                    let mut e = Json::object();
                    e.set("id", s.id.as_str());
                    e.set("kind", s.kind.as_str());
                    e.set("class", s.class.as_str());
                    e.set("sha256", s.sha256.as_str());
                    e.set("location", s.location.as_str());
                    e
                })
                .collect(),
        ),
    );
    out
}

fn sources_text(sources: &[Source], lines: &mut Vec<String>) {
    let counts: Vec<String> = Provenance::ALL
        .iter()
        .map(|&c| {
            format!(
                "{} {}",
                sources.iter().filter(|s| s.class == c).count(),
                c.as_str()
            )
        })
        .collect();
    lines.push(format!(
        "sources: {} in {LOCK_PATH} ({})",
        sources.len(),
        counts.join(", ")
    ));
    for s in sources {
        lines.push(format!(
            "  {:<9} {} ({}) sha256 {} {}",
            s.class.as_str(),
            s.id,
            s.kind,
            s.sha256,
            s.location
        ));
    }
}

fn freshness_json(built: IsoDate, as_of: Option<IsoDate>, missing: &[MissingProduct]) -> Json {
    let mut out = Json::object();
    out.set("built", built.to_string());
    out.set("as_of", as_of.map(|d| d.to_string()));
    out.set(
        "missing",
        Json::Array(
            missing
                .iter()
                .map(|m| {
                    let mut e = Json::object();
                    e.set("set_code", m.set_code.as_str());
                    e.set("product_id", m.product_id.as_str());
                    e.set("name", m.name.as_str());
                    e.set("release_date", m.release_date.to_string());
                    e.set("card_numbers", m.card_numbers);
                    e
                })
                .collect(),
        ),
    );
    out
}

fn freshness_text(
    built: IsoDate,
    as_of: Option<IsoDate>,
    missing: &[MissingProduct],
    lines: &mut Vec<String>,
) {
    let window = as_of.map_or_else(
        || "dated after it (released or announced; no --as-of)".to_owned(),
        |d| format!("released after it and on or before {d}"),
    );
    lines.push(format!(
        "freshness: snapshot built {built}; products {window} with no card numbers in the data: \
         {}",
        missing.len()
    ));
    let dated = if as_of.is_some() {
        "released"
    } else {
        "release date"
    };
    for m in missing {
        lines.push(format!(
            "  {} {}: {dated} {}, {} card numbers",
            m.set_code, m.name, m.release_date, m.card_numbers
        ));
    }
}

fn templates_json(r: &TemplatesReport) -> Json {
    let mut out = Json::object();
    out.set("report", TEMPLATES_REPORT_PATH);
    out.set("binding_cards", r.binding_cards);
    out.set("reproduced_total", r.reproduced_total);
    out.set(
        "remaining_bindings",
        r.binding_cards.saturating_sub(r.reproduced_total),
    );
    out.set(
        "templates",
        Json::Array(
            r.templates
                .iter()
                .map(|t| {
                    let mut e = Json::object();
                    e.set("id", t.id.as_str());
                    e.set("entry", t.entry.as_str());
                    e.set("family", t.family.as_str());
                    e.set("reproduced", t.reproduced.len());
                    e.set(
                        "cards",
                        Json::Array(
                            t.reproduced
                                .iter()
                                .map(|n| Json::from(n.as_str()))
                                .collect(),
                        ),
                    );
                    e
                })
                .collect(),
        ),
    );
    out.set(
        "mismatches",
        Json::Array(
            r.mismatches
                .iter()
                .map(|m| Json::from(m.as_str()))
                .collect(),
        ),
    );
    out
}

fn templates_text(r: &TemplatesReport, lines: &mut Vec<String>) {
    lines.push(format!(
        "templates: {} general template(s) reproduce {} of {} binding cards byte for byte; {} \
         bindings remain ({TEMPLATES_REPORT_PATH})",
        r.templates.len(),
        r.reproduced_total,
        r.binding_cards,
        r.binding_cards.saturating_sub(r.reproduced_total)
    ));
    for t in &r.templates {
        let cards = if t.reproduced.is_empty() {
            "-".to_owned()
        } else {
            t.reproduced.join(", ")
        };
        lines.push(format!(
            "  {} ({}): {}: {cards}",
            t.id,
            t.entry,
            t.reproduced.len()
        ));
        lines.push(format!("    {}", t.family));
    }
    for m in &r.mismatches {
        lines.push(format!("  mismatch: {m}"));
    }
}

/// The document and text lines of `gcg-eval data-status`.
///
/// # Errors
/// An embedded file without a field read here (a build defect).
pub fn report(args: &DataStatusArgs) -> Result<(Json, Vec<String>), String> {
    let mut doc = data_status(PACKAGE_VERSION, CODE_DIGEST, SIM_SCHEMA);
    let mut lines = status_text(&doc);
    if args.sources {
        let sources = packaged_sources();
        sources_text(&sources, &mut lines);
        doc.set("sources", sources_json(&sources));
    }
    if args.freshness {
        let built_at = match doc.get("snapshot").and_then(|s| s.get("built_at")) {
            Some(Json::Str(s)) => s.clone(),
            _ => return Err("the snapshot has no built_at".to_owned()),
        };
        let built = built_date(&built_at)?;
        let missing = missing_products(
            CardDb::embedded(),
            crate::validate_cmd::packaged_releases(),
            built,
            args.as_of,
        );
        freshness_text(built, args.as_of, &missing, &mut lines);
        doc.set("freshness", freshness_json(built, args.as_of, &missing));
    }
    if args.templates {
        let r = packaged_templates();
        templates_text(&r, &mut lines);
        doc.set("templates", templates_json(&r));
    }
    Ok((doc, lines))
}

/// `gcg-eval data-status`: the JSON (`indent=2, sort_keys=True, ensure_ascii=False`, as
/// `gcg-sim data status --json`) or the text lines.
///
/// # Errors
/// What [`report`] returns.
pub fn run(args: &DataStatusArgs, out: &mut dyn Write) -> Result<(), CommandError> {
    let (doc, lines) = report(args).map_err(CommandError::Invalid)?;
    let text = if args.json {
        dumps(&doc, DumpOptions::PRETTY_SORTED_UTF8)
    } else {
        lines.join("\n")
    };
    let _ = writeln!(out, "{text}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_name_the_text_extraction_and_its_hash() {
        let asia = "PDF is 3,046,969 bytes (>= 3 MB), so it is not stored; sha256 is of the PDF. \
             local_path is the text extraction (sha256 41c9f9166485f83446945a003547833c657c2c5b93de495b4ce944505e16e98d). \
             The PDF itself is not stored (>3 MB); its text extraction is \
             data/official_raw/asia_pdf_floor-rule-forasia.txt.";
        assert_eq!(
            text_extraction(asia),
            Some(TextExtraction {
                path: "data/official_raw/asia_pdf_floor-rule-forasia.txt".to_owned(),
                sha256: "41c9f9166485f83446945a003547833c657c2c5b93de495b4ce944505e16e98d"
                    .to_owned(),
            })
        );
        assert_eq!(text_extraction("Byte-identical to the US PDF."), None);
        assert_eq!(
            text_extraction("Text extraction pending; no file yet."),
            None,
            "a path and a hash are both needed"
        );
    }

    #[test]
    fn classes_follow_local_path_then_the_notes() {
        let e = |local: JsonValue, notes: &str| serde_json::json!({"id": "x", "local_path": local, "notes": notes});
        assert_eq!(classify(&e("a/b.html".into(), "")), Provenance::Packaged);
        assert_eq!(classify(&e(NOT_PACKAGED.into(), "")), Provenance::HashOnly);
        assert_eq!(classify(&e(JsonValue::Null, "")), Provenance::HashOnly);
        let notes = format!(
            "Text extraction: data/official_raw/x.txt (sha256 {})",
            "a".repeat(64)
        );
        assert_eq!(classify(&e(JsonValue::Null, &notes)), Provenance::TextOnly);
        assert_eq!(classify(&e("p.pdf".into(), &notes)), Provenance::Packaged);
    }

    #[test]
    fn the_embedded_files_parse() {
        let sources = packaged_sources();
        assert_eq!(sources.len(), 49);
        let t = packaged_templates();
        assert!(t.mismatches.is_empty());
        assert_eq!(
            t.reproduced_total,
            u64::try_from(
                t.templates
                    .iter()
                    .flat_map(|r| r.reproduced.iter())
                    .collect::<BTreeSet<_>>()
                    .len()
            )
            .expect("small")
        );
    }

    #[test]
    fn built_at_gives_its_date() {
        assert_eq!(
            built_date("2026-10-12T14:37:38.154Z").map(|d| d.to_string()),
            Ok("2026-10-12".to_owned())
        );
        assert!(built_date("12 Oct 2026").is_err());
    }
}
