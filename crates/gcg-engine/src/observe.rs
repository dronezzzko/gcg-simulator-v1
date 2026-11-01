//! Information sets and determinization (`engine/observe.py`; docs/port/contracts.md,
//! gcg-engine `observe`; Inv 06b §16). Both decklists are known to both players; hands, deck
//! order and Shields are hidden. A player knows a card's identity when its `known` bit is set;
//! every other identity is drawn from that owner's decklist minus the cards the player knows.

use std::collections::BTreeMap;

use gcg_core::pyjson::Json;
use gcg_core::rng::{SeedPart, SplitMix64, derive_seed};
use gcg_data::{DefId, py_repr};
use gcg_effects::Registry;

use crate::{
    Action, DecisionHdr, EngineError, EnginePrompt, GameState, InfoCard, InfoSetKey, Prompt, Uid,
    Zone,
};

/// `HIDDEN_ZONES = frozenset({DECK, HAND, SHIELD, RESOURCE_DECK})` (`engine/observe.py:19`), in
/// value order; every use is membership or is sorted afterwards.
pub use crate::HIDDEN_ZONES;

fn hidden_zone(z: Zone) -> bool {
    HIDDEN_ZONES.contains(&z)
}

/// `is_hidden_from(st, uid, observer)` (`engine/observe.py:22-24`).
#[must_use]
pub fn is_hidden_from(st: &GameState, uid: Uid, observer: u8) -> bool {
    let c = st.card(uid);
    hidden_zone(c.zone) && c.known & (1 << observer) == 0
}

/// `unknown_instances(st, observer, owner)` (`engine/observe.py:27-36`): `owner`'s cards in a
/// hidden zone whose identity `observer` does not know, in uid order.
#[must_use]
pub fn unknown_instances(st: &GameState, observer: u8, owner: u8) -> Vec<Uid> {
    let bit = 1 << observer;
    let mut out: Vec<Uid> = HIDDEN_ZONES
        .iter()
        .flat_map(|&z| st.zone(owner, z).iter().copied())
        .filter(|&uid| st.card(uid).known & bit == 0)
        .collect();
    out.sort();
    out
}

/// `Counter(xs)`, keyed in ascending order (only read by key and through `elements()`, which
/// `unknown_pools` sorts).
fn counter(xs: &[DefId]) -> BTreeMap<DefId, i64> {
    let mut m = BTreeMap::new();
    for &x in xs {
        *m.entry(x).or_insert(0) += 1;
    }
    m
}

/// `sorted(counter.elements())`: each key repeated by its positive count, ascending.
fn sorted_elements(m: &BTreeMap<DefId, i64>) -> Vec<DefId> {
    let mut out = Vec::new();
    for (&k, &n) in m {
        for _ in 0..n.max(0) {
            out.push(k);
        }
    }
    out
}

/// `unknown_pools(st, observer, owner)` (`engine/observe.py:39-53`): the main-deck and resource
/// pools the hidden instances of `owner` can be, each sorted ascending. Every card of `owner`
/// the observer can see (uid order, any zone, tokens included) takes one copy of its printed
/// card (`base_def_id`) from the main pool, else from the resource pool; a missing key reads 0.
#[must_use]
pub fn unknown_pools(st: &GameState, observer: u8, owner: u8) -> (Vec<DefId>, Vec<DefId>) {
    let [m0, m1] = st.decklists();
    let [r0, r1] = st.resource_decklists();
    let (main_list, res_list) = if owner == 0 { (m0, r0) } else { (m1, r1) };
    let mut main = counter(main_list);
    let mut res = counter(res_list);
    let db = st.db();
    let bit = 1 << observer;
    for c in st.cards() {
        if c.owner != owner || (hidden_zone(c.zone) && c.known & bit == 0) {
            continue;
        }
        let def_id = db.base_def_id(c.def_id);
        if let Some(n) = main.get_mut(&def_id).filter(|n| **n > 0) {
            *n -= 1;
        } else if let Some(n) = res.get_mut(&def_id).filter(|n| **n > 0) {
            *n -= 1;
        }
    }
    (sorted_elements(&main), sorted_elements(&res))
}

/// `_fit(pool, n)` (`engine/observe.py:98-110`): the first `n` of the sorted pool, or the pool
/// padded by repetition from its start.
fn fit(mut pool: Vec<DefId>, n: usize) -> Result<Vec<DefId>, EngineError> {
    if pool.len() >= n {
        pool.truncate(n);
        return Ok(pool);
    }
    if pool.is_empty() {
        return Err(EngineError::Value(
            "cannot determinize: empty pool for hidden cards".to_owned(),
        ));
    }
    let base = pool.len();
    let mut i = 0;
    while pool.len() < n {
        pool.push(pool[i % base]);
        i += 1;
    }
    Ok(pool)
}

