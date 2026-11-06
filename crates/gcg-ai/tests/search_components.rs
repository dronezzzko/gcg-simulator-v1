//! D2's shared search components against the Python reference, on the states of
//! `tests/vectors/search/decisions.ndjson` (`oracle/export_ai_vectors.py`): canonical move
//! keys, per-decision seeding, the decision log, opening scores and the mulligan. These read
//! only the state layer, the card database and the effect registry. The components that need
//! the derived view, determinisation or `apply` are in `search_engine.rs`.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use gcg_ai::{
    AgentCore, AgentError, Alternative, MoveKey, canonical, log_entry, mulligan_choice,
    opening_score,
};
use gcg_core::pyjson::{DumpOptions, dumps};
use gcg_core::rng::{SplitMix64, derive_seed};
use gcg_data::{CardDb, CardDef, DefId};
use gcg_effects::Registry;
use gcg_engine::canon::from_canonical;
use gcg_engine::{Action, ActionKind, DecisionKind, GameState, StateSnapshot};
use serde_json::Value;

fn vectors() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/vectors/search")
}

fn records() -> Vec<Value> {
    let text = fs::read_to_string(vectors().join("decisions.ndjson")).expect("the vectors");
    text.lines()
        .map(|l| serde_json::from_str(l).expect("a JSON record"))
        .collect()
}

fn state(reg: &Arc<Registry>, rec: &Value) -> GameState {
    let snap = &rec["snapshot"];
    let snapshot = StateSnapshot {
        state: snap["state"].clone(),
        continuous: snap["continuous"].clone(),
        filter_sets: snap["filter_sets"].clone(),
        rules_checked: snap["rules_checked"].as_i64().expect("rules_checked"),
    };
    from_canonical(reg, &snapshot).unwrap_or_else(|e| panic!("{}: {e}", rec["id"]))
}

fn u64_of(v: &Value) -> u64 {
    v.as_u64()
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        .unwrap_or_else(|| panic!("not a u64: {v}"))
}

fn int(v: &Value) -> i64 {
    v.as_i64().unwrap_or_else(|| panic!("not an int: {v}"))
}

fn action(v: &Value) -> Action {
    let kind = ActionKind::from_value(v[0].as_str().expect("a kind")).expect("an action kind");
    let arg = |i: usize| i32::try_from(int(&v[i])).expect("an i32 argument");
    Action::new(kind, arg(1), arg(2), arg(3))
}

fn move_key(v: &Value) -> MoveKey {
    MoveKey {
        kind: ActionKind::from_value(v[0].as_str().expect("a kind")).expect("an action kind"),
        a: int(&v[1]),
        b: int(&v[2]),
        variant: i32::try_from(int(&v[3])).expect("an i32 variant"),
    }
}

/// `float.fromhex` for the forms `float.hex()` writes (`0x1.8p-2`, `-0x0.0p+0`, subnormals).
fn fhex(s: &str) -> f64 {
    let (neg, body) = s.strip_prefix('-').map_or((false, s), |r| (true, r));
    let body = body.strip_prefix("0x").expect("a hex float");
    let (mantissa, exp) = body.split_once('p').expect("an exponent");
    let exp: i32 = exp.parse().expect("an int exponent");
    let (int_part, frac) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let mut m = u64::from_str_radix(int_part, 16).expect("hex digits");
    for c in frac.chars() {
        m = m * 16 + u64::from(c.to_digit(16).expect("a hex digit"));
    }
    let shift = exp - 4 * i32::try_from(frac.len()).expect("short");
    let pow2 = if shift >= -1022 {
        f64::from_bits(u64::try_from(shift + 1023).expect("in range") << 52)
    } else {
        f64::from_bits(1u64 << u32::try_from(shift + 1074).expect("subnormal range"))
    };
    #[expect(clippy::cast_precision_loss, reason = "a 53-bit mantissa is exact")]
    let v = m as f64 * pow2;
    if neg { -v } else { v }
}

fn alternatives(v: &Value) -> Vec<Alternative> {
    v.as_array()
        .expect("alternatives")
        .iter()
        .map(|a| Alternative {
            action: action(&a[0]),
            value: fhex(a[1].as_str().expect("a hex float")),
            visits: u32::try_from(int(&a[2])).expect("visits"),
        })
        .collect()
}

fn same_alternatives(id: &Value, got: &[Alternative], want: &[Alternative]) {
    assert_eq!(got.len(), want.len(), "{id}: alternatives");
    for (g, w) in got.iter().zip(want) {
        assert_eq!(
            (g.action, g.value.to_bits(), g.visits),
            (w.action, w.value.to_bits(), w.visits),
            "{id}"
        );
    }
}

#[test]
fn the_vectors_cover_every_decision_kind_the_agents_meet() {
    let recs = records();
    let kinds: Vec<&str> = recs
        .iter()
        .map(|r| r["kind"].as_str().expect("kind"))
        .collect();
    for kind in ["choose_first", "redraw", "main", "action_step", "select"] {
        assert!(kinds.contains(&kind), "{kind}");
    }
}

#[test]
fn canonical_keys_and_representatives_match_python() {
    let reg = Registry::embedded();
    let mut checked = 0;
    for rec in records() {
        let st = state(&reg, &rec);
        let dec = st.pending().expect("a pending decision");
        let want: Vec<(MoveKey, Action)> = rec["canonical"]
            .as_array()
            .expect("canonical")
            .iter()
            .map(|m| (move_key(&m[0]), action(&m[1])))
            .collect();
        assert_eq!(canonical(&st, &dec), want, "{}", rec["id"]);
        checked += 1;
    }
    assert!(checked >= 100, "{checked}");
}

