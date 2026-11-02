//! T-058, engine property tests (PLAN §9.6) on states of seeded random games between the
//! example decks (cases from `gcg_core::prop`), through the public API:
//!
//! - `clone_from_state(x)` equals `clone(x)`: the same canonical bytes, digest, version, RNG,
//!   `rules_checked`, history epoch and options, into recycled destinations of other sizes
//!   with or without a cached view, and the same digests along one random continuation;
//!   writing a copy leaves its source alone (W-52);
//! - `from_canonical` of a state's snapshot re-serialises to the same bytes and digest, keeps
//!   `rules_checked` and the options, snapshots identically, and plays on identically; a state
//!   holding each value effect no IR node spells round-trips through the snapshot's side table;
//! - `determinize` is pure in `(st, observer, seed)`: it leaves `st` alone, keeps every zone,
//!   every field but the hidden def ids, every card the observer knows and the observer's
//!   information-set key, fills the hidden slots with `_fit` of the unknown pools
//!   (`observe.py:56-110`), touches once and reseeds with `derive_seed(seed, "hidden-rng")`;
//!   `determinize_into` a recycled state gives the same state;
//! - `permute_hidden` is pure, keeps the observer's information-set key and permutes each
//!   owner's hidden non-resource def ids (`observe.py:113-131`).
//!
//! The arena-level properties of T-058 (the zone lists and the frame arenas against list and
//! dict models, `clone_from_state` on the fixture states, edited-state round trips) are unit
//! tests next to the arenas: `zones::tests::random_edits_agree_with_a_list_model`,
//! `state::tests::random_frame_edits_agree_with_a_dict_model`,
//! `state::tests::clone_from_equals_clone` and `canon::tests::edited_states_round_trip`.

mod l5_common;

use gcg_core::derive_seed;
use gcg_core::prop;
use gcg_core::pyjson::{DumpOptions, dumps};
use gcg_core::rng::SplitMix64;
use gcg_data::DefId;
use gcg_effects::Kw;
use gcg_engine::canon::{canonical_state, from_canonical, state_digest};
use gcg_engine::observe::{
    determinize, determinize_into, information_set_key, is_hidden_from, permute_hidden,
    unknown_instances, unknown_pools,
};
use gcg_engine::testkit::{engine as tk, snapshot};
use gcg_engine::{
    AddOptions, ContRef, Duration, GameState, LISTED_ZONES, LastingArgs, NewGameOptions, Scenario,
    ScenarioOptions, StateSnapshot, Step, Zone, apply, legal_actions, view,
};
use l5_common::{engine_records, reg, start_game};
use serde_json::Value;

/// `json.dumps(x)` with Python's default separators: the canonical bytes, key order included.
const PY_DEFAULT: DumpOptions = DumpOptions {
    indent: None,
    sort_keys: false,
    compact: false,
    ensure_ascii: true,
};

/// The three example decks, as the `setup_touch` vectors carry them.
fn example_decks() -> Vec<&'static Value> {
    let mut out: Vec<&Value> = Vec::new();
    for rec in engine_records("setup_touch") {
        for d in rec["decks"].as_array().unwrap() {
            if !out.contains(&d) {
                out.push(d);
            }
        }
    }
    assert_eq!(out.len(), 3, "the three example decks");
    out
}

/// A random legal action of `st` drawn with `rng`, applied; `false` when the game is over.
fn step_randomly(st: &mut GameState, rng: &mut SplitMix64) -> Result<bool, String> {
    let options = legal_actions(st);
    if options.is_empty() {
        return Ok(false);
    }
    let a = options[rng.randrange_usize(options.len())];
    apply(st, a, true).map_err(|e| format!("{a}: {e}"))?;
    Ok(true)
}

