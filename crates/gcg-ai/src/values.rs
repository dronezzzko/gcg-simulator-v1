//! Heuristic worth of cards and combat outcomes, shared by the playout policy and the
//! evaluation (`ai/values.py`; docs/port/inventory/08-ai-policy-eval.md §3).
//!
//! Every float expression keeps Python's evaluation order: integer parts are summed as
//! integers first, then promoted, and each `+`/`*` is one IEEE operation in the order the
//! Python source writes it (Inv 08 §3: re-associating `card_worth` changes 134 of the 1152
//! real cards).

use gcg_data::{CardDef, CardType};
use gcg_effects::Kw;
use gcg_engine::{Derived, GameState, Uid, Zone, view};

/// `LINK_BONUS` (`ai/values.py:13`).
pub const LINK_BONUS: f64 = 1.0;
/// `PAIR_BONUS` (`ai/values.py:14`).
pub const PAIR_BONUS: f64 = 0.5;
/// `KEYWORD_BONUS` (`ai/values.py:15`).
pub const KEYWORD_BONUS: f64 = 0.3;

/// `hp_left(st, dv, uid)` (`ai/values.py:18-19`): `max(0, hp_of(uid) - damage)`.
#[must_use]
pub fn hp_left(st: &GameState, dv: &Derived, uid: Uid) -> i64 {
    (i64::from(view::hp_of(st, dv, uid)) - i64::from(st.card(uid).damage)).max(0)
}

/// The number of keys of `dv.kw.get(uid)` (a 0-amount keyword is a key).
#[must_use]
pub fn keyword_count(dv: &Derived, uid: Uid) -> usize {
    view::keywords(dv, uid)
        .0
        .iter()
        .filter(|a| a.is_some())
        .count()
}

/// `unit_value(st, dv, uid)` (`ai/values.py:22-33`): the rough worth of a Unit or Base on the
/// field, `0.5 * (ap + hp_left) + 0.25 * level`, plus the link or pair bonus and 0.3 per
/// distinct keyword.
#[must_use]
pub fn unit_value(st: &GameState, dv: &Derived, uid: Uid) -> f64 {
    let c = st.card(uid);
    let stats = i64::from(view::ap_of(st, dv, uid)) + hp_left(st, dv, uid);
    let mut v = 0.5 * int_f64(stats) + 0.25 * f64::from(view::level_of(st, uid));
    if view::is_linked(dv, uid) {
        v += LINK_BONUS;
    } else if c.pair >= 0 {
        v += PAIR_BONUS;
    }
    let kws = keyword_count(dv, uid);
    if kws > 0 {
        v += KEYWORD_BONUS * int_f64(kws as i64);
    }
    v
}

/// `card_worth(cd)` (`ai/values.py:36-47`): the worth of a card in hand or in a pile, from its
/// printed `CardDef` fields only.
#[must_use]
pub fn card_worth(cd: &CardDef) -> f64 {
    match cd.card_type {
        CardType::Unit => {
            1.0 + 0.3 * f64::from(cd.cost) + 0.1 * int_f64(i64::from(cd.ap) + i64::from(cd.hp))
        }
        CardType::Pilot => 1.3 + 0.1 * int_f64(i64::from(cd.ap) + i64::from(cd.hp)),
        CardType::Command => 1.4 + 0.2 * f64::from(cd.cost),
        CardType::Base => 1.2,
        CardType::Resource | CardType::UnitToken | CardType::ExBase | CardType::ExResource => 0.5,
    }
}

/// `pile_value(st, dv, uid)` (`ai/values.py:50-54`): field cards (BATTLE, BASE) by
/// [`unit_value`], every other card by [`card_worth`].
#[must_use]
pub fn pile_value(st: &GameState, dv: &Derived, uid: Uid) -> f64 {
    if matches!(st.card(uid).zone, Zone::Battle | Zone::Base) {
        return unit_value(st, dv, uid);
    }
    card_worth(st.def_of(uid))
}

/// `Fight(kills, dies)` (`ai/values.py:57-62`): the outcome of `attacker` dealing battle
/// damage to `defender` and back (rule 8-5).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Fight {
    pub kills: bool,
    pub dies: bool,
}

/// `fight(st, dv, attacker, defender)` (`ai/values.py:65-71`). First Strike saves the attacker
/// only when it kills.
#[must_use]
pub fn fight(st: &GameState, dv: &Derived, attacker: Uid, defender: Uid) -> Fight {
    let a_ap = i64::from(view::ap_of(st, dv, attacker));
    let d_ap = i64::from(view::ap_of(st, dv, defender));
    let kills = a_ap >= hp_left(st, dv, defender) && a_ap > 0;
    let first_strike = view::has_kw(dv, attacker, Kw::FirstStrike);
    let dies = d_ap >= hp_left(st, dv, attacker) && d_ap > 0 && !(first_strike && kills);
    Fight { kills, dies }
}

/// `ready_blockers(st, dv, player, exclude=-1)` (`ai/values.py:74-80`): `player`'s active
/// BATTLE Units with 【Blocker】 other than `exclude`, in zone order.
#[must_use]
pub fn ready_blockers(st: &GameState, dv: &Derived, player: u8, exclude: i32) -> Vec<Uid> {
    st.zone(player, Zone::Battle)
        .iter()
        .copied()
        .filter(|&u| {
            i32::from(u) != exclude && !st.card(u).rested && view::has_kw(dv, u, Kw::Blocker)
        })
        .collect()
}

/// `life(st, dv, player)` (`ai/values.py:83-87`): Shields plus the Base's remaining HP in
/// Shield equivalents, `len(SHIELD) + base / 3.0`.
#[must_use]
pub fn life(st: &GameState, dv: &Derived, player: u8) -> f64 {
    let base: i64 = st
        .zone(player, Zone::Base)
        .iter()
        .map(|&u| hp_left(st, dv, u))
        .sum();
    int_f64(st.zone(player, Zone::Shield).len() as i64) + int_f64(base) / 3.0
}

/// `exposed(st, player)` (`ai/values.py:90-93`): no Shields and no Base, so the next battle
/// damage to the player wins (rule 1-2-2-1).
#[must_use]
pub fn exposed(st: &GameState, player: u8) -> bool {
    st.zone(player, Zone::Shield).is_empty() && st.zone(player, Zone::Base).is_empty()
}

/// Python's `float(n)` for the small integers of the heuristics (exact below 2^53).
#[must_use]
#[allow(
    clippy::cast_precision_loss,
    reason = "board counts and stat sums are far below 2^53, where the conversion is exact"
)]
pub const fn int_f64(n: i64) -> f64 {
    n as f64
}
