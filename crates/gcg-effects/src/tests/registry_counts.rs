//! PLAN §3.4 test 4 (T-043): the registry counts of Inv 04 §2, the registry-global tables
//! and the history kinds that `HappenedThisTurn` reads.

use gcg_data::DefId;

use super::{raw, tables};
use crate::contracts::{EvSet, KeywordProgram, ProgramId, ScriptSource, SystemProgram};
use crate::dsl::{Ev, Node};

fn count(name: &str) -> i64 {
    raw()["counts"][name]
        .as_i64()
        .unwrap_or_else(|| panic!("counts.{name}"))
}

#[test]
fn counts_match_the_inventory_and_the_ir() {
    let t = tables();
    assert_eq!(t.cards.len(), 1153);
    assert_eq!(t.cards.iter().filter(|c| c.script.is_none()).count(), 0);
    let by_source = |s: ScriptSource| t.cards.iter().filter(|c| c.source == s).count();
    assert_eq!(by_source(ScriptSource::Binding), 302);
    assert_eq!(by_source(ScriptSource::Compiled), 538);
    assert_eq!(by_source(ScriptSource::Vanilla), 313);
    assert_eq!(t.abilities.len(), 1260);
    assert_eq!(t.programs.len(), 623 + 3);
    let build_instrs: usize = t.programs[..623].iter().map(|p| p.instrs.len()).sum();
    assert_eq!(build_instrs, 1830);
    assert_eq!(t.aliases.len(), 3);
    assert_eq!(
        t.cards.iter().filter(|c| c.events != EvSet::EMPTY).count(),
        480
    );
    assert!(!t.hp_reduction_possible);
    for (name, n) in [
        ("defs", t.cards.len()),
        ("abilities", t.abilities.len()),
        ("build_programs", 623),
        ("system_programs", 3),
        ("build_instrs", build_instrs),
        ("instrs", t.programs.iter().map(|p| p.instrs.len()).sum()),
        ("nodes", t.nodes.len()),
        ("strings", t.strings.len()),
        ("var_names", t.var_names.len()),
        ("binding", by_source(ScriptSource::Binding)),
        ("compiled", by_source(ScriptSource::Compiled)),
        ("vanilla", by_source(ScriptSource::Vanilla)),
    ] {
        assert_eq!(usize::try_from(count(name)).ok(), Some(n), "counts.{name}");
    }
}

#[test]
fn always_events_and_aliases_match_the_inventory() {
    let t = tables();
    let v = t.view();
    let always = EvSet::EMPTY.with(Ev::DestroysByBattle).with(Ev::TurnEnd);
    assert_eq!(t.always_events, always);
    let alias =
        |def: u16| -> Vec<&str> { v.aliases(DefId(def)).iter().map(|s| v.str(*s)).collect() };
    let named: Vec<(&str, Vec<&str>)> = t
        .aliases
        .iter()
        .map(|(d, _)| (&*t.card_texts[usize::from(d.0)].card_number, alias(d.0)))
        .collect();
    assert_eq!(
        named,
        [
            ("GD02-098", vec!["Char Aznable"]),
            ("ST12-011", vec!["Zechs Merquise"]),
            ("ST12-012", vec!["Marida Cruz"]),
        ]
    );
    assert!(v.aliases(DefId(0)).is_empty());
}

#[test]
fn keyword_and_system_programs_have_fixed_ids_and_labels() {
    let t = tables();
    let v = t.view();
    let labels = ["keyword:Repair", "keyword:Breach", "keyword:Support"];
    for (kw, (id, label)) in KeywordProgram::ALL
        .into_iter()
        .zip([0u16, 1, 2].into_iter().zip(labels))
    {
        assert_eq!(v.keyword_program(kw), ProgramId(id));
        assert_eq!(v.str(v.program(ProgramId(id)).label), label);
    }
    for (s, id) in SystemProgram::ALL.into_iter().zip([623u16, 624, 625]) {
        assert_eq!(v.system_program(s), ProgramId(id));
        assert_eq!(v.str(v.program(ProgramId(id)).label), s.label());
    }
    assert_eq!(
        v.repr(v.program(ProgramId(625)).instrs[0]),
        "Discard(count=Sum(terms=(HandSize(player=<P.YOU: 'you'>), -10)), \
         player=<P.YOU: 'you'>, chooser=None, random=False, filters=(), var='discarded')"
    );
}

/// Every `HappenedThisTurn.kind` is a kind that some `core.record` call site writes (the
/// IR's `history_kinds`, scanned like `gcg-engine`'s `HistKind`); none is known to be
/// recorded nowhere.
#[test]
fn happened_this_turn_kinds_are_recorded_kinds() {
    let t = tables();
    let recorded: Vec<&str> = raw()["history_kinds"]
        .as_array()
        .expect("history_kinds")
        .iter()
        .map(|k| k.as_str().expect("kind"))
        .collect();
    assert_eq!(recorded.len(), 22);
    let mut kinds: Vec<&str> = t
        .nodes
        .iter()
        .filter_map(|n| match n {
            Node::HappenedThisTurn(h) => Some(&*t.strings[h.kind.0 as usize]),
            _ => None,
        })
        .collect();
    kinds.sort();
    kinds.dedup();
    for k in &kinds {
        assert!(
            recorded.contains(k),
            "HappenedThisTurn kind {k:?} is never recorded"
        );
    }
    assert_eq!(
        kinds,
        [
            "command_activated",
            "cost_paid",
            "deployed",
            "destroyed",
            "destroys_by_battle_paired",
            "discard",
            "paired",
            "resource_set_active"
        ]
    );
}

#[test]
fn every_custom_node_has_typed_parameters() {
    let t = tables();
    let customs = t
        .nodes
        .iter()
        .filter(|n| {
            matches!(
                n,
                Node::CustomStep(_)
                    | Node::CustomCond(_)
                    | Node::CustomFilter(_)
                    | Node::CustomValue(_)
            )
        })
        .count();
    assert_eq!(t.hook_params.len(), customs);
    assert!(t.hook_params.windows(2).all(|w| w[0].0 < w[1].0));
}
