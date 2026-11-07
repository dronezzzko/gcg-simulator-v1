//! GAP-C's opt-in tools (docs/dev/PLAN.md A30; docs/dev/eval/weakness-status/GAP-C.md),
//! through the `gcg-eval` binary where the verification names a command:
//!
//! - W-88: `data-status --sources` lists every source of `data/sources.d/*.json` with its hash
//!   and flags the Asia floor-rules PDF as text-only; the embedded classes equal P6-K's
//!   file-based ones;
//! - W-145: after the 2026-11-06 refresh `data-status --freshness --as-of 2026-11-06` lists
//!   nothing, `--as-of 2027-01-01` names EVX10 (released 2026-11-27, 0 card numbers) and not
//!   GD07; a synthetic snapshot with ten GD07 cards loads, leaves only EVX10 missing on
//!   2027-01-29, and a deck of its cards validates over that database with no code change;
//! - W-63, W-67: `data-status --templates` reports the committed templates report;
//! - W-75, W-76: `benchmark --analysis` writes the validator's `scope` block into
//!   `analysis.json`, and every other report equals the run without `--analysis`;
//! - W-104: `benchmark`'s `timing.json` holds Python's keys, the topology and the
//!   deterministic decision counts (the recorded decision: no CPU-time field);
//! - W-119: `transcripts` gives one id to a game wherever its replay lives, holds no search
//!   statistics or agent names, writes the bytes of `scripts/blind_transcripts.py` (with its
//!   EX count fixed), and refuses a replay that does not reproduce.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use gcg_core::pyjson::{DumpOptions, dumps};
use gcg_data::legality::{BanlistHistory, LegalityOptions, LegalitySources, ListRequest, check};
use gcg_data::{CardCount, CardDb, CardType, Color, Deck, DeckEntry, deck_digest, official};
use gcg_eval::benchmark_opts::{SIM_SCHEMA, analysis_scope};
use gcg_eval::build_info::{CODE_DIGEST, PACKAGE_VERSION};
use gcg_eval::data_status::{
    Provenance, built_date, missing_products, packaged_sources, packaged_templates, templates_of,
};
use gcg_eval::validate_cmd::packaged_releases;
use serde_json::Value;
use sha2::{Digest, Sha256};

