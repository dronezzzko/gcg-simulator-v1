//! The registry-only parts of the policy against Python (`registry.ndjson` of
//! `oracle/export_policy_vectors.py`): the choice polarity of every `Choose` instruction
//! with and without the rule table, the battle-only rule flag and the class of the first step
//! (`ai/targeting.py`), and per card definition `card_worth` (`ai/values.py:36-47`) and
//! `command_harms_enemy_units` (`ai/targeting.py:96-112`).

mod policy_support;

use gcg_ai::targeting::{
    REF_KINDS, command_harms_enemy_units, first_step, node_ref, polarity_at,
    rule_lasts_this_battle_at, rule_polarity_of,
};
use gcg_ai::values::card_worth;
use gcg_data::DefId;
use gcg_effects::{CLASSES, ContentId, Node, NodeKind, RuleKind};
use policy_support::{Mismatches, f, records, registry, same, same_floats};

#[test]
fn choice_polarity_matches_python_at_every_choose() {
    let reg = registry();
    let recs = records("registry.ndjson");
    let mut m = Mismatches::default();
    let mut chooses = 0;
    for r in recs.iter().filter(|r| r["t"] == "choose") {
        chooses += 1;
        let cid = r["program"].as_str().unwrap();
        let id = format!("{cid}:{}", r["pc"]);
        let Some(program) = ContentId::from_hex(cid).and_then(|c| reg.program_by_content_id(&c))
        else {
            m.check(&id, "program", Err("no such program in the IR".into()));
            continue;
        };
        let instrs = &reg.program(program).instrs;
        let pc = usize::try_from(r["pc"].as_u64().unwrap()).unwrap();
        assert!(matches!(reg.node(instrs[pc]), Node::Choose(_)), "{id}");
        let pol = |table| i64::from(polarity_at(&reg, instrs, pc, table).value());
        m.check(
            &id,
            "polarity",
            same(pol(false), r["polarity"].as_i64().unwrap()),
        );
        m.check(
            &id,
            "polarity with the rule table",
            same(pol(true), r["polarity_table"].as_i64().unwrap()),
        );
        m.check(
            &id,
            "lasts this battle",
            same(
                rule_lasts_this_battle_at(&reg, instrs, pc),
                r["battle"].as_bool().unwrap(),
            ),
        );
        let step = first_step(&reg, instrs, pc).map(|i| reg.node(instrs[i]).kind().py_name());
        m.check(&id, "first step", same(step, r["first_step"].as_str()));
    }
    let counts = recs.iter().find(|r| r["t"] == "counts").expect("counts");
    assert_eq!(chooses, counts["chooses"].as_u64().unwrap());
    m.assert_none(4 * 500);
}

#[test]
fn card_worth_and_removal_match_python_for_every_definition() {
    let reg = registry();
    let db = reg.db();
    let recs = records("registry.ndjson");
    let mut m = Mismatches::default();
    let mut cards = 0;
    for r in recs.iter().filter(|r| r["t"] == "card") {
        cards += 1;
        let def = DefId(u16::try_from(r["def_id"].as_u64().unwrap()).unwrap());
        let cd = db.by_id(def);
        let id = format!("{} ({})", cd.card_number, def.0);
        m.check(
            &id,
            "card number",
            same(&*cd.card_number, r["card_number"].as_str().unwrap()),
        );
        m.check(
            &id,
            "card_worth",
            same_floats(&[card_worth(cd)], &[f(&r["worth"])]),
        );
        m.check(
            &id,
            "command_harms_enemy_units",
            same(
                command_harms_enemy_units(&reg, def),
                r["harms"].as_bool().unwrap(),
            ),
        );
    }
    let counts = recs.iter().find(|r| r["t"] == "counts").expect("counts");
    assert_eq!(cards, counts["cards"].as_u64().unwrap());
    assert_eq!(cards, db.len() as u64);
    m.assert_none(3 * 1291);
}

/// `command_harms_enemy_units` reads the Command's program; Python would index
/// `R.programs[-1]` for a Command without one, which the shipped registry never has.
#[test]
fn every_command_ability_has_a_program() {
    let reg = registry();
    for i in 0..reg.db().len() {
        let def = DefId(u16::try_from(i).unwrap());
        if let Some(aid) = reg.card(def).command_aid {
            assert!(
                reg.ability(aid).program.is_some(),
                "{} has a Command without a program",
                reg.db().by_id(def).card_number
            );
        }
    }
}

/// `node_ref` reads exactly the DSL classes with a `ref` field (`getattr(later, "ref", None)`,
/// `ai/targeting.py:132`), and every instruction of the registry with a mandatory `ref`
/// yields one.
#[test]
fn node_ref_covers_every_class_with_a_ref_field() {
    let mut with_ref: Vec<&str> = CLASSES
        .iter()
        .filter(|c| c.fields.iter().any(|fl| fl.name == "ref"))
        .map(|c| c.name)
        .collect();
    let mut listed = REF_KINDS.to_vec();
    with_ref.sort();
    listed.sort();
    assert_eq!(listed, with_ref);
    let reg = registry();
    let recs = records("registry.ndjson");
    let mut seen = 0;
    for r in recs.iter().filter(|r| r["t"] == "choose") {
        let cid = ContentId::from_hex(r["program"].as_str().unwrap()).unwrap();
        let program = reg.program_by_content_id(&cid).unwrap();
        for &ins in &reg.program(program).instrs {
            let node = reg.node(ins);
            let kind: NodeKind = node.kind();
            let schema = &CLASSES[kind as usize];
            let field = schema.fields.iter().find(|fl| fl.name == "ref");
            match field {
                Some(fl) if fl.default_repr.is_none() => {
                    assert!(node_ref(node).is_some(), "{} has a ref", kind.py_name());
                }
                Some(_) => {}
                None => assert!(node_ref(node).is_none(), "{} has no ref", kind.py_name()),
            }
            seen += 1;
        }
    }
    assert!(seen > 1000, "only {seen} instructions checked");
}

/// `RULE_POLARITY` (`ai/targeting.py:22-52`) covers every rule kind, with the 10 harmful and
/// the 3 unknown ones.
#[test]
fn the_rule_table_covers_every_rule_kind() {
    use gcg_ai::targeting::Polarity;
    let count = |p| {
        RuleKind::ALL
            .iter()
            .filter(|&&k| rule_polarity_of(k) == p)
            .count()
    };
    assert_eq!(count(Polarity::Harms), 10);
    assert_eq!(count(Polarity::Unknown), 3);
    assert_eq!(count(Polarity::Helps), RuleKind::ALL.len() - 13);
}
