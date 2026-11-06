//! W-164 and W-157: `CardDb::from_sources` on synthetic sources that take the loader paths the
//! packaged data never takes (a token definition with no matching card, two matching token
//! cards, the nested per-number override form, the singular `card_number` key), plus the fatal
//! load errors, against Python's `CardDB` on the same sources (`synthetic.json`).

mod common;

use common::{arr, def_mismatches, fixture, s, u};
use gcg_data::{CardDb, DataError, DefId};

fn variant_name(e: &DataError) -> &'static str {
    match e {
        DataError::Io { .. } => "Io",
        DataError::Json { .. } => "Json",
        DataError::Shape { .. } => "Shape",
        DataError::UnknownCanonicalPrinting { .. } => "UnknownCanonicalPrinting",
        DataError::UnknownCardType { .. } => "UnknownCardType",
        DataError::UnknownColor { .. } => "UnknownColor",
        DataError::UnparseableLink { .. } => "UnparseableLink",
        DataError::BadModifier { .. } => "BadModifier",
        DataError::UnknownPredicateOperator { .. } => "UnknownPredicateOperator",
    }
}

#[test]
fn synthetic_sources_load_as_in_python() {
    let fx = fixture("synthetic.json");
    let cases = arr(&fx["cases"]);
    assert!(cases.len() >= 10);
    for case in cases {
        let id = s(&case["id"]);
        let result = CardDb::from_sources(s(&case["cards_ndjson"]), s(&case["overrides"]));
        if let Some(err) = case.get("error") {
            let e = result
                .err()
                .unwrap_or_else(|| panic!("{id}: Rust loaded, Python raised"));
            assert_eq!(variant_name(&e), s(&err["rust"]), "{id}: {e}");
            let python = s(&err["python"]);
            if let Some(message) = python.strip_prefix("ValueError: ") {
                assert!(
                    e.to_string().ends_with(message),
                    "{id}: {e:?} does not end with Python's {message:?}"
                );
            }
            continue;
        }
        let db = result.unwrap_or_else(|e| panic!("{id}: {e}"));
        let want = &case["db"];
        let mismatches = def_mismatches(db.defs(), arr(&want["defs"]));
        assert!(mismatches.is_empty(), "{id}: {mismatches:#?}");
        let fp: Vec<&str> = arr(&want["fingerprints"]).iter().map(s).collect();
        assert_eq!(db.fingerprints().as_slice(), fp.as_slice(), "{id}");
        for t in arr(&want["tokens"]) {
            let id_want = DefId(u16::try_from(u(&t[1])).expect("def_id"));
            assert_eq!(
                db.token_for_key(s(&t[0])).map(|d| d.def_id),
                Some(id_want),
                "{id}: token {}",
                t[0]
            );
        }
        let applied: Vec<String> = db
            .applied_overrides()
            .iter()
            .map(|o| {
                format!(
                    "{} {} {} {:?}",
                    o.conflict_id, o.card_number, o.decision, o.fields
                )
            })
            .collect();
        let applied_want: Vec<String> = arr(&want["applied_overrides"])
            .iter()
            .map(|o| {
                let fields: Vec<Box<str>> = arr(&o[3]).iter().map(|f| s(f).into()).collect();
                format!("{} {} {} {:?}", s(&o[0]), s(&o[1]), s(&o[2]), fields)
            })
            .collect();
        assert_eq!(applied, applied_want, "{id}");
        let variants = arr(&want["unit_variants"]);
        let got: Vec<(u16, u16)> = db
            .defs()
            .iter()
            .filter_map(|d| db.unit_variant(d.def_id).map(|v| (d.def_id.0, v.def_id.0)))
            .collect();
        let want_variants: Vec<(u16, u16)> = variants
            .iter()
            .map(|v| {
                (
                    u16::try_from(u(&v[0])).expect("def_id"),
                    u16::try_from(u(&v[1])).expect("def_id"),
                )
            })
            .collect();
        assert_eq!(got, want_variants, "{id}");
    }
}

/// W-157: an unparsable overrides document is an error naming the file, never an empty
/// default; the packaged data loads all 402 resolutions' card-data changes.
#[test]
fn overrides_never_fall_back_to_empty() {
    let cards = r#"{"product_id":"SY-1","card_number":"SY-1","name":"A","card_type":"UNIT"}"#;
    for bad in ["", "{", "{\"resolutions\": {", "not json"] {
        let e = CardDb::from_sources(cards, bad).err().expect("must fail");
        assert!(matches!(&e, DataError::Json { file, .. } if &**file == "overrides.json"));
        assert!(
            e.to_string()
                .starts_with("overrides.json is not valid JSON"),
            "{e}"
        );
    }
    let e = CardDb::from_sources(cards, "[]").err().expect("must fail");
    assert!(matches!(&e, DataError::Shape { file, .. } if &**file == "overrides.json"));
    let e = CardDb::from_sources("{\"card_number\": 1}\n", "{}")
        .err()
        .expect("must fail");
    assert!(matches!(&e, DataError::Shape { file, .. } if &**file == "cards.ndjson"));
    let e = CardDb::from_sources("\n\n{oops\n", "{}")
        .err()
        .expect("must fail");
    assert!(matches!(&e, DataError::Json { line: 3, .. }), "{e}");
    assert_eq!(CardDb::embedded().applied_overrides().len(), 194);
}
