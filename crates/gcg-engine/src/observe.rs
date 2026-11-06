//! Information sets and determinization (`engine/observe.py`; docs/dev/contracts.md,
//! gcg-engine `observe`). Both decklists are known to both players; hands, deck
//! order and Shields are hidden. A player knows a card's identity when its `known` bit is set;
//! every other identity is drawn from that owner's decklist minus the cards the player knows.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::sync::Arc;

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

thread_local! {
    static FIT_ADJUSTMENTS: Cell<FitAdjustments> = const {
        Cell::new(FitAdjustments { pads: 0, trims: 0 })
    };
}

/// W-08: how often `_fit` changed a pool's size on this thread since it started. A pool whose
/// size differs from its hidden slots holds a multiset the real game cannot contain: a padded
/// pool repeats cards, a trimmed one drops the cards past the slot count. Nothing reads it
/// inside a game, and no output contains it.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct FitAdjustments {
    /// Pools shorter than their slots, padded by repetition.
    pub pads: u64,
    /// Pools longer than their slots, trimmed to the first `n`.
    pub trims: u64,
}

/// W-08: the pads and trims `_fit` made on this thread so far (see [`FitAdjustments`]).
#[must_use]
pub fn fit_adjustments() -> FitAdjustments {
    FIT_ADJUSTMENTS.with(Cell::get)
}

fn note_fit(pad: bool) {
    FIT_ADJUSTMENTS.with(|c| {
        let mut a = c.get();
        if pad {
            a.pads = a.pads.saturating_add(1);
        } else {
            a.trims = a.trims.saturating_add(1);
        }
        c.set(a);
    });
}

/// `_fit(pool, n)` (`engine/observe.py:98-110`): the first `n` of the sorted pool, or the pool
/// padded by repetition from its start. Every pad and trim is counted ([`fit_adjustments`]).
fn fit(mut pool: Vec<DefId>, n: usize) -> Result<Vec<DefId>, EngineError> {
    if pool.len() >= n {
        if pool.len() > n {
            note_fit(false);
        }
        pool.truncate(n);
        return Ok(pool);
    }
    if pool.is_empty() {
        return Err(EngineError::Value(
            "cannot determinize: empty pool for hidden cards".to_owned(),
        ));
    }
    note_fit(true);
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

// ---------------------------------------------------------------------------------------------
// Exact pools and identity assignment (W-08, W-134, W-139; additive, unused by the reference)

/// W-08: the cards of `owner` hidden from an observer, wherever they lie, and the identities
/// they can be by card conservation.
///
/// The slots are `owner`'s cards (by [`crate::CardInstance::owner`], not by the zone list that
/// holds them) in a hidden zone of either player whose identity the observer does not know, in
/// uid order; a slot in a Resource Deck is a resource slot. The pools are [`unknown_pools`]:
/// `owner`'s decklists minus every card of `owner` the observer can see. Each owner's cards are
/// exactly its decklists (tokens are never hidden), so the pools hold as many cards as the
/// slots, and an assignment from them is a multiset the real game can contain.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct HiddenPools {
    /// Hidden non-resource slots of `owner`, in uid order.
    pub main_slots: Vec<Uid>,
    /// Hidden slots of `owner` in a Resource Deck, in uid order.
    pub res_slots: Vec<Uid>,
    /// The main-deck pool, sorted.
    pub main: Vec<DefId>,
    /// The resource pool, sorted.
    pub res: Vec<DefId>,
}

impl HiddenPools {
    /// Every slot: the main slots, then the resource slots.
    #[must_use]
    pub fn slots(&self) -> Vec<Uid> {
        self.main_slots
            .iter()
            .chain(&self.res_slots)
            .copied()
            .collect()
    }
}

