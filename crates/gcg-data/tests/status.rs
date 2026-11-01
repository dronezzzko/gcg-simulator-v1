//! T-038 and T-039: the `data status` parts, `versions()` and `conflict_resolutions` against
//! Python's output (`status.json`). The CLI (workstream E) renders `data status`; here the
//! parts are assembled with Python's `coverage` and rendered the way `cli.py:294-331` does, and
//! both outputs must equal Python's stdout byte for byte.

mod common;

use common::{arr, fixture, s, to_json};
use gcg_core::pyfloat::format_fixed;
use gcg_core::pyjson::{DumpOptions, Json, dumps};
use gcg_data::{conflict_resolutions, data_status_parts, parse_deck, versions};
use sha2::{Digest, Sha256};

/// `json.dumps(obj, ensure_ascii=False)` with Python's default separators.
const PY_DEFAULT_UTF8: DumpOptions = DumpOptions {
    indent: None,
    sort_keys: false,
    compact: false,
    ensure_ascii: false,
};

fn get<'a>(v: &'a Json, key: &str) -> &'a Json {
    v.get(key).unwrap_or_else(|| panic!("missing {key}"))
}

fn text(v: &Json) -> String {
    match v {
        Json::Str(x) => x.clone(),
        Json::Int(i) => i.to_string(),
        Json::Null => "None".to_owned(),
        other => panic!("unexpected {other:?}"),
    }
}

fn int(v: &Json) -> i128 {
    match v {
        Json::Int(i) => *i,
        other => panic!("expected an int, got {other:?}"),
    }
}

fn len(v: &Json) -> usize {
    match v {
        Json::Array(a) => a.len(),
        Json::Object(o) => o.len(),
        other => panic!("expected a container, got {other:?}"),
    }
}

/// `_status_text(status)` (`cli.py:294-322`), as the CLI renders it.
fn status_text(status: &Json) -> String {
    let (v, snap, bl, cov, con) = (
        get(status, "versions"),
        get(status, "snapshot"),
        get(status, "banlist"),
        get(status, "coverage"),
        get(status, "conflicts"),
    );
    let implemented = int(get(cov, "implemented"));
    let card_numbers = int(get(cov, "card_numbers"));
    let pct = if card_numbers == 0 {
        0.0
    } else {
        (100 * implemented) as f64 / card_numbers as f64
    };
    let t = |o: &Json, k: &str| text(get(o, k));
    let mut lines = vec![
        format!(
            "gcg-sim {} (results schema {})",
            t(v, "package"),
            t(v, "results_schema")
        ),
        format!("card data: gcg-api dataset {}", t(snap, "dataset_version")),
        format!(
            "  source commit {}, built {}",
            t(snap, "source_commit"),
            t(snap, "built_at")
        ),
        format!(
            "  {} printings, {card_numbers} card numbers, {} sets, {} rulings, {} rules FAQ \
             entries, {} errata",
            t(snap, "card_count"),
            t(snap, "set_count"),
            t(snap, "ruling_count"),
            t(snap, "rules_faq_count"),
            t(snap, "errata_count")
        ),
        format!(
            "rules: Comprehensive Rules Ver. {} (effective {})",
            t(v, "rules_version"),
            t(v, "rules_effective_date")
        ),
        format!(
            "banned & restricted list: effective {} ({} banned, {} restricted, {} banned pairs, \
             {} attribute pair rule(s))",
            t(bl, "effective_date"),
            len(get(bl, "banned")),
            len(get(bl, "restricted")),
            len(get(bl, "banned_pairs")),
            len(get(bl, "attribute_pair_rules"))
        ),
        format!(
            "BO3 match rules: effective {}",
            t(v, "bo3_rules_effective_date")
        ),
        format!(
            "implementation coverage: {implemented}/{card_numbers} card numbers ({}%)",
            format_fixed(pct, 1)
        ),
    ];
    if let Json::Object(by_type) = get(cov, "by_type") {
        for (k, c) in by_type {
            let gcg_core::pyjson::Key::Str(name) = k else {
                panic!("type keys are strings")
            };
            lines.push(format!(
                "  {name}: {}/{}",
                int(get(c, "implemented")),
                int(get(c, "cards"))
            ));
        }
    }
    lines.push(format!(
        "conflicts: {} resolutions in overrides.json ({} card-data overrides on {} card \
         numbers); {} curated conflicts",
        t(con, "resolutions"),
        t(con, "card_data_overrides"),
        t(con, "card_numbers_overridden"),
        t(con, "curated_conflicts")
    ));
    lines.join("\n") + "\n"
}

