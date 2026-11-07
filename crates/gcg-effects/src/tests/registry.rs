//! The public `Registry` API on the embedded IR: `embedded` is loaded once over the embedded
//! card data, every lookup agrees with the decoded tables that the other tests check against
//! Python, `golden_json` is the golden writer's record, and `with_fragment` applies a fragment
//! with a new generation.

use std::fs;
use std::sync::Arc;

use gcg_core::pyjson::dumps;
use gcg_data::{CardDb, DefId, ImplementationStatus};

use super::{repo_root, tables};
use crate::contracts::{
    AbilityId, ContRef, ContinuousId, KeywordProgram, NodeId, ProgramId, Registry, StrId,
    SystemProgram, VarId,
};
use crate::dsl::NodeKind;
use crate::golden::GOLDEN_DUMP;

fn def(i: usize) -> DefId {
    DefId(u16::try_from(i).expect("def ids fit u16"))
}

#[test]
fn embedded_is_loaded_once_over_the_embedded_card_data() {
    let a = Registry::embedded();
    let b = Registry::embedded();
    assert!(Arc::ptr_eq(&a, &b));
    assert_eq!(a.generation(), b.generation());
    assert!(std::ptr::eq(a.db(), CardDb::embedded()));
    assert_eq!(a.db().len(), tables().cards.len());
}

#[test]
fn node_and_program_lookups_agree_with_the_tables() {
    let reg = Registry::embedded();
    let t = tables();
    let v = t.view();
    for (i, node) in t.nodes.iter().enumerate() {
        let id = NodeId(u32::try_from(i).expect("fits"));
        assert_eq!(reg.node(id), node);
        assert_eq!(reg.node_content_id(id), t.node_content_ids[i]);
        assert_eq!(reg.repr(id), v.repr(id));
        let kind = node.kind();
        assert_eq!(
            reg.filter_id(id).map(|f| f.node()),
            NodeKind::FILTER.contains(&kind).then_some(id)
        );
        assert_eq!(
            reg.cond_id(id).map(|c| c.node()),
            NodeKind::COND.contains(&kind).then_some(id)
        );
        assert_eq!(
            reg.continuous_id(id).map(ContinuousId::node),
            NodeKind::CONTINUOUS.contains(&kind).then_some(id)
        );
    }
    let past = NodeId(u32::try_from(t.nodes.len()).expect("fits"));
    assert_eq!(reg.filter_id(past), None);
    for (i, p) in t.programs.iter().enumerate() {
        let id = ProgramId(u16::try_from(i).expect("fits"));
        assert_eq!(reg.program(id), p);
        assert_eq!(reg.program_by_content_id(&p.content_id), Some(id));
    }
    for (kw, id) in KeywordProgram::ALL.into_iter().zip([0u16, 1, 2]) {
        assert_eq!(reg.keyword_program(kw), ProgramId(id));
    }
    for (s, id) in SystemProgram::ALL.into_iter().zip([709u16, 710, 711]) {
        assert_eq!(reg.system_program(s), ProgramId(id));
    }
    for (n, params) in t.hook_params.iter() {
        assert_eq!(reg.hook_params(*n), Some(params));
    }
    assert_eq!(reg.hook_params(NodeId(0)), v.hook_params(NodeId(0)));
    for (r, text) in t.rule_mod_reprs.iter() {
        assert_eq!(reg.rule_mod_repr(*r), &**text);
        assert_eq!(reg.repr(r.node()), &**text);
    }
}