fn root() -> PathBuf {
    fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")).expect("repo root")
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("gapc-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn s(p: &Path) -> &str {
    p.to_str().expect("UTF-8 path")
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

fn gcg_eval(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_gcg-eval"))
        .args(args)
        .current_dir(root())
        .env_remove("COLUMNS")
        .output()
        .expect("gcg-eval runs")
}

fn ok(args: &[&str]) -> String {
    let o = gcg_eval(args);
    assert_eq!(o.status.code(), Some(0), "{args:?}: {}", text(&o.stderr));
    text(&o.stdout)
}

fn read_json(rel: &str) -> Value {
    let p = root().join(rel);
    serde_json::from_str(&fs::read_to_string(&p).expect("readable")).expect("JSON")
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn str_of<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key].as_str().unwrap_or("")
}

// ---------------------------------------------------------------------------------------------
// W-88

/// The entries of `data/sources.d/*.json`, as `tools/sources.py:load_fragments` reads them.
fn fragment_entries() -> Vec<Value> {
    let mut files: Vec<PathBuf> = fs::read_dir(root().join("data/sources.d"))
        .expect("data/sources.d")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();
    files
        .iter()
        .flat_map(|f| {
            let v: Value = serde_json::from_str(&fs::read_to_string(f).expect("fragment"))
                .expect("fragment JSON");
            v.as_array().cloned().unwrap_or_default()
        })
        .collect()
}

/// P6-K's file-based class (`crates/gcg-data/tests/p6k_data_tripwires.rs::provenance_listing`).
fn p6k_class(e: &Value, raw_txt_shas: &[String]) -> Provenance {
    let path = e["local_path"].as_str().unwrap_or("not-packaged");
    if root().join(path).is_file() {
        return Provenance::Packaged;
    }
    let notes = str_of(e, "notes");
    let text_only = notes.contains("text extraction")
        && raw_txt_shas.iter().any(|sha| notes.contains(&sha[..9]));
    if text_only {
        Provenance::TextOnly
    } else {
        Provenance::HashOnly
    }
}

#[test]
fn w88_data_status_sources_lists_every_source_and_flags_the_asia_pdf_text_only() {
    let raw_txt_shas: Vec<String> = fs::read_dir(root().join("data/official_raw"))
        .expect("data/official_raw")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "txt"))
        .map(|p| sha256_hex(&fs::read(p).expect("readable")))
        .collect();
    let fragments = fragment_entries();
    let listing = packaged_sources();
    assert_eq!(listing.len(), fragments.len(), "one row per source");
    let listed: BTreeSet<&str> = listing.iter().map(|s| s.id.as_str()).collect();
    let ids: BTreeSet<&str> = fragments.iter().map(|e| str_of(e, "id")).collect();
    assert_eq!(
        listed, ids,
        "the listing covers every entry of data/sources.d/*.json"
    );
    for e in &fragments {
        let row = listing
            .iter()
            .find(|s| s.id == str_of(e, "id"))
            .expect("listed");
        assert_eq!(row.sha256, str_of(e, "sha256"), "{}", row.id);
        assert_eq!(
            row.class,
            p6k_class(e, &raw_txt_shas),
            "{}: the embedded class equals P6-K's file-based class",
            row.id
        );
        match row.class {
            Provenance::Packaged if str_of(e, "kind") != "license" => {
                let bytes = fs::read(root().join(&row.location)).expect("packaged file");
                assert_eq!(sha256_hex(&bytes), row.sha256, "{}", row.id);
            }
            Provenance::TextOnly => {
                let t = row.text_extraction.as_ref().expect("a text extraction");
                assert_eq!(row.location, t.path);
                let bytes = fs::read(root().join(&t.path)).expect("the text extraction is stored");
                assert_eq!(sha256_hex(&bytes), t.sha256, "{}", row.id);
            }
            _ => {}
        }
    }
    let text_only: Vec<&str> = listing
        .iter()
        .filter(|s| s.class == Provenance::TextOnly)
        .map(|s| s.id.as_str())
        .collect();
    assert_eq!(text_only, ["official-asia-floor-pdf"]);

    let base = ok(&["data-status"]);
    let mut expected = Vec::new();
    gcg_runner::cli_support::data_status_command(
        false,
        PACKAGE_VERSION,
        CODE_DIGEST,
        SIM_SCHEMA,
        &mut expected,
    );
    assert_eq!(
        base,
        text(&expected),
        "no option: gcg-sim data status's text"
    );
    let out = ok(&["data-status", "--sources"]);
    assert!(out.starts_with(base.trim_end()), "the status comes first");
    assert!(
        out.contains(
            "sources: 49 in data/SOURCES.lock.json (43 packaged, 1 text-only, 5 hash-only)"
        ),
        "{out}"
    );
    for row in &listing {
        let line = format!(
            "  {:<9} {} ({}) sha256 {} {}",
            row.class.as_str(),
            row.id,
            row.kind,
            row.sha256,
            row.location
        );
        assert!(out.lines().any(|l| l == line), "{line}");
    }
    assert!(
        out.lines().any(|l| l
            .starts_with("  text-only official-asia-floor-pdf (official-pdf) sha256 7bf82d9bd32d")
            && l.ends_with(" data/official_raw/asia_pdf_floor-rule-forasia.txt"))
    );
    let doc: Value =
        serde_json::from_str(&ok(&["data-status", "--sources", "--json"])).expect("JSON");
    assert_eq!(doc["sources"]["entries"].as_array().map(Vec::len), Some(49));
    assert_eq!(doc["sources"]["counts"]["text-only"], 1);
    assert_eq!(
        doc["coverage"]["card_numbers"], 1290,
        "gcg-sim data status --json's keys stay"
    );
}

// ---------------------------------------------------------------------------------------------
// W-145

