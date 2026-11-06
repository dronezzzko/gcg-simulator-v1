//! T-035: the embedded `CardDb` equals Python's `get_card_db()` field by field (1153
//! definitions), with the four recorded fingerprints, the token map, the applied overrides,
//! the Unit variants and the lookups (`oracle/export_data.py`, `carddefs.json`).

mod common;

use common::{arr, def_mismatches, fixture, opt_s, s, u};
use gcg_data::{CardDb, DefId, resolve_card_number};

fn def_id(v: &serde_json::Value) -> DefId {
    DefId(u16::try_from(u(v)).expect("def_id fits u16"))
}

#[test]
fn every_card_def_equals_python_field_by_field() {
    let fx = fixture("carddefs.json");
    let db = CardDb::embedded();
    assert_eq!(db.len(), 1153);
    assert!(!db.is_empty());
    assert_eq!(db.len() as u64, u(&fx["len"]));
    let mismatches = def_mismatches(db.defs(), arr(&fx["defs"]));
    assert!(
        mismatches.is_empty(),
        "{} mismatches, first: {:#?}",
        mismatches.len(),
        &mismatches[..mismatches.len().min(10)]
    );
    for (i, d) in db.defs().iter().enumerate() {
        assert_eq!(usize::from(d.def_id.0), i);
        assert_eq!(db.by_id(d.def_id).def_id, d.def_id);
    }
}

#[test]
fn fingerprints_equal_python() {
    let fx = fixture("carddefs.json");
    let want: Vec<&str> = arr(&fx["fingerprints"]).iter().map(s).collect();
    let got = CardDb::embedded().fingerprints();
    assert_eq!(got.as_slice(), want.as_slice());
    // Recorded at HEAD d9d41af.
    assert_eq!(
        got,
        [
            "c38ea9017c2109ff622745f53eaed210c526369fd8f2aeaa7b8ae17c76baa7de",
            "c4bb4d9269df02c745a753059cfb0f5993290600fc3202d8e54eadd1fc8cd5df",
            "176b958ac9db6a04ae9c71285629fd390bf75f06e3c8fa90d065167958130105",
            "613cd0b24183fe239d6806a4442f87d8ae1c0b72e8efa7cc8513123df5d8273b",
        ]
    );
}

#[test]
fn tokens_overrides_and_variants_equal_python() {
    let fx = fixture("carddefs.json");
    let db = CardDb::embedded();
    let tokens = arr(&fx["tokens"]);
    assert_eq!(tokens.len(), 29);
    for t in tokens {
        let key = s(&t[0]);
        let got = db.token_for_key(key).map(|d| d.def_id);
        assert_eq!(got, Some(def_id(&t[1])), "token {key:?}");
    }
    assert!(db.token_for_key("Nobody|X|1|1|").is_none());

    let applied: Vec<(&str, &str, &str, Vec<&str>)> = db
        .applied_overrides()
        .iter()
        .map(|o| {
            (
                &*o.conflict_id,
                &*o.card_number,
                &*o.decision,
                o.fields.iter().map(|f| &**f).collect(),
            )
        })
        .collect();
    let want: Vec<(&str, &str, &str, Vec<&str>)> = arr(&fx["applied_overrides"])
        .iter()
        .map(|o| {
            (
                s(&o[0]),
                s(&o[1]),
                s(&o[2]),
                arr(&o[3]).iter().map(s).collect(),
            )
        })
        .collect();
    assert_eq!(applied.len(), 166);
    assert_eq!(applied, want);

    let variants = arr(&fx["unit_variants"]);
    assert_eq!(variants.len(), 1);
    for d in db.defs() {
        let expected = variants
            .iter()
            .find(|v| def_id(&v[0]) == d.def_id)
            .map(|v| def_id(&v[1]));
        assert_eq!(db.unit_variant(d.def_id).map(|v| v.def_id), expected);
        let base = variants
            .iter()
            .find(|v| def_id(&v[1]) == d.def_id)
            .map_or(d.def_id, |v| def_id(&v[0]));
        assert_eq!(db.base_def_id(d.def_id), base);
    }
    assert_eq!(db.ex_base().def_id, def_id(&fx["ex_base"]));
    assert_eq!(db.ex_resource().def_id, def_id(&fx["ex_resource"]));
    assert_eq!(db.real_cards().count() as u64, u(&fx["real_cards"]));
}

#[test]
fn lookups_equal_python() {
    let fx = fixture("carddefs.json");
    let db = CardDb::embedded();
    let lookups = arr(&fx["lookups"]);
    assert!(lookups.len() > 3000);
    for row in lookups {
        let q = s(&row[0]);
        let want = (!row[1].is_null()).then(|| def_id(&row[1]));
        assert_eq!(db.get(q).map(|d| d.def_id), want, "get({q:?})");
        assert_eq!(db.normalize_id(q), opt_s(&row[2]), "normalize_id({q:?})");
    }
    for row in arr(&fx["resolve"]) {
        let w = s(&row[0]);
        assert_eq!(resolve_card_number(w, db), opt_s(&row[1]), "resolve({w:?})");
    }
}
