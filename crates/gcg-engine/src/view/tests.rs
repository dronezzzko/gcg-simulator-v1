//! Unit tests of layer L2 on states rebuilt from the committed `views` traces (their
//! Python-recorded values are checked by `tests/view_corpus.rs`): the cache policy of
//! `view.py:232-239`, accessor fallbacks, `same_values`, `resolve_ref`, the location codes and
//! the `node_json` reader.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use gcg_data::DefId;
use gcg_effects::{AbilityId, InlineNode, Kw, Node, RefId, Registry, RuleKind};
use serde_json::Value;

use super::derived::{blank, view_key};
use super::eval::{resolve, resolve_ref, selects};
use super::signature::node_json;
use super::*;
use crate::canon::from_canonical;
use crate::{
    Battle, HistKind, HistoryEvent, PLAYER_TARGET, RuleInst, RuleKey, StateSnapshot, Zone,
};

/// The snapshot of the `index`-th trace of a tier-1 `views` file.
pub(crate) fn corpus_state(file: &str, index: usize) -> GameState {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../port/corpus/v1/tier1/views")
        .join(file);
    let text = fs::read_to_string(&path).expect("a committed views file");
    let snap: Value = text
        .lines()
        .map(|l| serde_json::from_str::<Value>(l).expect("JSON"))
        .filter(|r| r["t"] == "snapshot")
        .nth(index)
        .expect("a snapshot record");
    let snapshot = StateSnapshot {
        state: snap["state"].clone(),
        continuous: snap["continuous"].clone(),
        filter_sets: snap["filter_sets"].clone(),
        rules_checked: snap["rules_checked"].as_i64().expect("rules_checked"),
    };
    from_canonical(&Registry::embedded(), &snapshot).expect("the snapshot loads")
}

/// `views-000` trace 0: p0 BATTLE [0, 14, 17, 13, 18], p1 BATTLE [64, 67], no battle, no
/// lasting effect, 13 history records.
fn state0() -> GameState {
    corpus_state("views-000.ndjson", 0)
}

fn cached(st: &GameState) -> Option<Arc<Derived>> {
    st.cache.borrow().clone()
}

#[test]
fn derived_fills_the_cache_and_touch_is_the_only_invalidation() {
    let mut st = state0();
    assert!(cached(&st).is_none(), "from_canonical starts with no view");
    let a = derived(&st).unwrap();
    assert!(Arc::ptr_eq(&a, &cached(&st).unwrap()));
    assert_eq!(a.key(), view_key(&st));
    assert!(Arc::ptr_eq(&a, &derived(&st).unwrap()), "a cache hit");

    // A view input changed without a touch: the cached view stays (view.py:235).
    st.cards[0].damage += 1;
    assert!(Arc::ptr_eq(&a, &derived(&st).unwrap()));

    let version = st.version;
    st.touch();
    assert_eq!(st.version, version + 1);
    assert!(cached(&st).is_none());
    let b = derived(&st).unwrap();
    assert!(!Arc::ptr_eq(&a, &b));
    assert_eq!(
        signature(&st, &a),
        signature(&st, &b),
        "damage is no view value"
    );
}

/// W-49: under `paranoid`, a cache hit whose view no longer matches the state (a view input
/// written without a touch) is reported (`tests/paranoid.py:59-73`).
#[cfg(feature = "paranoid")]
#[test]
#[should_panic(expected = "paranoid: stale view")]
fn paranoid_reports_a_stale_cached_view() {
    let mut st = state0();
    let ap = ap_of(&st, &derived(&st).unwrap(), 0);
    let db = st.db();
    let other = (0..db.len())
        .map(|d| DefId(u16::try_from(d).unwrap()))
        .find(|&d| db.by_id(d).card_type.is_unit() && db.by_id(d).ap.max(0) != ap)
        .unwrap();
    st.cards[0].def_id = other;
    let _ = derived(&st);
}

#[test]
fn a_key_change_recomputes_without_a_touch() {
    let mut st = state0();
    let a = derived(&st).unwrap();
    // core.record appends history without touching (core.py:142-151).
    st.history.push(HistoryEvent {
        kind: HistKind::Draw,
        player: 0,
        by: 0,
        uid: 1,
        def_id: 0,
        source: NO_ARG,
    });
    let b = derived(&st).unwrap();
    assert!(!Arc::ptr_eq(&a, &b));
    assert_eq!(b.key().n_history, a.key().n_history + 1);
    assert!(Arc::ptr_eq(&b, &cached(&st).unwrap()));

    st.battles.push(Battle {
        attacker: 0,
        attacker_seq: st.cards[0].zone_seq,
        target: PLAYER_TARGET,
        target_seq: 0,
        defender: 1,
        battle_id: 7,
        blocked: false,
        damage_only: false,
        attack_triggers: true,
        ended: false,
    });
    let c = derived(&st).unwrap();
    assert!(!Arc::ptr_eq(&b, &c));
    assert_eq!(c.key().battle, Some((7, PLAYER_TARGET, false)));
    st.battles.last_mut().unwrap().ended = true;
    let d = derived(&st).unwrap();
    assert!(!Arc::ptr_eq(&c, &d), "ended is part of the key");
    st.turn += 1;
    assert!(!Arc::ptr_eq(&d, &derived(&st).unwrap()));
}