/// States of one seeded random game between two example decks: about `n` decision points
/// spread over the whole game, including the final state.
fn sample_states(rng: &mut SplitMix64, n: u64) -> Result<Vec<GameState>, String> {
    let decks = example_decks();
    let rec = serde_json::json!({
        "decks": [
            decks[rng.randrange_usize(decks.len())],
            decks[rng.randrange_usize(decks.len())],
        ],
        "seed": rng.next_u64().to_string(),
    });
    let mut st = start_game(&rec, NewGameOptions::default()).map_err(|e| e.to_string())?;
    let mut play = SplitMix64::new(rng.next_u64());
    let mut out = Vec::new();
    while st.pending().is_some() {
        if rng.randrange(60) < n {
            out.push(st.clone());
        }
        step_randomly(&mut st, &mut play)?;
    }
    out.push(st);
    Ok(out)
}

/// The digests along `steps` random actions from `st` with `SplitMix64(seed)`.
fn continuation(mut st: GameState, seed: u64, steps: usize) -> Result<Vec<String>, String> {
    let mut rng = SplitMix64::new(seed);
    let mut out = vec![state_digest(&st)];
    for _ in 0..steps {
        if !step_randomly(&mut st, &mut rng)? {
            break;
        }
        out.push(state_digest(&st));
    }
    Ok(out)
}

fn canonical_bytes(st: &GameState) -> String {
    dumps(&canonical_state(st), PY_DEFAULT)
}

/// Everything public that a copy of `a` must equal.
fn same_state(a: &GameState, b: &GameState, what: &str) -> Result<(), String> {
    let checks = [
        ("canonical bytes", canonical_bytes(a) == canonical_bytes(b)),
        ("digest", state_digest(a) == state_digest(b)),
        ("version", a.version() == b.version()),
        ("rng", a.rng_state() == b.rng_state()),
        ("rules_checked", a.rules_checked() == b.rules_checked()),
        ("history epoch", a.history_epoch() == b.history_epoch()),
        ("options", legal_actions(a) == legal_actions(b)),
    ];
    match checks.iter().find(|(_, ok)| !ok) {
        None => Ok(()),
        Some((field, _)) => Err(format!("{what}: the {field} differs")),
    }
}

#[test]
fn clone_from_state_equals_clone_on_game_states() {
    prop::check("engine-clone-from-state", 16, |rng| {
        let sources = sample_states(rng, 4)?;
        let recycled = sample_states(rng, 2)?;
        for src in &sources {
            if rng.randrange(2) == 0 {
                view::derived(src).map_err(|e| e.to_string())?;
            }
            let mut dst = recycled[rng.randrange_usize(recycled.len())].clone();
            if rng.randrange(2) == 0 {
                view::derived(&dst).map_err(|e| e.to_string())?;
            }
            dst.clone_from_state(src);
            let copy = src.clone();
            same_state(src, &copy, "clone")?;
            same_state(src, &dst, "clone_from_state")?;
            let seed = rng.next_u64();
            let want = continuation(src.clone(), seed, 40)?;
            let digest = state_digest(src);
            if continuation(dst, seed, 40)? != want || continuation(copy, seed, 40)? != want {
                return Err("a copy plays on differently from its source".into());
            }
            if state_digest(src) != digest {
                return Err("playing a copy changed its source".into());
            }
        }
        Ok(())
    });
}

/// How many sampled states had each shape the round trip must carry.
#[derive(Default, Debug)]
struct Shapes {
    states: usize,
    value_effects: usize,
    lasting: usize,
    frames: usize,
    once_keys: usize,
    battles: usize,
}