#[test]
fn data_status_text_and_json_equal_python() {
    let fx = fixture("status.json");
    let parts = data_status_parts(s(&fx["package"]), s(&fx["code_digest"]));
    let mut status = Json::object();
    status.set("versions", parts.versions);
    status.set("snapshot", parts.snapshot);
    status.set("banlist", parts.banlist);
    status.set("coverage", to_json(&fx["coverage"]));
    status.set("conflicts", parts.conflicts);
    let json_out = dumps(&status, DumpOptions::PRETTY_SORTED_UTF8) + "\n";
    assert_eq!(json_out, s(&fx["json"]));
    assert_eq!(status_text(&status), s(&fx["text"]));
}

#[test]
fn versions_equal_python() {
    let fx = fixture("status.json");
    let v = versions(s(&fx["package"]), s(&fx["code_digest"]));
    assert_eq!(dumps(&v, PY_DEFAULT_UTF8), s(&fx["versions_json"]));
}

#[test]
fn conflict_resolutions_equal_python() {
    let fx = fixture("status.json");
    let rows = arr(&fx["conflict_resolutions"]);
    assert_eq!(rows.len(), 16);
    for row in rows {
        let (a, b) = (&row["deck_under_test"], &row["benchmark"]);
        let dut = parse_deck(s(&a[1]), s(&a[0]), None).expect("deck under test parses");
        let bench = parse_deck(s(&b[1]), s(&b[0]), None).expect("benchmark deck parses");
        let got = conflict_resolutions(&[("deck_under_test", &dut), ("benchmark", &bench)]);
        assert_eq!(
            dumps(&got, PY_DEFAULT_UTF8),
            s(&row["json"]),
            "{} vs {}",
            a[0],
            b[0]
        );
    }
}

/// `code_digest()` (`reports/versions.py:32-52`) on Python's synthetic tree: the vector the
/// binaries' build-time digest (workstream E, `build-support/`) must reproduce. Checked here
/// with the algorithm as PLAN §2.4 states it: skip `__pycache__` and `*.pyc`, sort by the whole
/// relative path, hash `path`, `\0`, bytes.
#[test]
fn code_digest_vector_is_consistent() {
    let fx = fixture("status.json");
    let v = &fx["code_digest_vector"];
    let mut files: Vec<(&str, Vec<u8>)> = arr(&v["files"])
        .iter()
        .map(|f| {
            let hex = s(&f[1]);
            let bytes = (0..hex.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("hex"))
                .collect();
            (s(&f[0]), bytes)
        })
        .filter(|(p, _)| {
            !p.split('/')
                .any(|c| c == "__pycache__" || c.ends_with(".pyc"))
        })
        .collect();
    files.sort_by(|a, b| a.0.cmp(b.0));
    let hashed: Vec<&str> = arr(&v["hashed"]).iter().map(s).collect();
    assert_eq!(files.iter().map(|f| f.0).collect::<Vec<_>>(), hashed);
    let mut h = Sha256::new();
    for (p, bytes) in &files {
        h.update(p.as_bytes());
        h.update(b"\0");
        h.update(bytes);
    }
    let digest: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(digest, s(&v["sha256"]));
}