#[test]
fn clones_share_the_cached_view_and_its_trigger_index() {
    let st = state0();
    let a = derived(&st).unwrap();
    let mut copy = st.clone();
    assert!(Arc::ptr_eq(&a, &cached(&copy).unwrap()));
    let b = derived(&copy).unwrap();
    assert!(Arc::ptr_eq(&a, &b));
    assert!(a.by_event.get().is_none());
    let idx = trigger_index(&copy, &b);
    assert!(
        std::ptr::eq(a.by_event.get().unwrap(), idx),
        "filled on the shared view"
    );
    copy.touch();
    assert!(cached(&copy).is_none());
    assert!(
        Arc::ptr_eq(&a, &cached(&st).unwrap()),
        "the original keeps its view"
    );
}

#[test]
fn the_empty_view_reads_every_key_as_absent() {
    let st = state0();
    let dv = Derived::empty();
    assert_eq!(dv.len(), 0);
    let reg = st.registry();
    for (i, c) in st.cards.iter().enumerate() {
        let u = Uid::try_from(i).unwrap();
        let cd = st.def_of(u);
        assert_eq!(ap_of(&st, &dv, u), cd.ap.max(0));
        let hp = if c.zone == Zone::Shield {
            1
        } else {
            cd.hp.max(0)
        };
        assert_eq!(hp_of(&st, &dv, u), hp);
        assert_eq!(keywords(&dv, u), gcg_effects::KwAmounts::default());
        assert!(!has_kw(&dv, u, Kw::Repair));
        assert_eq!(kw_amount(&dv, u, Kw::Repair), 0);
        let traits: Vec<&str> = traits_of(&st, &dv, u).iter(reg).collect();
        let printed: Vec<&str> = cd.traits.iter().map(|t| &**t).collect();
        assert_eq!(traits, printed);
        assert!(abilities_of(&dv, u).is_empty());
        assert!(rules_all(&dv, u).is_empty());
        assert!(!is_linked(&dv, u));
        let level = if cd.is_token() { 0 } else { cd.level };
        assert_eq!(play_level(&st, &dv, u), level.max(0));
    }
}

#[test]
fn a_uid_created_after_the_view_is_an_absent_key() {
    let mut st = state0();
    let dv = derived(&st).unwrap();
    let mut card = st.cards[0];
    card.zone = Zone::Battle;
    st.cards.push(card);
    let u = Uid::try_from(st.cards.len() - 1).unwrap();
    assert!(!dv.present(u));
    assert_eq!(ap_of(&st, &dv, u), st.def_of(u).ap.max(0));
    assert_eq!(hp_of(&st, &dv, u), st.def_of(u).hp.max(0));
    assert!(abilities_of(&dv, u).is_empty());
    assert_eq!(rules_of(&dv, u, RuleKind::CantAttack).count(), 0);
    assert_eq!(play_cost(&st, &dv, u).unwrap(), st.def_of(u).cost.max(0));
}

fn rule(reg: &Registry) -> (gcg_effects::RuleModId, gcg_effects::RuleModId) {
    // Two distinct RuleMod nodes of the IR, from RuleGrant effects of constants.
    let mut found = Vec::new();
    for d in 0..reg.db().len() {
        for &aid in reg.card(DefId(u16::try_from(d).unwrap())).own.iter() {
            if let Node::Constant(c) = reg.node(reg.ability(aid).ability.node()) {
                for &e in &c.effects {
                    if let Node::RuleGrant(g) = reg.node(e.node())
                        && !found.contains(&g.rule)
                    {
                        found.push(g.rule);
                    }
                }
            }
        }
        if found.len() >= 2 {
            break;
        }
    }
    (found[0], found[1])
}

fn inst(rule: gcg_effects::RuleModId, source: i32) -> RuleInst {
    RuleInst {
        rule,
        controller: 0,
        source,
        key: RuleKey::Constant {
            aid: AbilityId(0),
            host: 0,
        },
        aux: NO_ARG,
    }
}

