//! T-045 (M5.1 gate) through the public API: the Python fixture snapshots of
//! `tests/fixtures/states.ndjson` (`oracle/export_state_fixtures.py`) rebuild with
//! `canon::from_canonical`, re-serialise with `canon::canonical_state` to Python's bytes, and
//! `canon::state_digest` equals Python's `state_digest` (PLAN §4.7-§4.8).

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use gcg_core::pyjson::{DumpOptions, dumps};
use gcg_data::CardDb;
use gcg_effects::{InlineNode, Registry};
use gcg_engine::canon::{canonical_state, from_canonical, state_digest};
use gcg_engine::{CanonError, GameState, StateSnapshot};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

/// `json.dumps(x)` with Python's defaults (`dump_sha256` of the fixtures).
const PY_DEFAULT: DumpOptions = DumpOptions {
    indent: None,
    sort_keys: false,
    compact: false,
    ensure_ascii: true,
};

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn snapshots() -> Vec<Value> {
    fs::read_to_string(crate_dir().join("tests/fixtures/states.ndjson"))
        .expect("the fixture file")
        .lines()
        .map(|l| serde_json::from_str(l).expect("a JSON record"))
        .collect()
}

fn snapshot(rec: &Value) -> StateSnapshot {
    StateSnapshot {
        state: rec["state"].clone(),
        continuous: rec["continuous"].clone(),
        filter_sets: rec["filter_sets"].clone(),
        rules_checked: rec["rules_checked"].as_i64().expect("rules_checked"),
    }
}