#[test]
fn w145_freshness_as_of_names_evx10_and_not_gd07() {
    let current = ok(&["data-status", "--freshness", "--as-of", "2026-11-06"]);
    assert!(
        current.contains(
            "freshness: snapshot built 2026-11-02; products released after it and on or before \
             2026-11-06 with no card numbers in the data: 0"
        ),
        "the refresh holds every set released by 2026-11-06, GD06 included: {current}"
    );
    let out = ok(&["data-status", "--freshness", "--as-of", "2027-01-01"]);
    assert!(
        out.contains(
            "freshness: snapshot built 2026-11-02; products released after it and on or before \
             2027-01-01 with no card numbers in the data: 1"
        ),
        "{out}"
    );
    assert!(
        out.lines().any(|l| l
            == "  EVX10 Official Card Sleeves EX 02[EVX10]: released 2026-11-27, 0 card numbers"),
        "{out}"
    );
    assert!(
        !out.contains("GD07"),
        "GD07 (2027-01-29) is not released on 2027-01-01"
    );
    let all = ok(&["data-status", "--freshness"]);
    assert!(
        all.lines()
            .any(|l| l == "  GD07 Blazing Fist [GD07]: release date 2027-01-29, 0 card numbers"),
        "without --as-of every later dated product is listed: {all}"
    );
    let doc: Value = serde_json::from_str(&ok(&[
        "data-status",
        "--freshness",
        "--as-of",
        "2027-01-01",
        "--json",
    ]))
    .expect("JSON");
    assert_eq!(
        doc["freshness"],
        serde_json::json!({
            "as_of": "2027-01-01",
            "built": "2026-11-02",
            "missing": [{"card_numbers": 0, "name": "Official Card Sleeves EX 02[EVX10]",
                         "product_id": "evx10", "release_date": "2026-11-27",
                         "set_code": "EVX10"}],
        })
    );
    for (args, message) in [
        (
            vec!["data-status", "--as-of", "2027-01-01"],
            "argument --as-of: needs --freshness",
        ),
        (
            vec!["data-status", "--freshness", "--as-of", "2026-11-31"],
            "argument --as-of: invalid date value",
        ),
    ] {
        let o = gcg_eval(&args);
        assert_eq!(o.status.code(), Some(2), "{args:?}");
        assert!(text(&o.stderr).contains(message), "{}", text(&o.stderr));
    }
}

/// A vanilla Unit printing for a synthetic copy of the snapshot (`p6k_data_tripwires.rs`),
/// with `ap` AP: distinct AP values keep the ten cards out of the B&R list's vanilla
/// attribute-pair rule (every two vanilla cards of one Lv., cost, AP and HP are a banned pair).
fn synthetic_unit(number: &str, ap: u32) -> Value {
    serde_json::json!({
        "product_id": number, "card_number": number, "name": format!("Synthetic {number}"),
        "set_code": number.split('-').next().unwrap_or("SY"), "rarity": "C",
        "card_type": "UNIT", "color": "Blue", "level": 2, "cost": 1,
        "ap": ap, "hp": 2, "ap_raw": ap.to_string(), "hp_raw": "2",
        "zone": "Space", "trait": "(Earth Federation)", "link": "-",
        "traits": ["Earth Federation"], "effect": "-",
    })
}

fn deck_of(name: &str, main: &[(&str, u64)], resources: &[(&str, u64)]) -> Deck {
    let counts = |cards: &[(&str, u64)]| -> Box<[CardCount]> {
        let mut v: Vec<CardCount> = cards
            .iter()
            .map(|&(n, c)| CardCount {
                card_number: n.into(),
                count: u128::from(c),
            })
            .collect();
        v.sort_by(|a, b| a.card_number.cmp(&b.card_number));
        v.into()
    };
    let entries: Vec<DeckEntry> = main
        .iter()
        .chain(resources)
        .enumerate()
        .map(|(i, &(n, c))| DeckEntry {
            count: c,
            card_number: n.into(),
            written: n.into(),
            name: None,
            line: i + 1,
        })
        .collect();
    let mut deck = Deck {
        name: name.into(),
        main: counts(main),
        resources: counts(resources),
        entries: entries.into(),
        source: None,
        digest: String::new(),
    };
    deck.digest = deck_digest(&deck);
    deck
}