/// `s.history = _own_history(s)` (`engine/observe.py:86-95`): every entry names the card's
/// current identity. Python builds a new list, so the history's identity changes
/// (`history_epoch`).
fn own_history(s: &mut GameState) {
    let GameState { history, cards, .. } = s;
    for h in history.iter_mut() {
        let Ok(uid) = usize::try_from(h.uid) else {
            continue;
        };
        if let Some(c) = cards.get(uid) {
            let now = i32::from(c.def_id.0);
            if h.def_id != now {
                h.def_id = now;
            }
        }
    }
    s.history_epoch = s.history_epoch.wrapping_add(1);
}

/// The body of `determinize` on the clone `s` (`engine/observe.py:63-83`). RNG order: owner 0's
/// main pool, owner 0's resource pool, owner 1's main, owner 1's resource; an owner without
/// hidden instances draws nothing.
fn determinize_in_place(s: &mut GameState, observer: u8, seed: u64) -> Result<(), EngineError> {
    let mut rng = SplitMix64::new(seed);
    for owner in 0u8..2 {
        let slots = unknown_instances(s, observer, owner);
        if slots.is_empty() {
            continue;
        }
        let (res_slots, main_slots): (Vec<Uid>, Vec<Uid>) = slots
            .iter()
            .partition(|&&u| s.card(u).zone == Zone::ResourceDeck);
        let (main_pool, res_pool) = unknown_pools(s, observer, owner);
        let mut main_pool = fit(main_pool, main_slots.len())?;
        let mut res_pool = fit(res_pool, res_slots.len())?;
        rng.shuffle(&mut main_pool);
        rng.shuffle(&mut res_pool);
        for (&uid, &def_id) in main_slots.iter().zip(&main_pool) {
            s.cards[usize::from(uid)].def_id = def_id;
        }
        for (&uid, &def_id) in res_slots.iter().zip(&res_pool) {
            s.cards[usize::from(uid)].def_id = def_id;
        }
    }
    own_history(s);
    s.rng = SplitMix64::new(derive_seed(seed, &[SeedPart::Str("hidden-rng")]));
    s.touch();
    Ok(())
}

/// `determinize(st, observer, seed)` (`engine/observe.py:56-83`): a full state consistent with
/// `observer`'s information set; hidden identities are resampled from the decklists with
/// `SplitMix64(seed)` and the game RNG is replaced by `SplitMix64(derive_seed(seed,
/// "hidden-rng"))`.
///
/// # Errors
/// Python's `ValueError` of `_fit` when hidden cards have an empty pool.
pub fn determinize(st: &GameState, observer: u8, seed: u64) -> Result<GameState, EngineError> {
    let mut s = st.clone();
    determinize_in_place(&mut s, observer, seed)?;
    Ok(s)
}

/// [`determinize`] into the recycled state `out` (PLAN §4.5): `out` becomes the same value
/// [`determinize`] returns. On an error `out` holds a partly determinized copy.
///
/// # Errors
/// As [`determinize`].
pub fn determinize_into(
    st: &GameState,
    observer: u8,
    seed: u64,
    out: &mut GameState,
) -> Result<(), EngineError> {
    out.clone_from_state(st);
    determinize_in_place(out, observer, seed)
}

/// `permute_hidden(st, observer, seed)` (`engine/observe.py:113-131`): the true identities of
/// the main-deck cards hidden from `observer` are shuffled (owner 0, then owner 1, both always
/// drawing), and the game RNG is reseeded with `derive_seed(seed, "permuted-rng")`.
#[must_use]
pub fn permute_hidden(st: &GameState, observer: u8, seed: u64) -> GameState {
    let mut s = st.clone();
    let mut rng = SplitMix64::new(seed);
    for owner in 0u8..2 {
        let slots: Vec<Uid> = unknown_instances(&s, observer, owner)
            .into_iter()
            .filter(|&u| s.card(u).zone != Zone::ResourceDeck)
            .collect();
        let mut ids: Vec<DefId> = slots.iter().map(|&u| s.card(u).def_id).collect();
        rng.shuffle(&mut ids);
        for (&uid, &def_id) in slots.iter().zip(&ids) {
            s.cards[usize::from(uid)].def_id = def_id;
        }
    }
    own_history(&mut s);
    s.rng = SplitMix64::new(derive_seed(seed, &[SeedPart::Str("permuted-rng")]));
    s.touch();
    s
}