#[test]
fn from_canonical_round_trips_game_states() {
    let mut seen = Shapes::default();
    prop::check("engine-from-canonical", 16, |rng| {
        for st in sample_states(rng, 6)? {
            let snap = snapshot(&st);
            seen.states += 1;
            seen.value_effects +=
                usize::from(snap.continuous.as_object().is_some_and(|m| !m.is_empty()));
            seen.lasting += usize::from(!st.lasting().is_empty() || !st.delayed().is_empty());
            seen.frames += usize::from(!st.frames().is_empty());
            seen.once_keys += usize::from(!st.once_used().is_empty());
            seen.battles += usize::from(!st.battles().is_empty());
            let back = from_canonical(st.registry(), &snap).map_err(|e| e.to_string())?;
            if canonical_bytes(&back) != canonical_bytes(&st) {
                return Err("the canonical bytes differ after a round trip".into());
            }
            let same = state_digest(&back) == state_digest(&st)
                && back.rules_checked() == st.rules_checked()
                && legal_actions(&back) == legal_actions(&st);
            if !same {
                return Err("the digest, rules_checked or options differ".into());
            }
            let again = snapshot(&back);
            if again.state != snap.state || again.continuous != snap.continuous {
                return Err("the snapshot of the rebuilt state differs".into());
            }
            let seed = rng.next_u64();
            if continuation(back, seed, 40)? != continuation(st, seed, 40)? {
                return Err("the rebuilt state plays on differently".into());
            }
        }
        Ok(())
    });
    println!("{seen:?}");
    let s = &seen;
    assert!(s.states >= 100, "{s:?}");
    assert!(
        s.value_effects > 0 && s.lasting > 0 && s.frames > 0,
        "{s:?}"
    );
    assert!(s.once_keys > 0 && s.battles > 0, "{s:?}");
}

/// Each value effect that no IR node spells, as a lasting effect: the snapshot's side table
/// spells it, so the state round-trips (random play reaches only some of them).
#[test]
fn snapshots_spell_every_value_effect() {
    let reg = reg();
    let mut sc = Scenario::new(&reg, ScenarioOptions::default());
    let u = sc
        .add(0, "GD01-060", Zone::Battle, AddOptions::default())
        .unwrap();
    sc.start(Step::Main).unwrap();
    let mut st = sc.into_state();
    let card = reg
        .db()
        .real_cards()
        .find_map(|c| reg.str_id(&c.card_number))
        .expect("a card number among the IR strings");
    let effects = [
        ContRef::StatMod { ap: 1, hp: -1 },
        ContRef::KeywordGrant {
            kw: Kw::FirstStrike,
            amount: 3,
        },
        ContRef::AbilityGrant { card, index: 0 },
    ];
    let target = [(u, st.card(u).zone_seq)];
    for e in effects {
        let o = LastingArgs {
            player: -1,
            uses: 0,
            filters: &[],
            aux: -1,
        };
        tk::add_lasting_effect(&mut st, e, 0, i32::from(u), &target, Duration::ThisTurn, o)
            .unwrap();
    }
    let snap = snapshot(&st);
    assert_eq!(
        snap.continuous.as_object().unwrap().len(),
        3,
        "{}",
        snap.continuous
    );
    let back = from_canonical(st.registry(), &snap).unwrap();
    assert_eq!(canonical_bytes(&back), canonical_bytes(&st));
    assert_eq!(state_digest(&back), state_digest(&st));
    assert_eq!(back.lasting().len(), 3);
    // The control: the registry alone cannot name these effects.
    let bare = StateSnapshot {
        continuous: Value::Object(serde_json::Map::new()),
        ..snap
    };
    assert!(from_canonical(st.registry(), &bare).is_err());
}

/// `_fit(pool, n)` (`observe.py:98-110`); `None` for an empty pool with `n > 0`.
fn fit(pool: &[DefId], n: usize) -> Option<Vec<DefId>> {
    if pool.len() >= n {
        return Some(pool[..n].to_vec());
    }
    if pool.is_empty() {
        return None;
    }
    Some((0..n).map(|i| pool[i % pool.len()]).collect())
}

fn sorted_defs(st: &GameState, uids: &[u16]) -> Vec<DefId> {
    let mut v: Vec<DefId> = uids.iter().map(|&u| st.card(u).def_id).collect();
    v.sort();
    v
}