#[test]
fn card_ability_and_table_lookups_agree_with_the_tables() {
    let reg = Registry::embedded();
    let t = tables();
    let v = t.view();
    for (i, a) in t.abilities.iter().enumerate() {
        let aid = AbilityId(u16::try_from(i).expect("fits"));
        assert_eq!(reg.ability(aid), a);
        assert_eq!(a.aid, aid);
    }
    for (i, c) in t.cards.iter().enumerate() {
        assert_eq!(reg.card(def(i)), c);
        assert_eq!(reg.events_of(def(i)), c.events);
        assert_eq!(reg.aliases(def(i)), v.aliases(def(i)));
        assert_eq!(reg.unimplemented_error(def(i)), None);
        assert_eq!(reg.golden_json(def(i)), t.golden_json(def(i)));
    }
    for (i, s) in t.strings.iter().enumerate() {
        let id = StrId(u32::try_from(i).expect("fits"));
        assert_eq!(reg.str(id), &**s);
        assert_eq!(reg.str_id(s), Some(id));
    }
    for (i, s) in t.var_names.iter().enumerate() {
        let id = VarId(u16::try_from(i).expect("fits"));
        assert_eq!(reg.var_name(id), &**s);
        assert_eq!(reg.var_id(s), Some(id));
    }
    assert_eq!(reg.str_id("no such string"), None);
    assert_eq!(reg.var_id("no_such_var"), None);
    assert_eq!(reg.always_events(), t.always_events);
    assert!(!reg.hp_reduction_possible());
    assert!(
        t.filter_sets.is_empty(),
        "the IR has no ApplyPlayer filter tuple"
    );
    assert_eq!(reg.filters_key(&[]), None);
}

#[test]
fn continuous_effects_round_trip_through_their_content_ids() {
    let reg = Registry::embedded();
    let t = tables();
    let v = t.view();
    assert_eq!(t.continuous.len(), 120);
    for (h, c) in t.continuous.iter() {
        let cid = t.node_content_ids[h.node().0 as usize];
        assert_eq!(reg.cont_of(*h), *c);
        assert_eq!(reg.cont_by_content_id(&cid), Some(*c));
        assert_eq!(c.content_id(&reg), cid);
        assert_eq!(c.repr(&reg), v.repr(h.node()));
        assert_eq!(reg.cont_node(*c), v.cont_node(*c));
    }
    // A runtime value that no IR node spells has no registry entry; its content id is the
    // repr's (runner/digest.py:26-27).
    let frozen = ContRef::StatMod { ap: -3, hp: 2 };
    let cid = frozen.content_id(&reg);
    assert_eq!(frozen.repr(&reg), "StatMod(ap=-3, hp=2)");
    assert_eq!(reg.cont_by_content_id(&cid), None);
}

#[test]
fn golden_json_is_the_golden_record() {
    let reg = Registry::embedded();
    let gd = fs::read_to_string(repo_root().join("tests/effects/golden/GD01.json"))
        .expect("golden file");
    let file: serde_json::Value = serde_json::from_str(&gd).expect("JSON");
    let number = "GD01-001";
    let d = reg.db().get(number).expect("a card").def_id;
    let ours: serde_json::Value =
        serde_json::from_str(&dumps(&reg.golden_json(d), GOLDEN_DUMP)).expect("JSON");
    assert_eq!(ours, file[number]);
}

#[test]
fn with_fragment_gives_a_new_generation_and_keeps_the_card_data() {
    let reg = Registry::embedded();
    let path = repo_root().join("crates/gcg-effects/ir/fragments/GD01-064.json");
    let fx: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(path).expect("fixture")).expect("JSON");
    let over = reg.with_fragment(&fx["fragment"]).expect("applies");
    assert_ne!(over.generation(), reg.generation());
    assert!(std::ptr::eq(over.db(), reg.db()));
    assert!(over.hp_reduction_possible());
    let d = reg.db().get("GD01-064").expect("a card").def_id;
    let ours: serde_json::Value =
        serde_json::from_str(&dumps(&over.golden_json(d), GOLDEN_DUMP)).expect("JSON");
    assert_eq!(ours, fx["golden"]);
    assert_ne!(over.golden_json(d), reg.golden_json(d));
    assert_eq!(over.card(d).own, [AbilityId(1447)].into());
    let again = reg
        .with_fragment(&fx["fragment"])
        .expect("applies again to the base");
    assert_ne!(again.generation(), over.generation());
}
