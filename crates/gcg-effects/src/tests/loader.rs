//! The loader's checks (PLAN §3.4): version, schema, unknown classes and hooks with the card
//! number (W-69), node references, handle kinds, the string tables, typed hook parameters and
//! the error texts.

use serde_json::{Value, json};

use super::{raw, tables};
use crate::contracts::{
    CondParams, ContRef, FilterParams, HookParams, IR_REGENERATE_COMMAND, IrError, StepParams,
    ValueParams,
};
use crate::dsl::{Node, P};
use crate::ir::Tables;

fn load_modified(edit: impl FnOnce(&mut Value)) -> Result<Tables, IrError> {
    let mut ir = raw().clone();
    edit(&mut ir);
    Tables::decode(&serde_json::to_string(&ir).expect("serialisable"))
}

/// The index of the first node of class `class`.
fn first_node(class: &str) -> usize {
    raw()["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .position(|n| n["_"] == class)
        .unwrap_or_else(|| panic!("no {class} node"))
}

#[test]
fn another_ir_version_names_the_regeneration_command() {
    let err = load_modified(|ir| ir["ir_version"] = json!(2))
        .err()
        .expect("refused");
    assert_eq!(err, IrError::Version { found: 2 });
    assert!(err.to_string().contains(IR_REGENERATE_COMMAND));
}

#[test]
fn a_schema_difference_is_refused() {
    let err = load_modified(|ir| ir["schema"]["Count"][0][1] = json!("Sel | None"))
        .err()
        .expect("refused");
    assert!(
        matches!(err, IrError::Schema { ref detail } if detail.starts_with("Count.sel")),
        "{err}"
    );
    let err = load_modified(|ir| ir["enums"]["Kw"][0][1] = json!("Repair!"))
        .err()
        .expect("refused");
    assert!(matches!(err, IrError::Schema { .. }), "{err}");
    let err = load_modified(|ir| ir["hooks"]["value"][0] = json!("renamed"))
        .err()
        .expect("refused");
    assert!(matches!(err, IrError::Schema { .. }), "{err}");
}

#[test]
fn an_unknown_class_names_its_card() {
    let i = first_node("Draw");
    let err = load_modified(|ir| ir["nodes"][i]["_"] = json!("Draw2"))
        .err()
        .expect("refused");
    match err {
        IrError::UnknownClass { class, card_number } => {
            assert_eq!(class, "Draw2");
            assert!(
                card_number.is_some(),
                "a card's script holds every Draw node"
            );
        }
        other => panic!("{other}"),
    }
}

#[test]
fn an_unknown_hook_names_its_card() {
    let i = first_node("CustomCond");
    let err = load_modified(|ir| ir["nodes"][i]["name"] = json!("wp_nowhere"))
        .err()
        .expect("refused");
    match &err {
        IrError::UnknownHook {
            kind,
            name,
            card_number,
        } => {
            assert_eq!((*kind, name.as_str()), ("cond", "wp_nowhere"));
            assert!(card_number.is_some());
            assert!(
                err.to_string()
                    .contains(card_number.as_deref().unwrap_or("?"))
            );
        }
        other => panic!("{other}"),
    }
}

#[test]
fn references_must_point_back_to_a_node_of_the_right_kind() {
    let i = first_node("Sel");
    let last = raw()["nodes"].as_array().expect("nodes").len();
    let err = load_modified(|ir| ir["nodes"][i]["filters"] = json!([{"$": last - 1}]))
        .err()
        .expect("refused");
    assert!(
        matches!(err, IrError::Malformed { ref detail, .. } if detail.contains("not an earlier node")),
        "{err}"
    );
    let count = first_node("Count");
    let filter = first_node("IsKind");
    assert!(filter < count);
    let err = load_modified(|ir| ir["nodes"][count]["sel"] = json!({"$": filter}))
        .err()
        .expect("refused");
    assert!(
        matches!(err, IrError::Malformed { ref detail, .. } if detail.contains("IsKind")),
        "{err}"
    );
}

#[test]
fn a_card_without_a_script_is_refused() {
    let err = load_modified(|ir| ir["cards"][5]["script"] = Value::Null)
        .err()
        .expect("refused");
    assert!(
        matches!(err, IrError::Malformed { ref path, .. } if path == "$.cards[5].script"),
        "{err}"
    );
}

#[test]
fn strings_must_be_sorted_and_printable() {
    let err = load_modified(|ir| {
        let strings = ir["strings"].as_array_mut().expect("strings");
        strings.swap(0, 1);
    })
    .err()
    .expect("refused");
    assert!(
        matches!(err, IrError::Malformed { ref detail, .. } if detail.contains("sorted")),
        "{err}"
    );
    let err = load_modified(|ir| {
        let strings = ir["strings"].as_array_mut().expect("strings");
        strings.push(json!("\u{ffff}\u{200b}"));
    })
    .err()
    .expect("refused");
    assert!(
        matches!(err, IrError::Malformed { ref detail, .. } if detail.contains("printable")),
        "{err}"
    );
}

#[test]
fn an_ir_contref_must_be_canonical() {
    let err = load_modified(|ir| {
        let c = ir["continuous"]
            .as_array_mut()
            .expect("continuous")
            .iter_mut()
            .find(|c| c["cont"].get("StatMod").is_some())
            .expect("a StatMod value");
        let node = c["node"].clone();
        c["cont"] = json!({ "Node": node });
    })
    .err()
    .expect("refused");
    assert!(
        matches!(err, IrError::Malformed { ref detail, .. } if detail.contains("canonical")),
        "{err}"
    );
}

#[test]
fn hook_parameters_are_typed_with_last_key_wins() {
    let t = tables();
    let v = t.view();
    let params = |name: &str| -> Vec<HookParams> {
        t.nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| match n {
                Node::CustomStep(x) => x.name.as_str() == name,
                Node::CustomCond(x) => x.name.as_str() == name,
                Node::CustomFilter(x) => x.name.as_str() == name,
                Node::CustomValue(x) => x.name.as_str() == name,
                _ => false,
            })
            .map(|(i, _)| {
                *v.hook_params(crate::contracts::NodeId(u32::try_from(i).expect("fits")))
                    .expect("typed")
            })
            .collect()
    };
    // GD05-006 and GD06-002 share the hook, each with its own tag.
    assert!(params("wp_gd05_a_once_free").iter().all(|p| matches!(
        p,
        HookParams::Cond(CondParams::GD05aOnceFree {
            tag: -205_006 | -206_002
        })
    )));
    assert!(params("wp_gd05_a_use_once").iter().all(|p| matches!(
        p,
        HookParams::Step(StepParams::GD05aUseOnce {
            tag: -205_006 | -206_002
        })
    )));
    let by: Vec<HookParams> = params("wp_gd05_a_destroyed_by_effect");
    assert!(
        by.contains(&HookParams::Cond(CondParams::GD05aDestroyedByEffect {
            by_you: true
        }))
    );
    let players: Vec<P> = params("wp_eb01_a_look_own_top")
        .iter()
        .map(|p| match p {
            HookParams::Step(StepParams::EB01aLookOwnTop { player, var }) => {
                assert!(v.var_name(*var).starts_with("looked_"));
                *player
            }
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(players, [P::Active, P::Standby]);
    assert_eq!(
        params("wp_gd03_a_ap_per"),
        [HookParams::Value(ValueParams::GD03aApPer { per: 4 })]
    );
    for p in params("wp_gd04_a_has_partner_in_trash") {
        let HookParams::Filter(FilterParams::GD04aHasPartnerInTrash { r#trait }) = p else {
            panic!("{p:?}");
        };
        assert_eq!(v.str(r#trait), "UN");
    }
    for p in params("wp_gd03_b_cond_holds") {
        let HookParams::Filter(FilterParams::GD03bCondHolds { cond }) = p else {
            panic!("{p:?}");
        };
        assert!(matches!(v.node(cond.node()), Node::Cmp(_)));
    }
}

#[test]
fn a_hook_with_a_wrong_parameter_is_refused() {
    let i = first_node("CustomCond");
    let name = raw()["nodes"][i]["name"].as_str().expect("name").to_owned();
    let j = raw()["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .position(|n| n["name"] == "wp_gd05_a_once_free")
        .expect("GD05-006 hook");
    let err = load_modified(|ir| ir["nodes"][j]["params"] = json!([["tag", "x"]])).err();
    assert!(err.is_some(), "a str tag must not load ({name})");
    let err = load_modified(|ir| ir["nodes"][j]["params"] = json!([]))
        .err()
        .expect("refused");
    assert!(
        matches!(err, IrError::Malformed { ref detail, .. } if detail.contains("missing parameter \"tag\"")),
        "{err}"
    );
}

#[test]
fn lookups_resolve_engine_names_and_handles() {
    let t = tables();
    let v = t.view();
    for name in [
        "__todeck", "__play", "it", "card", "pilot", "unit", "looked", "t", "__loop0", "",
    ] {
        let id = v
            .var_id(name)
            .unwrap_or_else(|| panic!("{name:?} not in var_names"));
        assert_eq!(v.var_name(id), name);
    }
    assert_eq!(v.var_id("no such var"), None);
    let s = v.str_id("GD01-001").expect("a card number");
    assert_eq!(v.str(s), "GD01-001");
    let draw = crate::contracts::NodeId(u32::try_from(first_node("Draw")).expect("fits"));
    assert!(
        v.filter_id(draw).is_none() && v.cond_id(draw).is_none() && v.continuous_id(draw).is_none()
    );
    let kind = crate::contracts::NodeId(u32::try_from(first_node("IsKind")).expect("fits"));
    assert!(v.filter_id(kind).is_some());
    let stat = crate::contracts::NodeId(u32::try_from(first_node("StatMod")).expect("fits"));
    let c = v.continuous_id(stat).expect("a Continuous");
    assert!(matches!(
        v.cont_of(c),
        ContRef::StatMod { .. } | ContRef::Node(_)
    ));
}
