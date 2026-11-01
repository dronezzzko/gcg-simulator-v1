//! PLAN §3.4 test 2 (T-041): `sha256(repr(x))[:16]` computed by the Rust `repr` equals
//! Python's for every IR node, all 626 programs, every filter set and every continuous effect
//! (`runner/digest.py:26-27,40-48`).

use super::tables;
use crate::contracts::{ContRef, ContentId, NodeId, ProgramId};
use crate::pyrepr::content_id_of;

#[test]
fn every_node_content_id_equals_pythons() {
    let t = tables();
    let v = t.view();
    let mut wrong = Vec::new();
    for (i, want) in t.node_content_ids.iter().enumerate() {
        let id = NodeId(u32::try_from(i).expect("node ids fit u32"));
        if content_id_of(&v.repr(id)) != *want {
            wrong.push(i);
        }
    }
    assert!(
        wrong.is_empty(),
        "{} nodes differ, first {:?}",
        wrong.len(),
        wrong.first()
    );
    assert_eq!(t.node_content_ids.len(), 4477);
}

#[test]
fn every_program_content_id_equals_pythons() {
    let t = tables();
    let v = t.view();
    assert_eq!(t.programs.len(), 626);
    for (i, p) in t.programs.iter().enumerate() {
        let id = ProgramId(u16::try_from(i).expect("program ids fit u16"));
        assert_eq!(
            content_id_of(&v.program_repr(id)),
            p.content_id,
            "program {i} ({})",
            v.str(p.label)
        );
        assert_eq!(v.program_by_content_id(&p.content_id), Some(id));
    }
    let repair = v.program_repr(ProgramId(0));
    assert_eq!(
        repair,
        "Program(instrs=(Recover(ref=This(), amount=KwAmount(keyword=<Kw.REPAIR: 'Repair'>, \
         ref=This())),), label='keyword:Repair')"
    );
    assert_eq!(t.programs[0].content_id.as_str(), "b3df0ca881a27647");
}

#[test]
fn every_filter_set_content_id_equals_pythons() {
    let t = tables();
    let v = t.view();
    let cx = v.repr_cx();
    for (i, f) in t.filter_sets.iter().enumerate() {
        assert_eq!(
            content_id_of(&cx.repr_of(&f.filters)),
            f.content_id,
            "filter set {i}"
        );
        let id = crate::contracts::FilterSetId(u16::try_from(i).expect("fits u16"));
        assert_eq!(v.filters_key(&f.filters), Some(id));
        assert_eq!(v.filter_set_by_content_id(&f.content_id), Some(id));
    }
}

#[test]
fn continuous_effects_print_and_hash_like_their_nodes() {
    let t = tables();
    let v = t.view();
    assert_eq!(t.continuous.len(), 110);
    let mut values = 0;
    for &(n, c) in &*t.continuous {
        assert_eq!(v.cont_of(n), c);
        assert_eq!(v.cont_repr(c), v.repr(n.node()));
        assert_eq!(v.cont_content_id(c), v.node_content_id(n.node()));
        assert_eq!(v.cont_by_content_id(&v.node_content_id(n.node())), Some(c));
        if !matches!(c, ContRef::Node(_)) {
            values += 1;
        }
    }
    assert_eq!(
        values, 29,
        "12 StatMod + 12 KeywordGrant + 5 AbilityGrant value variants"
    );
}

#[test]
fn rule_mod_reprs_equal_the_rust_repr() {
    let t = tables();
    let v = t.view();
    assert_eq!(t.rule_mod_reprs.len(), 68);
    for (r, text) in &*t.rule_mod_reprs {
        assert_eq!(v.repr(r.node()), **text);
        assert_eq!(v.rule_mod_repr(*r), &**text);
    }
}

#[test]
fn content_ids_round_trip_as_text() {
    for cid in &*tables().node_content_ids {
        assert_eq!(ContentId::from_hex(cid.as_str()), Some(*cid));
    }
}
