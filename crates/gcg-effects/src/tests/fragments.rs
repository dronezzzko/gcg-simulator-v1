//! IR fragments of script overrides (PLAN §3.4 "Test overrides"): the fixtures that
//! `oracle/export_ir.py` exports by applying three `testkit.card_text` overrides of the
//! Python tests in fresh processes. Applying a fragment to the base IR must give the
//! overridden card Python's golden record, Python's content ids for every new node and
//! program, and the registry-global values of `_index_events`.

use std::fs;

use gcg_core::pyjson::dumps;
use gcg_data::DefId;
use serde_json::Value;

use super::{repo_root, tables};
use crate::contracts::{AbilityId, IrError, NodeId, ProgramId};
use crate::dsl::Ev;
use crate::golden::GOLDEN_DUMP;
use crate::ir::Tables;
use crate::pyrepr::content_id_of;

fn fixtures() -> Vec<Value> {
    let dir = repo_root().join("crates/gcg-effects/ir/fragments");
    let mut names: Vec<_> = fs::read_dir(&dir)
        .expect("fragment fixtures")
        .map(|e| e.expect("entry").path())
        .collect();
    names.sort();
    names
        .iter()
        .map(|p| serde_json::from_str(&fs::read_to_string(p).expect("readable")).expect("JSON"))
        .collect()
}

/// `serde_json::Value` as `gcg_core::pyjson::Json`, for a byte comparison under
/// `GOLDEN_DUMP`.
fn to_json(v: &Value) -> gcg_core::pyjson::Json {
    use gcg_core::pyjson::Json;
    match v {
        Value::Null => Json::Null,
        Value::Bool(b) => Json::Bool(*b),
        Value::Number(n) => Json::Int(i128::from(n.as_i64().expect("ints only"))),
        Value::String(s) => Json::Str(s.clone()),
        Value::Array(a) => Json::Array(a.iter().map(to_json).collect()),
        Value::Object(o) => Json::Object(
            o.iter()
                .map(|(k, v)| (k.as_str().into(), to_json(v)))
                .collect(),
        ),
    }
}

fn def_of(t: &Tables, number: &str) -> DefId {
    let i = t
        .card_texts
        .iter()
        .position(|c| &*c.card_number == number)
        .expect("a card");
    DefId(u16::try_from(i).expect("fits"))
}

#[test]
fn fragments_reproduce_pythons_override() {
    let base = tables();
    let all = fixtures();
    assert_eq!(all.len(), 4);
    for fx in &all {
        let number = fx["card_number"].as_str().expect("card number");
        let frag = &fx["fragment"];
        let t = base
            .with_fragment(frag)
            .unwrap_or_else(|e| panic!("{number}: {e}"));
        let v = t.view();
        let def = def_of(&t, number);
        assert_eq!(
            dumps(&t.golden_json(def), GOLDEN_DUMP),
            dumps(&to_json(&fx["golden"]), GOLDEN_DUMP),
            "{number}: golden record under the override"
        );
        let new_nodes = frag["node_content_ids"].as_array().expect("ids");
        assert_eq!(t.nodes.len(), base.nodes.len() + new_nodes.len());
        for (k, cid) in new_nodes.iter().enumerate() {
            let id = NodeId(u32::try_from(base.nodes.len() + k).expect("fits"));
            assert_eq!(
                content_id_of(&v.repr(id)).as_str(),
                cid.as_str().expect("cid"),
                "{number} node {k}"
            );
        }
        for k in base.programs.len()..t.programs.len() {
            let id = ProgramId(u16::try_from(k).expect("fits"));
            assert_eq!(content_id_of(&v.program_repr(id)), v.program(id).content_id);
        }
        let entry = v.card(def);
        assert!(
            entry
                .own
                .iter()
                .all(|a| usize::from(a.0) >= base.abilities.len()),
            "Python's new aids"
        );
        for a in &*entry.own {
            assert_eq!(v.ability(*a).aid, *a);
            assert_eq!(v.ability(*a).def_id, def);
        }
        assert_eq!(
            Some(t.hp_reduction_possible),
            frag["hp_reduction_possible"].as_bool()
        );
        assert_eq!(t.always_events, base.always_events);
        assert_eq!(
            t.card_texts[usize::from(def.0)].card_number.as_ref(),
            number
        );
        // Every other card is unchanged.
        for (i, c) in base.cards.iter().enumerate() {
            if i != usize::from(def.0) {
                assert_eq!(c.own, t.cards[i].own);
            }
        }
    }
}

#[test]
fn the_hp_minus_override_makes_hp_reduction_possible() {
    let fx = fixtures()
        .into_iter()
        .find(|f| f["card_number"] == "GD01-064")
        .expect("the DINN fixture");
    let t = tables().with_fragment(&fx["fragment"]).expect("applies");
    assert!(
        t.hp_reduction_possible,
        "tests/rules/test_damage.py:791 (W-51)"
    );
    let def = def_of(&t, "GD01-064");
    assert!(t.view().events_of(def).contains(Ev::Deployed));
    assert_eq!(t.view().card(def).own, [AbilityId(1447)].into());
}

#[test]
fn a_fragment_of_another_base_is_refused() {
    let mut frag = fixtures()[0]["fragment"].clone();
    frag["base_fingerprint"] = Value::String("0".repeat(64));
    let err = tables().with_fragment(&frag).err().expect("refused");
    assert!(
        matches!(
            err,
            IrError::DataMismatch {
                which: "base_fingerprint",
                ..
            }
        ),
        "{err}"
    );
}

#[test]
fn a_fragment_cannot_be_applied_twice() {
    let frag = &fixtures()[0]["fragment"];
    let once = tables().with_fragment(frag).expect("applies to the base");
    let err = once.with_fragment(frag).err().expect("its base changed");
    assert!(matches!(err, IrError::DataMismatch { .. }), "{err}");
}

#[test]
fn a_name_alias_override_changes_the_aliases() {
    let fx = fixtures()
        .into_iter()
        .find(|f| f["card_number"] == "GD01-062")
        .expect("the alias fixture");
    let t = tables().with_fragment(&fx["fragment"]).expect("applies");
    let v = t.view();
    let def = def_of(&t, "GD01-062");
    let names: Vec<&str> = v.aliases(def).iter().map(|s| v.str(*s)).collect();
    assert_eq!(names, ["Char Aznable"]);
    assert_eq!(t.aliases.len(), tables().aliases.len() + 1);
    assert!(t.aliases.windows(2).all(|w| w[0].0 < w[1].0));
}
