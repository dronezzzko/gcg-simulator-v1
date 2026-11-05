//! Schema negative controls (T-076; PLAN §9.5): every document in
//! `tests/vectors/invalid-results/` is a Python `results.json` with one mutation that breaks one
//! assertion keyword of `results.v1.schema.json`, as jsonschema judged it
//! (`oracle/export_parity.py --section invalid`). The 1.1.0 validator must reject each
//! one, at the location jsonschema reports or inside it, and accept the unmutated base. The
//! controls cover every keyword the schema uses except the five annotations, so a keyword the
//! validator silently ignored would let its control through. `gcg-diff schema-check
//! --expect-invalid tests/vectors/invalid-results/*.json` runs the same files through the CLI.

use std::fs;
use std::path::{Path, PathBuf};

use gcg_runner::SchemaVersion;
use gcg_runner::reports::schema::{RESULTS_SCHEMA_V1_1, SUPPORTED_KEYWORDS, Schema};
use gcg_runner::reports::{json_from_value, validate_results};
use serde_json::Value;

fn vectors() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/vectors")
}

fn read_json(path: &Path) -> Value {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn index() -> Value {
    read_json(&vectors().join("parity/invalid-results.json"))
}

fn strs(v: &Value) -> Vec<String> {
    v.as_array()
        .expect("a list")
        .iter()
        .map(|s| s.as_str().expect("a string").to_owned())
        .collect()
}

#[test]
fn the_controls_cover_every_assertion_keyword_of_the_schema() {
    let ix = index();
    let keywords = strs(&ix["schema_keywords"]);
    let mut supported: Vec<String> = SUPPORTED_KEYWORDS.iter().map(|k| (*k).to_owned()).collect();
    supported.sort();
    assert_eq!(
        keywords, supported,
        "the schema's keywords are the validator's"
    );
    let annotations = strs(&ix["annotations"]);
    let mut covered: Vec<String> = ix["controls"]
        .as_array()
        .expect("controls")
        .iter()
        .map(|c| c["keyword"].as_str().expect("keyword").to_owned())
        .chain(annotations)
        .collect();
    covered.sort();
    assert_eq!(covered, keywords, "one control per assertion keyword");
}

#[test]
fn the_directory_holds_exactly_the_indexed_controls() {
    let ix = index();
    let mut listed = ix["controls"]
        .as_array()
        .expect("controls")
        .iter()
        .map(|c| c["file"].as_str().expect("file").to_owned())
        .collect::<Vec<_>>();
    listed.sort();
    let mut on_disk: Vec<String> = fs::read_dir(vectors().join("invalid-results"))
        .expect("invalid-results")
        .map(|e| e.expect("entry").file_name().into_string().expect("UTF-8"))
        .collect();
    on_disk.sort();
    assert_eq!(on_disk, listed);
}

#[test]
fn the_base_document_validates() {
    let base = index()["base"].as_str().expect("base").to_owned();
    let doc = read_json(&vectors().join("records").join(base).join("results.json"));
    assert_eq!(
        validate_results(&json_from_value(&doc), SchemaVersion::V1_1),
        Ok(())
    );
}

#[test]
fn every_control_is_rejected_where_jsonschema_rejects_it() {
    let ix = index();
    let controls = ix["controls"].as_array().expect("controls");
    assert!(controls.len() >= 21, "{} controls", controls.len());
    for c in controls {
        let file = c["file"].as_str().expect("file");
        let doc = read_json(&vectors().join("invalid-results").join(file));
        let err = validate_results(&json_from_value(&doc), SchemaVersion::V1_1)
            .expect_err(&format!("{file} must not validate"));
        let python = c["pointer"].as_str().expect("pointer");
        assert!(
            err.pointer == python || err.pointer.starts_with(&format!("{python}/")),
            "{file}: rejected at {:?}, jsonschema rejects {python:?} ({err})",
            err.pointer
        );
    }
}

#[test]
fn a_control_differs_from_the_base_in_its_mutation_only() {
    let ix = index();
    let base_name = ix["base"].as_str().expect("base").to_owned();
    let base = read_json(
        &vectors()
            .join("records")
            .join(base_name)
            .join("results.json"),
    );
    for c in ix["controls"].as_array().expect("controls") {
        let file = c["file"].as_str().expect("file");
        let mut doc = read_json(&vectors().join("invalid-results").join(file));
        let m = c["mutation"].as_array().expect("mutation");
        let (op, pointer) = (m[0].as_str().expect("op"), m[1].as_str().expect("pointer"));
        let parts: Vec<&str> = pointer.split('/').skip(1).collect();
        let (last, parents) = parts.split_last().expect("a pointer below the root");
        let mut parent = &mut doc;
        for p in parents {
            parent = match parent {
                Value::Array(items) => &mut items[p.parse::<usize>().expect("index")],
                Value::Object(map) => map.get_mut(*p).expect("key"),
                _ => panic!("{file}: {pointer} crosses a scalar"),
            };
        }
        let original = base.pointer(pointer).cloned();
        match (op, parent) {
            ("remove", Value::Object(map)) => {
                assert!(map.get(*last).is_none(), "{file}");
                map.insert((*last).to_owned(), original.expect("removed value"));
            }
            ("add", Value::Array(items)) if *last == "-" => {
                items.pop();
            }
            ("add", Value::Object(map)) => {
                assert!(original.is_none(), "{file}: add creates {pointer}");
                map.shift_remove(*last);
            }
            ("replace", Value::Array(items)) => {
                items[last.parse::<usize>().expect("index")] = original.expect("replaced");
            }
            ("replace", Value::Object(map)) => {
                map.insert((*last).to_owned(), original.expect("replaced value"));
            }
            (op, _) => panic!("{file}: unexpected {op} at {pointer}"),
        }
        assert_eq!(
            doc, base,
            "{file}: undoing the mutation gives the base document"
        );
    }
}

/// Removes keyword `k` wherever it is a keyword of a subschema (never a property name under
/// `properties`/`$defs`, never inside `const`/`enum` data).
fn without_keyword(schema: &mut Value, k: &str) {
    let Value::Object(map) = schema else { return };
    map.shift_remove(k);
    for (key, value) in map.iter_mut() {
        match key.as_str() {
            "properties" | "$defs" => {
                if let Value::Object(children) = value {
                    for child in children.values_mut() {
                        without_keyword(child, k);
                    }
                }
            }
            "oneOf" | "prefixItems" => {
                if let Value::Array(children) = value {
                    for child in children {
                        without_keyword(child, k);
                    }
                }
            }
            "items" | "additionalProperties" | "unevaluatedProperties" | "propertyNames" => {
                without_keyword(value, k);
            }
            _ => {}
        }
    }
}

/// Negative control of the controls: with its keyword taken out of the schema, every control of
/// a keyword whose removal only relaxes the schema validates, so each control fails because of
/// its keyword and nothing else. Three keywords are excluded because removing them does not only
/// relax: `$ref` and `properties` change what `unevaluatedProperties` and
/// `additionalProperties` see, and without `type` a `null` matches both branches of the
/// nullable `oneOf`s.
#[test]
fn each_control_fails_only_because_of_its_keyword() {
    let ix = index();
    let schema: Value = serde_json::from_str(RESULTS_SCHEMA_V1_1).expect("schema");
    let mut checked = 0;
    for c in ix["controls"].as_array().expect("controls") {
        let keyword = c["keyword"].as_str().expect("keyword");
        if matches!(keyword, "$ref" | "properties" | "type") {
            continue;
        }
        let mut relaxed = schema.clone();
        without_keyword(&mut relaxed, keyword);
        let compiled =
            Schema::compile(&relaxed.to_string()).unwrap_or_else(|e| panic!("{keyword}: {e}"));
        let file = c["file"].as_str().expect("file");
        let doc = read_json(&vectors().join("invalid-results").join(file));
        assert_eq!(
            compiled.validate(&json_from_value(&doc)),
            Ok(()),
            "{file} still fails without {keyword}"
        );
        checked += 1;
    }
    assert_eq!(checked, 18);
}