/// W-08: [`HiddenPools`] of `owner` for `observer`.
///
/// # Errors
/// [`EngineError::Value`] when the pools hold a different number of cards than the slots (a
/// conservation failure: a hidden card outside its owner's decklists).
pub fn exact_pools(st: &GameState, observer: u8, owner: u8) -> Result<HiddenPools, EngineError> {
    let bit = 1 << observer;
    let mut slots: Vec<Uid> = (0..2u8)
        .flat_map(|side| {
            HIDDEN_ZONES
                .iter()
                .flat_map(move |&z| st.zone(side, z).iter().copied())
        })
        .filter(|&u| {
            let c = st.card(u);
            c.owner == owner && c.known & bit == 0
        })
        .collect();
    slots.sort();
    let (res_slots, main_slots): (Vec<Uid>, Vec<Uid>) = slots
        .iter()
        .partition(|&&u| st.card(u).zone == Zone::ResourceDeck);
    let (main, res) = unknown_pools(st, observer, owner);
    if main.len() + res.len() != main_slots.len() + res_slots.len() {
        return Err(EngineError::Value(format!(
            "cannot determinize exactly: player {owner} has {} hidden cards and {} unseen \
             decklist cards",
            main_slots.len() + res_slots.len(),
            main.len() + res.len()
        )));
    }
    Ok(HiddenPools {
        main_slots,
        res_slots,
        main,
        res,
    })
}

/// Removes one copy of `def_id` from the sorted `pool`, or returns `false`.
fn take_one(pool: &mut Vec<DefId>, def_id: DefId) -> bool {
    pool.binary_search(&def_id)
        .map(|i| {
            pool.remove(i);
        })
        .is_ok()
}

/// The identities of `p`'s slots from its pools shuffled by `rng` (owner by owner, the main
/// pool and then the resource pool, as `determinize`). When a resource card lies outside a
/// Resource Deck (or a main-deck card inside one), the pool longer than its slots gives its
/// last cards to the other slots, which are then shuffled again; no card is repeated or
/// dropped.
fn exact_ids(p: HiddenPools, rng: &mut SplitMix64) -> Vec<(Uid, DefId)> {
    let HiddenPools {
        main_slots,
        res_slots,
        mut main,
        mut res,
    } = p;
    rng.shuffle(&mut main);
    rng.shuffle(&mut res);
    if res.len() > res_slots.len() {
        main.extend(res.drain(res_slots.len()..));
        rng.shuffle(&mut main);
    } else if main.len() > main_slots.len() {
        res.extend(main.drain(main_slots.len()..));
        rng.shuffle(&mut res);
    }
    main_slots
        .into_iter()
        .zip(main)
        .chain(res_slots.into_iter().zip(res))
        .collect()
}

/// Checks that every listed card is hidden from `observer` and every identity a definition.
fn check_hidden(s: &GameState, observer: u8, ids: &[(Uid, DefId)]) -> Result<(), EngineError> {
    let db_len = s.db().len();
    for &(uid, def_id) in ids {
        if usize::from(uid) >= s.cards.len() || !is_hidden_from(s, uid, observer) {
            return Err(EngineError::Value(format!(
                "card {uid} is not hidden from player {observer}"
            )));
        }
        if usize::from(def_id.0) >= db_len {
            return Err(EngineError::Value(format!(
                "def_id {} is not a definition of the card database",
                def_id.0
            )));
        }
    }
    Ok(())
}

/// W-08 (P6-C interface request 1): sets the identity of each listed card, every one of which
/// must lie in a hidden zone with an identity `observer` does not know, then rewrites the
/// history to the new identities and touches the state, as `determinize` does. The game
/// generator is unchanged.
///
/// # Errors
/// [`EngineError::Value`] for a card that is not hidden from `observer` or an identity outside
/// the card database; the state is then unchanged.
pub fn assign_hidden(
    s: &mut GameState,
    observer: u8,
    ids: &[(Uid, DefId)],
) -> Result<(), EngineError> {
    check_hidden(s, observer, ids)?;
    for &(uid, def_id) in ids {
        s.cards[usize::from(uid)].def_id = def_id;
    }
    own_history(s);
    s.touch();
    Ok(())
}

