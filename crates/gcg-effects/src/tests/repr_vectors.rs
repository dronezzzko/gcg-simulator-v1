//! PLAN §3.4 test 5 (T-044): CPython `repr` vectors. `port/oracle/export_ir.py` builds two
//! synthetic instances of every DSL and instruction class with every field set to a
//! non-default value (strings with `'`, `"`, both, `Ⅱ`, `①`, a backslash and control
//! characters), programs, the runtime continuous values (`StatMod`, `KeywordGrant`,
//! `AbilityGrant` as `_frozen` and `_h_delayed` build them) and filter tuples, each with
//! Python's `repr`. They live in their own arena, so the registry's tables stay unchanged.

use serde_json::Value;

use super::raw;
use crate::codec::{Field, ReprCx};
use crate::contracts::{ContRef, FilterId, NodeId, ProgramId, StrId};
use crate::dsl::{Kw, NodeKind};
use crate::ir::{ArenaTables, decode_arena};
use crate::pyrepr::repr_str;
use crate::view::View;

fn vectors() -> (&'static Value, ArenaTables) {
    let doc = &raw()["repr_vectors"];
    (doc, decode_arena(doc).unwrap_or_else(|e| panic!("{e}")))
}

fn node_id(v: &Value) -> NodeId {
    NodeId(u32::try_from(v.as_u64().expect("node id")).expect("fits u32"))
}

fn view(a: &ArenaTables) -> View<'_> {
    static NO_PROGRAMS: [ProgramId; 3] = [ProgramId(0); 3];
    View {
        strings: &a.strings,
        var_names: &a.var_names,
        nodes: &a.nodes,
        node_content_ids: &[],
        programs: &[],
        keyword_programs: &NO_PROGRAMS,
        system_programs: &NO_PROGRAMS,
        abilities: &[],
        cards: &[],
        continuous: &[],
        filter_sets: &[],
        rule_mod_reprs: &[],
        aliases: &[],
        hook_params: &[],
    }
}

#[test]
fn every_class_prints_like_cpython() {
    let (doc, a) = vectors();
    let cx = ReprCx {
        nodes: &a.nodes,
        strings: &a.strings,
        var_names: &a.var_names,
    };
    let cases = doc["cases"].as_array().expect("cases");
    let mut kinds: Vec<NodeKind> = Vec::new();
    for case in cases {
        let id = node_id(&case["node"]);
        let want = case["repr"].as_str().expect("repr");
        assert_eq!(cx.repr_of(&id), want);
        kinds.push(a.kinds[id.0 as usize]);
    }
    kinds.sort();
    kinds.dedup();
    assert_eq!(
        kinds,
        NodeKind::ALL,
        "every DSL and instruction class has a vector"
    );
    let texts: Vec<&str> = cases.iter().filter_map(|c| c["repr"].as_str()).collect();
    for needle in [
        "\"it's\"",
        "'say \"hi\"'",
        "'both \\' and \"'",
        "Ⅱ",
        "①",
        "\\\\",
        "\\t",
        "\\x00",
        "\\x7f",
        "(",
        ",)",
    ] {
        assert!(
            texts.iter().any(|t| t.contains(needle)),
            "no vector exercises {needle}"
        );
    }
}

#[test]
fn programs_print_like_cpython() {
    let (doc, a) = vectors();
    let cx = ReprCx {
        nodes: &a.nodes,
        strings: &a.strings,
        var_names: &a.var_names,
    };
    for p in doc["programs"].as_array().expect("programs") {
        let instrs: Box<[NodeId]> = p["instrs"]
            .as_array()
            .expect("instrs")
            .iter()
            .map(node_id)
            .collect();
        let mut text = String::from("Program(instrs=");
        instrs.repr(&cx, &mut text);
        text.push_str(", label=");
        text.push_str(&repr_str(p["label"].as_str().expect("label")));
        text.push(')');
        assert_eq!(text, p["repr"].as_str().expect("repr"));
    }
}

fn cont_ref(v: &Value, a: &ArenaTables) -> ContRef {
    let int = |x: &Value| i32::try_from(x.as_i64().expect("int")).expect("fits i32");
    if let Some([ap, hp]) = v["StatMod"].as_array().map(Vec::as_slice) {
        return ContRef::StatMod {
            ap: int(ap),
            hp: int(hp),
        };
    }
    if let Some([kw, n]) = v["KeywordGrant"].as_array().map(Vec::as_slice) {
        let kw = Kw::from_value(kw.as_str().expect("kw")).expect("a Kw value");
        return ContRef::KeywordGrant { kw, amount: int(n) };
    }
    let Some([card, index]) = v["AbilityGrant"].as_array().map(Vec::as_slice) else {
        panic!("unexpected ContRef {v}");
    };
    let card = card.as_str().expect("card number");
    let card = a
        .strings
        .iter()
        .position(|s| &**s == card)
        .expect("in the vector strings");
    ContRef::AbilityGrant {
        card: StrId(u32::try_from(card).expect("fits u32")),
        index: int(index),
    }
}

#[test]
fn runtime_continuous_values_print_like_cpython() {
    let (doc, a) = vectors();
    let v = view(&a);
    let conts = doc["conts"].as_array().expect("conts");
    assert_eq!(conts.len(), 6);
    for c in conts {
        let cont = cont_ref(&c["cont"], &a);
        let want = c["repr"].as_str().expect("repr");
        assert_eq!(v.cont_repr(cont), want);
        assert_eq!(v.cont_content_id(cont), crate::pyrepr::content_id_of(want));
    }
}

#[test]
fn filter_tuples_print_like_cpython() {
    let (doc, a) = vectors();
    let cx = ReprCx {
        nodes: &a.nodes,
        strings: &a.strings,
        var_names: &a.var_names,
    };
    for t in doc["filter_tuples"].as_array().expect("filter_tuples") {
        let filters: Box<[FilterId]> = t["filters"]
            .as_array()
            .expect("filters")
            .iter()
            .map(|f| FilterId(node_id(f)))
            .collect();
        assert_eq!(cx.repr_of(&filters), t["repr"].as_str().expect("repr"));
    }
}