fn sha256_hex(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The embedded registry plus the filter sets that the fixtures name: Python registers filter
/// tuples at run time (`effects/registry.py:132-138`), and the IR has none yet, so the test
/// adds them to a copy of the IR under Python's content ids (the IR loader is public).
fn registry_with_filter_sets(snaps: &[Value]) -> Arc<Registry> {
    let base = Registry::embedded();
    let text = fs::read_to_string(crate_dir().join("../gcg-effects/ir/registry.json"))
        .expect("the effect IR");
    let mut ir: Value = serde_json::from_str(&text).expect("JSON");
    let mut sets = ir["filter_sets"].as_array().expect("filter_sets").clone();
    for s in snaps {
        for (cid, filters) in s["filter_sets"].as_object().expect("an object") {
            let ids: Vec<Value> = filters
                .as_array()
                .expect("a filter list")
                .iter()
                .map(|f| match base.decode_inline(f) {
                    Ok(InlineNode::Arena(id)) => Value::from(id.0),
                    other => panic!("filter {f} is not an IR node: {other:?}"),
                })
                .collect();
            if !sets.iter().any(|x| x["content_id"] == *cid) {
                sets.push(json!({"filters": ids, "content_id": cid}));
            }
        }
    }
    ir["filter_sets"] = Value::Array(sets);
    Arc::new(Registry::from_ir(&ir.to_string(), CardDb::embedded()).expect("loads"))
}

fn check_round_trip(reg: &Arc<Registry>, rec: &Value) -> GameState {
    let id = rec["id"].as_str().expect("id");
    let st = from_canonical(reg, &snapshot(rec)).unwrap_or_else(|e| panic!("{id}: {e}"));
    let text = dumps(&canonical_state(&st), PY_DEFAULT);
    assert_eq!(
        sha256_hex(&text),
        rec["dump_sha256"],
        "{id}: Python's bytes"
    );
    let back: Value = serde_json::from_str(&text).expect("JSON");
    assert_eq!(back, rec["state"], "{id}");
    assert_eq!(state_digest(&st), rec["digest"], "{id}: state_digest");
    assert_eq!(st.rules_checked(), rec["rules_checked"].as_i64().unwrap());
    st
}

#[test]
fn every_fixture_snapshot_round_trips_with_pythons_digest() {
    let snaps = snapshots();
    assert_eq!(snaps.len(), 69);
    let ext = registry_with_filter_sets(&snaps);
    let embedded = Registry::embedded();
    let mut on_embedded = 0;
    for rec in &snaps {
        let st = check_round_trip(&ext, rec);
        // A snapshot of a state reloads to the same state.
        let again = StateSnapshot {
            state: serde_json::from_str(&dumps(&canonical_state(&st), PY_DEFAULT)).expect("JSON"),
            ..snapshot(rec)
        };
        let st2 = from_canonical(&ext, &again).expect("reloads");
        assert_eq!(state_digest(&st2), rec["digest"]);
        if rec["filter_sets"]
            .as_object()
            .is_some_and(serde_json::Map::is_empty)
        {
            check_round_trip(&embedded, rec);
            on_embedded += 1;
        }
    }
    assert_eq!(
        on_embedded, 68,
        "every snapshot but the synthetic one names no filter set"
    );
}

#[test]
fn a_filter_set_the_registry_lacks_is_unknown_content() {
    let snaps = snapshots();
    let rec = snaps
        .iter()
        .find(|r| r["id"] == "synthetic/rare-shapes")
        .expect("the synthetic snapshot");
    let cid = rec["state"]["lasting"][0][9]
        .as_str()
        .expect("a filter set");
    let err = from_canonical(&Registry::embedded(), &snapshot(rec))
        .err()
        .expect("refused");
    assert_eq!(
        err,
        CanonError::UnknownContent {
            kind: "filter_set",
            id: cid.into()
        }
    );
}

/// The content id Python wrote for the synthetic filter set is `sha256(repr(tuple))[:16]` of
/// the Rust `repr` of its decoded filter nodes (`runner/digest.py:26-27`, `:40-43`).
#[test]
fn side_table_filter_tuples_hash_to_their_content_ids() {
    let reg = Registry::embedded();
    let mut checked = 0;
    for rec in snapshots() {
        for (cid, filters) in rec["filter_sets"].as_object().expect("an object") {
            let reprs: Vec<String> = filters
                .as_array()
                .expect("a list")
                .iter()
                .map(|f| match reg.decode_inline(f) {
                    Ok(InlineNode::Arena(id)) => reg.repr(id),
                    other => panic!("{other:?}"),
                })
                .collect();
            let tuple = if reprs.len() == 1 {
                format!("({},)", reprs[0])
            } else {
                format!("({})", reprs.join(", "))
            };
            assert_eq!(&sha256_hex(&tuple)[..16], cid, "{tuple}");
            checked += 1;
        }
    }
    assert_eq!(checked, 1);
}

/// Every continuous effect of every side table resolves to an effect whose content id is
/// its key, whether the registry holds it (build-time) or not (runtime values).
#[test]
fn side_table_effects_hash_to_their_content_ids() {
    let reg = Registry::embedded();
    let (mut arena, mut detached) = (0, 0);
    for rec in snapshots() {
        for (cid, form) in rec["continuous"].as_object().expect("an object") {
            let c = match reg.decode_inline(form).expect("an inline node") {
                InlineNode::Arena(id) => {
                    arena += 1;
                    assert_eq!(reg.node_content_id(id).as_str(), cid);
                    reg.cont_of(reg.continuous_id(id).expect("a continuous node"))
                }
                InlineNode::Detached(node) => {
                    detached += 1;
                    let cont = match node {
                        gcg_effects::Node::StatMod(m) => match (m.ap, m.hp) {
                            (gcg_effects::ValueRef::Lit(ap), gcg_effects::ValueRef::Lit(hp)) => {
                                gcg_effects::ContRef::StatMod {
                                    ap: i32::try_from(ap).expect("fits"),
                                    hp: i32::try_from(hp).expect("fits"),
                                }
                            }
                            other => panic!("{other:?}"),
                        },
                        gcg_effects::Node::AbilityGrant(g) => gcg_effects::ContRef::AbilityGrant {
                            card: g.card_number,
                            index: i32::try_from(g.ability_index).expect("fits"),
                        },
                        other => panic!("an unexpected runtime effect {other:?}"),
                    };
                    assert_eq!(reg.cont_by_content_id(&cont.content_id(&reg)), None);
                    cont
                }
            };
            assert_eq!(c.content_id(&reg).as_str(), cid);
        }
    }
    assert!(
        arena > 10 && detached >= 2,
        "{arena} build-time, {detached} runtime"
    );
}