/// W-08: [`determinize_into`] with exact pools: every card of either owner hidden from
/// `observer` (wherever it lies) takes an identity from its owner's [`exact_pools`], so the
/// world's hidden multiset of each owner is the true one and nothing is padded or trimmed.
/// The cards of `keep` (hidden from `observer`) keep their identities: they leave the slots,
/// and one copy of each leaves its owner's pool.
///
/// The draws are `determinize`'s (`SplitMix64(seed)`, owner 0 then owner 1, the main pool then
/// the resource pool, an owner without slots drawing nothing; the history rewritten, the
/// generator `SplitMix64(derive_seed(seed, "hidden-rng"))`, one touch), so where every hidden
/// card lies in its owner's zones, `_fit` neither pads nor trims and `keep` is empty, the world
/// is `determinize`'s.
///
/// # Errors
/// A conservation failure of [`exact_pools`], or a kept card that is not hidden from
/// `observer` or whose identity is not in its owner's pool. On an error `out` holds a partly
/// determinized copy.
pub fn determinize_exact_into(
    st: &GameState,
    observer: u8,
    seed: u64,
    keep: &[Uid],
    out: &mut GameState,
) -> Result<(), EngineError> {
    out.clone_from_state(st);
    let mut rng = SplitMix64::new(seed);
    for owner in 0u8..2 {
        let mut p = exact_pools(out, observer, owner)?;
        for &uid in keep {
            if usize::from(uid) >= out.cards.len() || !is_hidden_from(out, uid, observer) {
                return Err(EngineError::Value(format!(
                    "kept card {uid} is not hidden from player {observer}"
                )));
            }
            let c = out.card(uid);
            if c.owner != owner {
                continue;
            }
            let def_id = out.db().base_def_id(c.def_id);
            let (slots, pools) = if c.zone == Zone::ResourceDeck {
                (&mut p.res_slots, [&mut p.res, &mut p.main])
            } else {
                (&mut p.main_slots, [&mut p.main, &mut p.res])
            };
            slots.retain(|&u| u != uid);
            let [first, second] = pools;
            if !take_one(first, def_id) && !take_one(second, def_id) {
                return Err(EngineError::Value(format!(
                    "kept card {uid} is not in player {owner}'s unseen decklist cards"
                )));
            }
        }
        if p.main_slots.is_empty() && p.res_slots.is_empty() {
            continue;
        }
        for (uid, def_id) in exact_ids(p, &mut rng) {
            out.cards[usize::from(uid)].def_id = def_id;
        }
    }
    own_history(out);
    out.rng = SplitMix64::new(derive_seed(seed, &[SeedPart::Str("hidden-rng")]));
    out.touch();
    Ok(())
}

/// W-08: [`determinize_exact_into`] into a new state, keeping no identity.
///
/// # Errors
/// As [`determinize_exact_into`].
pub fn determinize_exact(
    st: &GameState,
    observer: u8,
    seed: u64,
) -> Result<GameState, EngineError> {
    let mut out = st.clone();
    determinize_exact_into(st, observer, seed, &[], &mut out)?;
    Ok(out)
}

/// W-134 (P6-C interface request 1): a copy of `st` whose game uses the decklists `main` and
/// `resource` (sorted here, as `game.py:151-152` stores them): `determinize`, the pools and
/// the listened events follow them; the test overrides stay. The cards are unchanged.
///
/// # Errors
/// [`EngineError::Internal`] for a def_id outside the card database.
pub fn with_decklists(
    st: &GameState,
    main: [Box<[DefId]>; 2],
    resource: [Box<[DefId]>; 2],
) -> Result<GameState, EngineError> {
    let sorted = |mut v: Box<[DefId]>| {
        v.sort();
        v
    };
    let [m0, m1] = main;
    let [r0, r1] = resource;
    let old = &st.konst;
    let mut konst = crate::GameConst::new(
        Arc::clone(&old.reg),
        [sorted(m0), sorted(m1)],
        [sorted(r0), sorted(r1)],
        old.turn_limit,
        old.max_actions,
    )?;
    konst.test_overrides.clone_from(&old.test_overrides);
    let mut out = st.clone();
    out.konst = Arc::new(konst);
    Ok(out)
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