#[test]
fn same_values_is_dict_equality_with_presence_and_order() {
    let reg = Registry::embedded();
    let db = reg.db();
    let (r1, r2) = rule(&reg);
    let fresh = |n| blank(n, Some(db), UNREAD_KEY);
    let a = fresh(4);
    // Uids beyond a view are absent keys; `key`, `linked` and `converged` are not compared.
    let mut b = fresh(6);
    b.linked[1] = true;
    b.converged = false;
    assert!(same_values(&a, &b) && same_values(&b, &a));
    // A key with 0 differs from an absent key (cost_mod, view.py:395).
    let mut c = fresh(4);
    c.cost_mod[2] = Some(0);
    assert!(!same_values(&a, &c));
    // A stacking keyword at amount 0 is present (view.py:376-377).
    let mut d = fresh(4);
    let mut e = fresh(4);
    d.set_stats(1, 2, 3, DefId(0)).unwrap();
    e.set_stats(1, 2, 3, DefId(0)).unwrap();
    assert!(same_values(&d, &e));
    d.add_keyword(1, Kw::Repair, 0).unwrap();
    assert!(!same_values(&d, &e));
    e.add_keyword(1, Kw::Repair, 0).unwrap();
    assert!(same_values(&d, &e));
    // Rule lists compare by RuleMod in order; controller, source, key and aux are ignored.
    let (mut f, mut g) = (fresh(4), fresh(4));
    f.push_rule(3, inst(r1, 1), RuleKind::Custom).unwrap();
    f.push_rule(3, inst(r2, 1), RuleKind::Custom).unwrap();
    g.push_rule(3, inst(r1, 9), RuleKind::Custom).unwrap();
    g.push_rule(3, inst(r2, 2), RuleKind::Custom).unwrap();
    assert!(same_values(&f, &g));
    let mut h = fresh(4);
    h.push_rule(3, inst(r2, 1), RuleKind::Custom).unwrap();
    h.push_rule(3, inst(r1, 1), RuleKind::Custom).unwrap();
    assert!(!same_values(&f, &h));
    // Ability lists compare in order.
    let (mut i, mut j) = (fresh(4), fresh(4));
    i.set_abilities(0, &[AbilityId(5), AbilityId(6)]).unwrap();
    j.set_abilities(0, &[AbilityId(6), AbilityId(5)]).unwrap();
    assert!(!same_values(&i, &j));
}

#[test]
fn rule_lists_keep_application_order_across_interleaved_targets() {
    let reg = Registry::embedded();
    let (r1, r2) = rule(&reg);
    let mut dv = blank(3, Some(reg.db()), UNREAD_KEY);
    dv.push_rule(1, inst(r1, 10), RuleKind::CantAttack).unwrap();
    dv.push_rule(2, inst(r2, 20), RuleKind::CantBlock).unwrap();
    dv.push_rule(1, inst(r2, 11), RuleKind::CantBlock).unwrap();
    dv.push_rule(2, inst(r1, 21), RuleKind::CantAttack).unwrap();
    dv.push_rule(1, inst(r1, 12), RuleKind::CantAttack).unwrap();
    let sources = |u| {
        rules_all(&dv, u)
            .iter()
            .map(|r| r.source)
            .collect::<Vec<_>>()
    };
    assert_eq!(sources(1), [10, 11, 12]);
    assert_eq!(sources(2), [20, 21]);
    assert!(sources(0).is_empty());
    let attack: Vec<i32> = rules_of(&dv, 1, RuleKind::CantAttack)
        .map(|r| r.source)
        .collect();
    assert_eq!(attack, [10, 12]);
    assert_eq!(
        dv.rule_kinds_slice(2),
        [RuleKind::CantBlock, RuleKind::CantAttack]
    );
}

#[test]
fn granted_traits_append_without_duplicating_the_current_ones() {
    let reg = Registry::embedded();
    let mut dv = blank(1, Some(reg.db()), UNREAD_KEY);
    let def = (0..reg.db().len())
        .map(|d| DefId(u16::try_from(d).unwrap()))
        .find(|&d| !reg.db().by_id(d).traits.is_empty())
        .unwrap();
    dv.set_stats(0, 1, 1, def).unwrap();
    let a = reg.str_id("Earth Federation").unwrap();
    dv.grant_traits(0, &[a]).unwrap();
    dv.grant_traits(0, &[]).unwrap();
    let (t, granted) = dv.traits_get(0).unwrap();
    assert_eq!(t.def, def);
    assert_eq!(granted, [a]);
}

/// The first `This()` and the first `All(...)` reference among constant scopes.
fn refs(reg: &Registry) -> (RefId, RefId) {
    let (mut this, mut all) = (None, None);
    for d in 0..reg.db().len() {
        for &aid in &reg.card(DefId(u16::try_from(d).unwrap())).own {
            if let Node::Constant(c) = reg.node(reg.ability(aid).ability.node()) {
                match reg.node(c.scope.node()) {
                    Node::This(_) => this = this.or(Some(c.scope)),
                    Node::All(_) => all = all.or(Some(c.scope)),
                    _ => {}
                }
            }
        }
    }
    (this.unwrap(), all.unwrap())
}