#[test]
fn w145_a_synthetic_gd07_snapshot_loads_and_a_deck_of_its_cards_validates() {
    let cards = fs::read_to_string(root().join("src/gcg_sim/data/gcgapi/cards.ndjson"))
        .expect("cards.ndjson");
    let overrides =
        fs::read_to_string(root().join("src/gcg_sim/data/overrides.json")).expect("overrides");
    let numbers: Vec<String> = (1..=10).map(|i| format!("GD07-{i:03}")).collect();
    let mut ndjson = cards.clone();
    for (ap, n) in (1..).zip(&numbers) {
        ndjson.push_str(&format!("{}\n", synthetic_unit(n, ap)));
    }
    let db = CardDb::from_sources(&ndjson, &overrides).expect("the synthetic copy loads");
    assert_eq!(db.len(), CardDb::embedded().len() + 10);
    let built = built_date("2026-11-02T14:57:40.351Z").expect("a date");
    let as_of = gcg_data::legality::IsoDate::parse("2027-01-29");
    let missing = |db| {
        missing_products(db, packaged_releases(), built, as_of)
            .iter()
            .map(|m| (m.set_code.clone(), m.card_numbers))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        missing(CardDb::embedded()),
        [("EVX10".to_owned(), 0), ("GD07".to_owned(), 0)]
    );
    assert_eq!(
        missing(&db),
        [("EVX10".to_owned(), 0)],
        "with GD07 cards in the data only EVX10 (sleeves, no cards) is missing"
    );
    // Forty copies of the new cards, ten blue Units with effects of the packaged data, ten
    // Resources.
    let blue: Vec<&str> = CardDb::embedded()
        .real_cards()
        .filter(|d| {
            d.card_type == CardType::Unit && d.color == Some(Color::Blue) && !d.is_vanilla()
        })
        .map(|d| &*d.card_number)
        .take(3)
        .collect();
    assert_eq!(blue.len(), 3);
    let mut main: Vec<(&str, u64)> = numbers.iter().map(|n| (n.as_str(), 4)).collect();
    main.extend([(blue[0], 4), (blue[1], 4), (blue[2], 2)]);
    let deck = deck_of("synthetic-gd07", &main, &[("R-001", 10)]);
    let request = LegalityOptions {
        banlist: Some(ListRequest::default()),
        implemented: None,
        event_date: None,
    };
    let sources = |db| LegalitySources {
        db,
        construction: official::construction_rules(),
        lists: BanlistHistory::packaged(),
        releases: packaged_releases(),
    };
    let report = check(&deck, &sources(&db), &request).expect("the current list");
    assert_eq!(
        report.rules,
        [],
        "a deck of the new cards is legal over the refreshed data"
    );
    // Negative control: the packaged data does not know the GD07 numbers.
    let packaged = check(&deck, &sources(CardDb::embedded()), &request).expect("the current list");
    let unknown: Vec<&str> = packaged
        .rules
        .iter()
        .filter(|v| v.code == gcg_data::ViolationCode::UnknownId)
        .flat_map(|v| v.cards.iter().map(|c| &**c))
        .collect();
    assert_eq!(
        unknown,
        numbers.iter().map(String::as_str).collect::<Vec<_>>()
    );
}

// ---------------------------------------------------------------------------------------------
// W-63, W-67

#[test]
fn w63_data_status_templates_reports_the_committed_template_report() {
    let committed = fs::read_to_string(root().join("docs/dev/audit/templates.json"))
        .expect("the committed report");
    let report = templates_of(&committed).expect("a templates report");
    assert_eq!(
        packaged_templates(),
        report,
        "the embedded report is the committed one"
    );
    assert!(report.mismatches.is_empty());
    let out = ok(&["data-status", "--templates"]);
    let header = format!(
        "templates: {} general template(s) reproduce {} of {} binding cards byte for byte; {} \
         bindings remain (docs/dev/audit/templates.json)",
        report.templates.len(),
        report.reproduced_total,
        report.binding_cards,
        report.binding_cards - report.reproduced_total
    );
    assert!(out.lines().any(|l| l == header), "{out}");
    for t in &report.templates {
        let line = format!(
            "  {} ({}): {}: {}",
            t.id,
            t.entry,
            t.reproduced.len(),
            if t.reproduced.is_empty() {
                "-".to_owned()
            } else {
                t.reproduced.join(", ")
            }
        );
        assert!(out.lines().any(|l| l == line), "{line}");
    }
    let doc: Value =
        serde_json::from_str(&ok(&["data-status", "--templates", "--json"])).expect("JSON");
    assert_eq!(
        doc["templates"]["reproduced_total"],
        report.reproduced_total
    );
    assert_eq!(doc["templates"]["binding_cards"], report.binding_cards);
}

// ---------------------------------------------------------------------------------------------
// W-75, W-76, W-104

fn tree(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in fs::read_dir(&d).expect("readable") {
            let p = e.expect("entry").path();
            if p.is_dir() {
                stack.push(p);
            } else {
                let rel = p
                    .strip_prefix(dir)
                    .expect("inside")
                    .to_string_lossy()
                    .into_owned();
                out.insert(rel, fs::read(&p).expect("file"));
            }
        }
    }
    out
}