/// What `determinize` and `permute_hidden` leave alone: every field of every card except the
/// def id of a card hidden from `observer`, every zone list, the decklists and the decision.
fn keeps_the_known(st: &GameState, w: &GameState, observer: u8) -> Result<(), String> {
    if st.cards().len() != w.cards().len() {
        return Err("the card count changed".into());
    }
    for (u, (a, b)) in (0u16..).zip(st.cards().iter().zip(w.cards())) {
        let hidden = is_hidden_from(st, u, observer);
        let same = (
            a.owner,
            a.zone,
            a.rested,
            a.damage,
            a.pair,
            a.zone_seq,
            a.entered_turn,
        ) == (
            b.owner,
            b.zone,
            b.rested,
            b.damage,
            b.pair,
            b.zone_seq,
            b.entered_turn,
        ) && a.known == b.known
            && (hidden || a.def_id == b.def_id);
        if !same {
            return Err(format!("card {u} changed beyond its hidden identity"));
        }
    }
    for p in 0..2u8 {
        for &z in &LISTED_ZONES {
            if st.zone(p, z) != w.zone(p, z) {
                return Err(format!("player {p}'s {z:?} list changed"));
            }
        }
    }
    let same = st.decklists() == w.decklists()
        && st.resource_decklists() == w.resource_decklists()
        && legal_actions(st) == legal_actions(w)
        && (st.turn(), st.active(), st.step()) == (w.turn(), w.active(), w.step())
        && information_set_key(st, observer) == information_set_key(w, observer);
    if same {
        Ok(())
    } else {
        Err("the decklists, the decision or the information set changed".into())
    }
}

#[test]
fn determinize_is_pure_and_keeps_what_the_observer_knows() {
    prop::check("engine-determinize", 16, |rng| {
        let states = sample_states(rng, 5)?;
        for st in &states {
            let observer = u8::from(rng.randrange(2) == 1);
            let seed = rng.next_u64();
            let (digest, version) = (state_digest(st), st.version());
            let w = determinize(st, observer, seed).map_err(|e| e.to_string())?;
            let again = determinize(st, observer, seed).map_err(|e| e.to_string())?;
            if state_digest(st) != digest || st.version() != version {
                return Err("determinize changed its input".into());
            }
            if state_digest(&again) != state_digest(&w) {
                return Err("determinize is not a function of (st, observer, seed)".into());
            }
            keeps_the_known(st, &w, observer)?;
            if w.version() != version + 1 || w.rng_state() != derive_seed!(seed, "hidden-rng") {
                return Err("the world's version or RNG".into());
            }
            for owner in 0..2u8 {
                let slots = unknown_instances(st, observer, owner);
                let (res, main): (Vec<u16>, Vec<u16>) = slots
                    .iter()
                    .partition(|&&u| st.card(u).zone == Zone::ResourceDeck);
                let (main_pool, res_pool) = unknown_pools(st, observer, owner);
                for (uids, pool) in [(&main, &main_pool), (&res, &res_pool)] {
                    let mut want = fit(pool, uids.len()).ok_or("an empty pool")?;
                    want.sort();
                    if sorted_defs(&w, uids) != want {
                        return Err(format!("owner {owner}'s hidden cards are not _fit(pool)"));
                    }
                }
            }
            let mut out = states[rng.randrange_usize(states.len())].clone();
            determinize_into(st, observer, seed, &mut out).map_err(|e| e.to_string())?;
            same_state(&w, &out, "determinize_into")?;
        }
        Ok(())
    });
}

#[test]
fn permute_hidden_keeps_the_information_set() {
    prop::check("engine-permute-hidden", 16, |rng| {
        for st in sample_states(rng, 5)? {
            let observer = u8::from(rng.randrange(2) == 1);
            let seed = rng.next_u64();
            let digest = state_digest(&st);
            let w = permute_hidden(&st, observer, seed);
            if state_digest(&permute_hidden(&st, observer, seed)) != state_digest(&w)
                || state_digest(&st) != digest
            {
                return Err("permute_hidden is not pure".into());
            }
            keeps_the_known(&st, &w, observer)?;
            if w.version() != st.version() + 1
                || w.rng_state() != derive_seed!(seed, "permuted-rng")
            {
                return Err("the permuted state's version or RNG".into());
            }
            for owner in 0..2u8 {
                let slots: Vec<u16> = unknown_instances(&st, observer, owner)
                    .into_iter()
                    .filter(|&u| st.card(u).zone != Zone::ResourceDeck)
                    .collect();
                if sorted_defs(&st, &slots) != sorted_defs(&w, &slots) {
                    return Err(format!(
                        "owner {owner}'s hidden cards are not a permutation"
                    ));
                }
            }
        }
        Ok(())
    });
}