#[test]
fn per_decision_seeds_follow_the_agent_counter() {
    let reg = Registry::embedded();
    for rec in records() {
        let st = state(&reg, &rec);
        let player = u8::try_from(int(&rec["player"])).expect("a seat");
        let name = rec["agent"].as_str().expect("an agent name");
        let mut core = AgentCore::new(name, u64_of(&rec["agent_seed"]), false);
        let k = u64_of(&rec["k"]);
        let mut seed = 0;
        for _ in 0..k {
            let (dec, rng) = core.begin(&st, player).expect("a decision for the seat");
            assert_eq!(dec.hdr.player, player);
            seed = rng.state;
        }
        assert_eq!(core.decisions(), k);
        assert_eq!(seed, u64_of(&rec["seed"]), "{}", rec["id"]);
        let err = core
            .begin(&st, 1 - player)
            .expect_err("not the seat's decision");
        assert_eq!(core.decisions(), k, "a failed begin does not count");
        assert_eq!(
            err,
            AgentError::NoDecision {
                agent: name.into(),
                player: 1 - player
            }
        );
        assert_eq!(
            err.to_string(),
            format!("{name}: no decision pending for player {}", 1 - player)
        );
    }
    // The seed path material is the masked master.
    assert_eq!(
        derive_seed(u64::MAX, &[gcg_core::rng::SeedPart::Str("x")]),
        15_212_809_081_249_037_171
    );
}

/// The alternatives the agent handed to `_finish` for the recorded choice.
fn finished_alternatives(rec: &Value) -> Vec<Alternative> {
    match rec["kind"].as_str() {
        Some("redraw") => alternatives(&rec["mulligan"][0]["alts"]),
        Some("choose_first") => alternatives(&rec["first_player"]["alts"]),
        _ => alternatives(&rec["search"][0]["alts"]),
    }
}

#[test]
fn decision_log_entries_match_python_bytes() {
    let reg = Registry::embedded();
    for rec in records() {
        let st = state(&reg, &rec);
        let player = u8::try_from(int(&rec["player"])).expect("a seat");
        let chosen = action(&rec["chosen"]);
        let alts = finished_alternatives(&rec);
        let entry = log_entry(&st, player, chosen, &alts);
        let text = dumps(&entry.to_json(), DumpOptions::COMPACT_SORTED_ASCII);
        assert_eq!(
            text,
            rec["log_text"].as_str().expect("log_text"),
            "{}",
            rec["id"]
        );

        let mut core = AgentCore::new("mcts-fast", 0, true);
        assert_eq!(core.finish(&st, player, chosen, &alts), chosen);
        assert_eq!(core.entries(), std::slice::from_ref(&entry));
        let mut quiet = AgentCore::new("mcts-fast", 0, false);
        quiet.finish(&st, player, chosen, &alts);
        assert!(quiet.entries().is_empty());
    }
}

#[test]
fn opening_scores_match_python() {
    let reg = Registry::embedded();
    let db = CardDb::embedded();
    let text = fs::read_to_string(vectors().join("opening.json")).expect("opening.json");
    let cases: Value = serde_json::from_str(&text).expect("JSON");
    let cases = cases.as_array().expect("a list");
    assert!(cases.len() >= 100);
    for case in cases {
        let hand: Vec<&CardDef> = case["hand"]
            .as_array()
            .expect("a hand")
            .iter()
            .map(|d| db.by_id(DefId(u16::try_from(int(d)).expect("a def id"))))
            .collect();
        let first = case["first"].as_bool().expect("first");
        let want = fhex(case["score"].as_str().expect("a hex float"));
        let got = opening_score(&hand, first, &reg);
        assert_eq!(got.to_bits(), want.to_bits(), "{case}");
    }
}

#[test]
fn mulligans_match_python() {
    let reg = Registry::embedded();
    let mut checked = 0;
    for rec in records() {
        if rec["kind"] != "redraw" {
            continue;
        }
        let st = state(&reg, &rec);
        assert_eq!(
            st.pending().expect("pending").hdr.kind,
            DecisionKind::Redraw
        );
        let player = u8::try_from(int(&rec["player"])).expect("a seat");
        for m in rec["mulligan"].as_array().expect("mulligan") {
            let mut rng = SplitMix64::new(u64_of(&rec["seed"]));
            let samples = u32::try_from(int(&m["samples"])).expect("samples");
            let (chosen, alts) = mulligan_choice(&st, player, samples, &mut rng);
            assert_eq!(chosen, action(&m["action"]), "{}", rec["id"]);
            same_alternatives(&rec["id"], &alts, &alternatives(&m["alts"]));
            assert_eq!(rng.state, u64_of(&m["rng_after"]), "{}", rec["id"]);
            checked += 1;
        }
    }
    assert!(checked >= 12, "{checked}");
}

#[test]
fn hex_floats_parse_exactly() {
    assert_eq!(
        fhex("0x1.b00be33f8f4adp-2").to_bits(),
        0x3FDB_00BE_33F8_F4AD
    );
    assert_eq!(fhex("0x1.0000000000000p+0"), 1.0);
    assert_eq!(fhex("-0x0.0p+0").to_bits(), (-0.0f64).to_bits());
    assert_eq!(fhex("0x0.0000000000001p-1022").to_bits(), 1);
    assert_eq!(fhex("0x1.ccccccccccccdp-1"), 0.9);
}