fn benchmark(out: &Path, extra: &[&str]) {
    let decks = root().join("examples/decks");
    let dut = decks.join("red-green-zeon.txt");
    let bench = decks.join("blue-white-federation.txt");
    let mut args = vec![
        "benchmark",
        s(&dut),
        s(&bench),
        "--matches",
        "2",
        "--seed",
        "5",
        "--format",
        "bo1",
        "--ai-preset",
        "fast",
        "--replays",
        "1",
        "--out",
        s(out),
    ];
    args.extend(extra);
    ok(&args);
}

fn to_value(j: &gcg_core::pyjson::Json) -> Value {
    serde_json::from_str(&dumps(j, DumpOptions::COMPACT_SORTED_ASCII)).expect("JSON")
}

#[test]
fn w75_w76_w104_analysis_scope_block_and_timing_fields() {
    let dir = scratch("analysis-scope");
    let plain = dir.join("plain");
    let analysed = dir.join("analysed");
    benchmark(&plain, &[]);
    benchmark(&analysed, &["--analysis"]);
    let mut ours = tree(&analysed);
    let analysis: Value = serde_json::from_slice(&ours["analysis.json"]).expect("JSON");
    ours.retain(|k, _| k != "analysis.json" && k != "timing.json" && !k.starts_with("variants/"));
    let mut reference = tree(&plain);
    reference.remove("timing.json");
    assert_eq!(
        ours, reference,
        "--analysis leaves every other report unchanged"
    );

    let scope = &analysis["scope"];
    assert_eq!(scope, &to_value(&analysis_scope()));
    let results: Value =
        serde_json::from_slice(&fs::read(plain.join("results.json")).expect("results"))
            .expect("JSON");
    // W-75: the format applied and the list's date.
    assert_eq!(scope["format"], "Constructed");
    assert_eq!(scope["banlist"]["applied"], true);
    assert_eq!(
        scope["banlist"]["effective_date"],
        results["versions"]["banlist_effective_date"]
    );
    let banlist = read_json("src/gcg_sim/data/official/banlist.json");
    assert_eq!(scope["banlist"]["format"], banlist["format"]);
    assert_eq!(
        scope["banlist"]["regions"],
        serde_json::json!(["asia-en", "en"])
    );
    // W-76: the unmodelled tournament procedures, unsupported formats, unchecked rules and the
    // Edition Beta facts.
    let bo3 = read_json("src/gcg_sim/data/official/bo3_match_rules.json");
    assert_eq!(
        scope["unmodelled_tournament_procedures"],
        bo3["ignored_by_simulator"]
    );
    assert_eq!(
        scope["unmodelled_tournament_procedures"]["intentional_draws_concessions_penalties"],
        "Not modelled."
    );
    let formats: Vec<&str> = scope["unsupported_formats"]
        .as_array()
        .expect("formats")
        .iter()
        .map(|f| str_of(f, "name"))
        .collect();
    assert_eq!(formats, ["Sealed", "Draft"]);
    let unchecked = scope["unchecked_tournament_rules"]
        .as_array()
        .expect("unchecked rules");
    assert!(unchecked.iter().any(|r| {
        r.as_str()
            .is_some_and(|r| r.starts_with("7-day new-card rule for Premier events"))
    }));
    assert!(
        scope["edition_beta"]["card_numbers_only_in_edition_beta"]
            .as_array()
            .expect("numbers")
            .contains(&Value::from("RP-001"))
    );

    // W-104: Python's keys, the topology and the deterministic decision counts.
    let timing: Value =
        serde_json::from_slice(&fs::read(plain.join("timing.json")).expect("timing"))
            .expect("JSON");
    let keys: Vec<&str> = timing
        .as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        [
            "decisions",
            "decisions_per_game",
            "finished_at",
            "games",
            "games_per_second",
            "match_seconds",
            "matches",
            "mean_match_seconds",
            "note",
            "platform",
            "python",
            "started_at",
            "topology",
            "wall_seconds",
            "workers",
        ]
    );
    let games = fs::read_to_string(plain.join("games.ndjson")).expect("games");
    let counts: Vec<u64> = games
        .lines()
        .map(|l| {
            let v: Value = serde_json::from_str(l).expect("record");
            u64::try_from(v["actions"].as_array().expect("actions").len()).expect("count")
        })
        .collect();
    assert_eq!(timing["decisions_per_game"], serde_json::json!(counts));
    let _ = fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------------------------
// W-119

const FIXTURE: &str = "crates/gcg-eval/tests/fixtures/gapc-transcript-replay.json";