#[test]
fn resolve_ref_computes_the_view_only_for_selecting_references() {
    let st = state0();
    let reg = Registry::embedded();
    let (this, all) = refs(&reg);
    assert!(!selects(&st, this) && selects(&st, all));
    let ctx = Ctx::new(0, 0, NO_ARG, CtxVars::Empty, EventPayload::default());
    assert_eq!(resolve_ref(&st, &ctx, this).unwrap(), [0]);
    // This() reads no view; the paranoid check resolves it again against derived(st)
    // (tests/paranoid.py:125-132), which fills the cache.
    assert_eq!(cached(&st).is_some(), cfg!(feature = "paranoid"));
    let got = resolve_ref(&st, &ctx, all).unwrap();
    let dv = cached(&st).expect("All() computes the view");
    assert_eq!(got, resolve(&st, &dv, &ctx, all).unwrap());
    let none = Ctx::new(0, NO_ARG, NO_ARG, CtxVars::Empty, EventPayload::default());
    assert!(resolve_ref(&st, &none, this).unwrap().is_empty());
}

#[test]
fn zone_loc_codes_are_view_py_936_954() {
    let codes: Vec<(Zone, i32)> = Zone::ALL.iter().map(|&z| (z, zone_loc_code(z))).collect();
    assert_eq!(
        codes,
        [
            (Zone::Deck, 8),
            (Zone::ResourceDeck, 10),
            (Zone::ResourceArea, 5),
            (Zone::Battle, 0),
            (Zone::Shield, 3),
            (Zone::Base, 2),
            (Zone::Removal, 11),
            (Zone::Hand, 6),
            (Zone::Trash, 7),
            (Zone::Resolving, -1),
            (Zone::Outside, -1),
            (Zone::Paired, 1),
        ]
    );
}

#[test]
fn ctx_falls_back_to_the_host_and_reads_the_first_event_pair() {
    let mut ev = EventPayload::default();
    ev.push(EvKey::Subject, 4).unwrap();
    ev.push(EvKey::Amount, 2).unwrap();
    ev.push(EvKey::Amount, 9).unwrap();
    let ctx = Ctx::new(1, 7, NO_ARG, CtxVars::Empty, ev);
    assert_eq!(ctx.card_uid, 7);
    assert_eq!(Ctx::new(1, 7, 3, CtxVars::Empty, ev).card_uid, 3);
    assert_eq!(ctx.ev(EvKey::Amount, 0), 2);
    assert_eq!(ctx.ev(EvKey::By, NO_ARG), NO_ARG);
    let reg = Registry::embedded();
    let amount = reg.str_id("amount").unwrap();
    assert_eq!(ctx.ev_str(&reg, amount, 0), 2);
}

#[test]
fn node_json_round_trips_every_card_script() {
    let reg = Registry::embedded();
    let mut n = 0;
    for d in 0..reg.db().len() {
        let Some(script) = reg.card(DefId(u16::try_from(d).unwrap())).script else {
            continue;
        };
        let j = node_json(&reg, script);
        let text = gcg_core::pyjson::dumps(&j, gcg_core::pyjson::DumpOptions::COMPACT_SORTED_ASCII);
        let v: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["_"], "CardScript", "{}", reg.repr(script));
        match reg.decode_inline(&v) {
            Ok(InlineNode::Arena(id)) => assert_eq!(id, script),
            other => panic!("{}: {other:?}", reg.repr(script)),
        }
        n += 1;
    }
    assert!(n > 1000);
}

#[test]
fn node_json_reads_strings_with_escapes_and_tuples() {
    let reg = Registry::embedded();
    let (r1, _) = rule(&reg);
    let j = node_json(&reg, r1.node());
    let v: Value = serde_json::from_str(&gcg_core::pyjson::dumps(
        &j,
        gcg_core::pyjson::DumpOptions::COMPACT_SORTED_ASCII,
    ))
    .unwrap();
    let keys: Vec<&String> = v.as_object().unwrap().keys().collect();
    assert_eq!(
        keys,
        [
            "_",
            "amount",
            "damage_kind",
            "kind",
            "name",
            "once_per_turn",
            "source_filters",
            "source_side"
        ]
    );
    let mut r = super::signature::read_repr("('a\\'b', \"c'd\", '\\x1f\\\\', (1,), (), -3, None)");
    assert_eq!(
        r,
        Some(serde_json::json!([
            "a'b",
            "c'd",
            "\u{1f}\\",
            [1],
            [],
            -3,
            null
        ]))
    );
    r = super::signature::read_repr("<Kw.REPAIR: 'Repair'>");
    assert_eq!(r, Some(serde_json::json!("Repair")));
    assert_eq!(super::signature::read_repr("(1, 2"), None);
}
