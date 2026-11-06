//! `Registry::decode_inline`: the side-table nodes of trace snapshots (PLAN §4.8; the
//! `node_json` form of `oracle/traces.py`, nested nodes inline) resolve to the arena node
//! with their CPython `repr`, runtime values decode detached, and malformed nodes are refused.

use serde_json::{Value, json};

use super::{raw, tables};
use crate::contracts::{InlineNode, IrError, NodeId, Registry, ValueRef};
use crate::dsl::{Node, NodeKind};

/// IR node `i` in the inline form: every `{"$": id}` reference replaced by the node itself.
/// The IR writes every field in declaration order with enums as their values (PLAN §3.3), as
/// `node_json` does.
fn inline(i: usize) -> Value {
    fn expand(v: &Value, nodes: &[Value]) -> Value {
        match v {
            Value::Object(m) if m.len() == 1 && m.contains_key("$") => {
                let id = m["$"].as_u64().expect("a node id");
                expand(&nodes[usize::try_from(id).expect("fits")], nodes)
            }
            Value::Object(m) => Value::Object(
                m.iter()
                    .map(|(k, x)| (k.clone(), expand(x, nodes)))
                    .collect(),
            ),
            Value::Array(a) => Value::Array(a.iter().map(|x| expand(x, nodes)).collect()),
            other => other.clone(),
        }
    }
    let nodes = raw()["nodes"].as_array().expect("nodes");
    expand(&nodes[i], nodes)
}

fn node_id(i: usize) -> NodeId {
    NodeId(u32::try_from(i).expect("fits"))
}

#[test]
fn side_table_nodes_and_one_node_per_class_resolve_to_themselves() {
    let reg = Registry::embedded();
    let mut classes = Vec::new();
    let mut side = 0;
    for (i, n) in tables().nodes.iter().enumerate() {
        let k = n.kind();
        let side_table = NodeKind::CONTINUOUS.contains(&k) || NodeKind::FILTER.contains(&k);
        if side_table || !classes.contains(&k) {
            if !classes.contains(&k) {
                classes.push(k);
            }
            side += usize::from(side_table);
            assert_eq!(
                reg.decode_inline(&inline(i)),
                Ok(InlineNode::Arena(node_id(i))),
                "node {i} ({})",
                k.py_name()
            );
        }
    }
    assert_eq!(classes.len(), 130, "every class the IR uses");
    assert!(side > 110, "{side} continuous and filter nodes");
}

#[test]
fn runtime_values_decode_detached() {
    let reg = Registry::embedded();
    let frozen = json!({"_": "StatMod", "ap": -3, "hp": 2});
    assert_eq!(
        reg.decode_inline(&frozen),
        Ok(InlineNode::Detached(Node::StatMod(crate::dsl::StatMod {
            ap: ValueRef::Lit(-3),
            hp: ValueRef::Lit(2),
        })))
    );
    let grant = json!({"_": "AbilityGrant", "card_number": "GD01-001", "ability_index": 7});
    assert_eq!(
        reg.decode_inline(&grant),
        Ok(InlineNode::Detached(Node::AbilityGrant(
            crate::dsl::AbilityGrant {
                card_number: reg.str_id("GD01-001").expect("a card number"),
                ability_index: 7,
            }
        )))
    );
    // A value-shaped node the IR does hold resolves to it, whatever its origin.
    let (h, _) = tables()
        .continuous
        .iter()
        .find(|(h, _)| matches!(reg.node(h.node()), Node::StatMod(m) if matches!(m.ap, ValueRef::Lit(_))))
        .expect("an IR StatMod literal");
    let i = h.node().0 as usize;
    assert_eq!(
        reg.decode_inline(&inline(i)),
        Ok(InlineNode::Arena(h.node()))
    );
}

#[test]
fn malformed_inline_nodes_are_refused() {
    let reg = Registry::embedded();
    let malformed = |v: Value| match reg.decode_inline(&v) {
        Err(IrError::Malformed { path, detail }) => format!("{path}: {detail}"),
        other => panic!("{v}: {other:?}"),
    };
    assert!(matches!(
        reg.decode_inline(&json!({"_": "NoSuchClass"})),
        Err(IrError::UnknownClass { .. })
    ));
    assert!(malformed(json!({"ap": 1, "hp": 0})).contains("missing class name"));
    assert!(malformed(json!({"_": "StatMod", "ap": 1})).starts_with("$"));
    assert!(
        malformed(json!({"_": "AbilityGrant", "card_number": "ZZ99-999", "ability_index": 0}))
            .contains("not in the string table")
    );
    // An IR reference is not an inline node.
    let rule = tables()
        .nodes
        .iter()
        .position(|n| matches!(n, Node::RuleGrant(_)))
        .expect("a RuleGrant");
    let mut by_ref = inline(rule);
    by_ref["rule"] = json!({"$": 3});
    assert!(malformed(by_ref).starts_with("$.rule"));
    // A nested node the registry does not hold.
    let mut nested = inline(rule);
    nested["rule"]["amount"] = json!(12_345);
    let detail = malformed(nested);
    assert!(
        detail.starts_with("$.rule") && detail.contains("does not hold"),
        "{detail}"
    );
}