/// SHA-256 of `scripts/blind_transcripts.py`'s transcript of the fixture, run with its EX count
/// fixed (line 98 compares `card_type.value` with `"ex_resource"`, but `CardType.EX_RESOURCE`
/// is `"EX RESOURCE"`, so the script prints `EX 0` for every board).
const PYTHON_FIXED_SHA256: &str =
    "c688a00c7d170e91c38b91ccebafd0d9c4f1c14c0d775f0adca8fbc0ead79839";
/// The same with the script as committed: it differs only in the `EX` count.
const PYTHON_AS_COMMITTED_SHA256: &str =
    "d22220670a09ad52c6dc4c89640354a2b0a608a01c7df171aad3dbe01652cc34";
const FIXTURE_ID: &str = "992c2300e7c5";

fn fixture() -> Value {
    read_json(FIXTURE)
}

fn put(dir: &Path, doc: &Value) {
    fs::create_dir_all(dir).expect("dir");
    fs::write(
        dir.join("renamed.json"),
        serde_json::to_string_pretty(doc).expect("JSON"),
    )
    .expect("write");
}

#[test]
fn w119_one_id_per_game_wherever_it_lives_and_no_search_statistics() {
    let dir = scratch("transcripts");
    let runs = dir.join("runs");
    let mut logged = fixture();
    logged["decision_log"] = serde_json::json!([
        {"visits": 40, "iterations": 40, "mean": 0.5, "alternatives": [], "agent": "mcts search"}
    ]);
    for seat in logged["seats"].as_array_mut().expect("seats") {
        seat["agent"] = Value::from("greedy-search");
    }
    put(&runs.join("a/deep/dir/replays"), &fixture());
    put(&runs.join("b/replays"), &logged);
    let out = dir.join("out");
    let key_file = dir.join("key.json");
    let stdout = ok(&["transcripts", s(&out), s(&key_file), "--run", s(&runs)]);
    assert_eq!(
        stdout,
        format!("1 transcripts in {}; key in {}\n", s(&out), s(&key_file))
    );
    let key: Value = serde_json::from_slice(&fs::read(&key_file).expect("key")).expect("JSON");
    let files: Vec<String> = ["a/deep/dir/replays/renamed.json", "b/replays/renamed.json"]
        .iter()
        .map(|r| s(&runs.join(r)).to_owned())
        .collect();
    assert_eq!(key, serde_json::json!({ FIXTURE_ID: files }));
    let body = fs::read_to_string(out.join(format!("game-{FIXTURE_ID}.txt"))).expect("transcript");
    let lower = body.to_lowercase();
    assert!(lower.contains("game over") && lower.contains("played:"));
    for w in gcg_eval::transcripts::LEAKS {
        assert!(!lower.contains(w), "the transcript names {w:?}");
    }
    assert_eq!(sha256_hex(body.as_bytes()), PYTHON_FIXED_SHA256);
    assert_ne!(PYTHON_FIXED_SHA256, PYTHON_AS_COMMITTED_SHA256);
    // The id ignores agents and logs and follows the actions.
    let doc = fixture();
    let id = gcg_eval::transcripts::game_id(&doc).expect("an id");
    assert_eq!(id, FIXTURE_ID);
    assert_eq!(gcg_eval::transcripts::game_id(&logged).expect("an id"), id);
    let mut fewer = doc.clone();
    fewer["actions"].as_array_mut().expect("actions").pop();
    assert_ne!(gcg_eval::transcripts::game_id(&fewer).expect("an id"), id);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn w119_a_replay_that_does_not_reproduce_is_refused() {
    let dir = scratch("transcripts-refused");
    let mut doc = fixture();
    doc["expected"]["turns"] = Value::from(3);
    put(&dir.join("run/replays"), &doc);
    let out = dir.join("out");
    let o = gcg_eval(&[
        "transcripts",
        s(&out),
        s(&dir.join("key.json")),
        "--run",
        s(&dir.join("run")),
    ]);
    assert_eq!(o.status.code(), Some(1));
    assert_eq!(
        text(&o.stderr),
        format!(
            "error: {}: the replay does not reproduce: ['turns: expected 3, replay gave 25']\n",
            s(&dir.join("run/replays/renamed.json"))
        )
    );
    assert!(!dir.join("key.json").exists());
    let usage = gcg_eval(&["transcripts", s(&out), s(&dir.join("key.json"))]);
    assert_eq!(usage.status.code(), Some(2));
    assert!(
        text(&usage.stderr).contains("the following arguments are required: --run"),
        "{}",
        text(&usage.stderr)
    );
    let _ = fs::remove_dir_all(&dir);
}