/// The prompt of a decision in a form that is equal exactly when the serialised prompt texts
/// are (`repr(st.pending.to_json())`, `engine/observe.py:149-150`): an Ask or Choose prompt
/// whose text is an engine literal, or an Ask prompt that spells a Choose prompt, takes that
/// form; string ids are interned, so equal Ask texts have equal ids.
fn canonical_prompt(st: &GameState, p: Prompt) -> Prompt {
    let reg = st.registry();
    match p {
        Prompt::Engine(_) => p,
        Prompt::Ask(id) => {
            let text = reg.str(id);
            if let Some(e) = EnginePrompt::from_value(text) {
                return Prompt::Engine(e);
            }
            text.strip_prefix("choose ")
                .and_then(|name| reg.var_id(name))
                .map_or(p, Prompt::Choose)
        }
        Prompt::Choose(v) => {
            let text = format!("choose {}", reg.var_name(v));
            EnginePrompt::from_value(&text).map_or(p, Prompt::Engine)
        }
    }
}

/// `information_set_key(st, observer)` (`engine/observe.py:134-150`): every card with its
/// identity masked (-1) when hidden from `observer`, the zone lists, the turn, the active
/// player, the step and the pending decision. Only equality and hashing are defined.
#[must_use]
pub fn information_set_key(st: &GameState, observer: u8) -> InfoSetKey {
    let cards: Box<[InfoCard]> = st
        .cards()
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let hidden = Uid::try_from(i).is_ok_and(|u| is_hidden_from(st, u, observer));
            InfoCard {
                def_id: if hidden { -1 } else { i32::from(c.def_id.0) },
                owner: c.owner,
                zone: c.zone,
                rested: c.rested,
                damage: c.damage,
                pair: c.pair,
            }
        })
        .collect();
    let pending = st.pending().map(|d| {
        let hdr = DecisionHdr {
            prompt: canonical_prompt(st, d.hdr.prompt),
            ..*d.hdr
        };
        let options: Box<[Action]> = d.options.into();
        (hdr, options)
    });
    InfoSetKey {
        cards,
        zones: st.zones.clone(),
        turn: st.turn,
        active: st.active,
        step: st.step,
        pending,
    }
}

impl InfoSetKey {
    /// The tuple `information_set_key` returns (`engine/observe.py:136-150`) as JSON, tuples
    /// as lists: `(cards, zones, turn, active, int(step), repr(pending))`, where each card is
    /// `(uid, def_id or -1, owner, int(zone), rested, damage, pair)` and `pending` is
    /// `st.pending.to_json()` (`engine/state.py:51-58`) or `None`. `reg` renders the prompt.
    #[must_use]
    pub fn to_json(&self, reg: &Registry) -> Json {
        let cards = self
            .cards
            .iter()
            .enumerate()
            .map(|(uid, c)| {
                Json::Array(vec![
                    Json::Int(i128::try_from(uid).unwrap_or(i128::MAX)),
                    Json::Int(i128::from(c.def_id)),
                    Json::Int(i128::from(c.owner)),
                    Json::Int(i128::from(c.zone.value())),
                    Json::Bool(c.rested),
                    Json::Int(i128::from(c.damage)),
                    Json::Int(i128::from(c.pair)),
                ])
            })
            .collect();
        let zones = (0..2u8)
            .map(|p| {
                Json::Array(
                    Zone::ALL
                        .iter()
                        .map(|&z| {
                            Json::Array(
                                self.zones
                                    .get(p, z)
                                    .iter()
                                    .map(|&u| Json::Int(i128::from(u)))
                                    .collect(),
                            )
                        })
                        .collect(),
                )
            })
            .collect();
        let pending = self.pending.as_ref().map_or_else(
            || "None".to_owned(),
            |(hdr, options)| decision_repr(hdr, options, reg),
        );
        Json::Array(vec![
            Json::Array(cards),
            Json::Array(zones),
            Json::Int(i128::from(self.turn)),
            Json::Int(i128::from(self.active)),
            Json::Int(i128::from(self.step.value())),
            Json::Str(pending),
        ])
    }
}

/// `repr(decision.to_json())` (`engine/state.py:51-58`): the dict
/// `{'player': .., 'kind': .., 'options': [[kind, a, b, c], ..], 'prompt': .., 'context': [[k, v], ..]}`.
fn decision_repr(hdr: &DecisionHdr, options: &[Action], reg: &Registry) -> String {
    let prompt = hdr.prompt.text(reg);
    let options: Vec<String> = options
        .iter()
        .map(|o| format!("[{}, {}, {}, {}]", py_repr(o.kind.as_str()), o.a, o.b, o.c))
        .collect();
    let context: Vec<String> = hdr
        .context
        .as_slice()
        .iter()
        .map(|&(k, v)| format!("[{}, {v}]", py_repr(k.as_str())))
        .collect();
    format!(
        "{{'player': {}, 'kind': {}, 'options': [{}], 'prompt': {}, 'context': [{}]}}",
        hdr.player,
        py_repr(hdr.kind.as_str()),
        options.join(", "),
        py_repr(&prompt),
        context.join(", ")
    )
}
