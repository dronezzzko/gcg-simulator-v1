//! PLAN §3.4 test 6: `gcg-data`'s four CardDb fingerprints equal the IR's `db_fingerprints`,
//! and the def_id order equals `cards[].card_number`; `Registry::from_ir` refuses an IR
//! exported from other card data with `IrError::DataMismatch`.

use std::fs;

use gcg_data::CardDb;
use serde_json::Value;

use super::{EMBEDDED_IR, raw, repo_root, tables};
use crate::contracts::{IrError, Registry};
use crate::ir::DB_FINGERPRINTS;

#[test]
fn gcg_data_fingerprints_equal_the_irs() {
    let db = CardDb::embedded();
    let fp = &raw()["db_fingerprints"];
    for ((name, _), ours) in DB_FINGERPRINTS.iter().zip(db.fingerprints()) {
        assert_eq!(
            fp[*name].as_str(),
            Some(ours.as_str()),
            "db_fingerprints.{name}"
        );
    }
    let ir_numbers: Vec<&str> = raw()["cards"]
        .as_array()
        .expect("cards")
        .iter()
        .map(|c| c["card_number"].as_str().expect("card_number"))
        .collect();
    let db_numbers: Vec<&str> = db.defs().iter().map(|d| &*d.card_number).collect();
    assert_eq!(ir_numbers, db_numbers, "def_id order");
    assert!(
        db.defs()
            .iter()
            .enumerate()
            .all(|(i, d)| usize::from(d.def_id.0) == i)
    );
    assert_eq!(tables().check_db(db), Ok(()));
}

#[test]
fn the_embedded_registry_is_checked_against_the_embedded_card_data() {
    let reg = Registry::embedded();
    assert!(std::ptr::eq(reg.db(), CardDb::embedded()));
    assert!(Registry::from_ir(EMBEDDED_IR, CardDb::embedded()).is_ok());
}

fn with_ir(edit: impl FnOnce(&mut Value)) -> String {
    let mut ir = raw().clone();
    edit(&mut ir);
    serde_json::to_string(&ir).expect("serialisable")
}

#[test]
fn an_ir_with_other_fingerprints_is_refused() {
    for (name, which) in DB_FINGERPRINTS {
        let text = with_ir(|ir| ir["db_fingerprints"][name] = Value::String("0".repeat(64)));
        let err = Registry::from_ir(&text, CardDb::embedded())
            .err()
            .expect("refused");
        assert!(
            matches!(&err, IrError::DataMismatch { which: w, expected, .. }
                if *w == which && *expected == "0".repeat(64)),
            "{err}"
        );
        assert!(err.to_string().contains("export_ir.py --write"), "{err}");
    }
    let text = with_ir(|ir| {
        ir["db_fingerprints"]
            .as_object_mut()
            .expect("an object")
            .shift_remove("token_keys");
    });
    assert!(matches!(
        Registry::from_ir(&text, CardDb::embedded()),
        Err(IrError::Malformed { .. })
    ));
}

/// A card database built from other card data: the packaged `cards.ndjson` without the
/// printings whose `product_id` is in `drop`, each remaining line passed through `edit`, and
/// the packaged overrides.
fn db_from(drop: &[&str], edit: impl Fn(&str) -> String) -> &'static CardDb {
    let root = repo_root().join("src/gcg_sim/data");
    let cards = fs::read_to_string(root.join("gcgapi/cards.ndjson")).expect("cards.ndjson");
    let overrides = fs::read_to_string(root.join("overrides.json")).expect("overrides.json");
    let kept: Vec<String> = cards
        .lines()
        .filter(|l| {
            let rec: Value = serde_json::from_str(l).expect("a printing");
            !drop.contains(&rec["product_id"].as_str().expect("product_id"))
        })
        .map(edit)
        .collect();
    assert_eq!(kept.len() + drop.len(), cards.lines().count());
    let db = CardDb::from_sources(&(kept.join("\n") + "\n"), &overrides).expect("loads");
    Box::leak(Box::new(db))
}

fn db_without(drop: &[&str]) -> &'static CardDb {
    db_from(drop, str::to_owned)
}

#[test]
fn an_ir_of_another_card_database_is_refused() {
    // One card fewer (EB01-060 has a single printing): the definitions differ.
    let fewer = db_without(&["EB01-060"]);
    assert_eq!(fewer.len() + 1, CardDb::embedded().len());
    let err = Registry::from_ir(EMBEDDED_IR, fewer)
        .err()
        .expect("refused");
    assert_eq!(
        err,
        IrError::DataMismatch {
            which: "defs",
            expected: "1153 definitions".into(),
            found: "1152 definitions".into(),
        }
    );
    // One alternative printing fewer: the same definitions with other product ids.
    let printing = db_without(&["GD01-001_p2"]);
    assert_eq!(printing.len(), CardDb::embedded().len());
    let err = Registry::from_ir(EMBEDDED_IR, printing)
        .err()
        .expect("refused");
    assert!(
        matches!(
            err,
            IrError::DataMismatch {
                which: "db_fingerprints.carddefs",
                ..
            }
        ),
        "{err}"
    );
    // A renumbered card: as many definitions, another number at some def_id.
    let renamed = db_from(&[], |l| l.replace("\"EB01-060\"", "\"EB01-960\""));
    assert_eq!(renamed.len(), CardDb::embedded().len());
    let err = Registry::from_ir(EMBEDDED_IR, renamed)
        .err()
        .expect("refused");
    assert!(
        matches!(&err, IrError::DataMismatch { which: "def_ids", expected, found }
            if expected.ends_with("= EB01-060") && found.contains("EB01-")),
        "{err}"
    );
}
