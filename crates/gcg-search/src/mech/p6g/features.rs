//! P6-G's leaf features (docs/dev/PLAN.md §12.3 S10; W-37, W-39, W-142): one superset vector
//! of a position, from the turn player's point of view, from which every P6-G model takes its
//! columns ([`super::model::ModelSpec::columns`]).
//!
//! The first block is the reference's `role_features` (`ai/evaluation.py:280-300`) with its
//! constant first term named `intercept`: no strengthened feature is a tempo (turn) indicator
//! (W-39). The other blocks are the W-37 groups: explicit hand slots per card type with castable
//! counts, per-keyword amounts on the battlefield (one feature pair per `Kw` variant, by an
//! exhaustive `match`), Units with activated and triggered abilities (from the IR), and Burst
//! probabilities per Shield. Each group keeps the two players apart (turn player first).

use gcg_ai::{Density, ROLE_FEATURES, role_features};
use gcg_data::CardType;
use gcg_effects::{Kw, Node};
use gcg_engine::{Derived, EngineError, GameState, Zone, view};

/// The number of reference role features (`ROLE_FEATURES`).
pub const N_BASE: usize = ROLE_FEATURES.len();
/// The columns of the role block that the hand slots replace (`hand_units` … `hand_bases`).
pub const HAND_DIFFS: std::ops::Range<usize> = 7..11;
/// The number of hand-slot features: four card types and two castable counts per player.
pub const N_HAND: usize = 12;
/// The number of keyword features: one per `Kw` variant per player.
pub const N_KW: usize = 2 * Kw::ALL.len();
/// The number of ability-count features.
pub const N_ABILITY: usize = 4;
/// The number of Burst features.
pub const N_BURST: usize = 4;

/// The first column of each block.
pub const HAND_START: usize = N_BASE;
pub const KW_START: usize = HAND_START + N_HAND;
pub const ABILITY_START: usize = KW_START + N_KW;
pub const BURST_START: usize = ABILITY_START + N_ABILITY;
/// The length of the superset vector.
pub const N_FEATURES: usize = BURST_START + N_BURST;

/// The superset feature names, in column order (`tp_` is the turn player, `ot_` the other).
pub const NAMES: [&str; N_FEATURES] = [
    "intercept",
    "life",
    "danger",
    "ap",
    "hp",
    "linked",
    "ready",
    "hand_units",
    "hand_pilots",
    "hand_commands",
    "hand_bases",
    "level",
    "bursts",
    "can_kill_turn_player",
    "can_kill_other",
    "inv_race_turn_player",
    "inv_race_other",
    "tp_hand_unit",
    "tp_hand_pilot",
    "tp_hand_command",
    "tp_hand_base",
    "ot_hand_unit",
    "ot_hand_pilot",
    "ot_hand_command",
    "ot_hand_base",
    "tp_castable_units",
    "tp_castable_others",
    "ot_castable_units",
    "ot_castable_others",
    "tp_kw_repair",
    "ot_kw_repair",
    "tp_kw_breach",
    "ot_kw_breach",
    "tp_kw_support",
    "ot_kw_support",
    "tp_kw_blocker",
    "ot_kw_blocker",
    "tp_kw_first_strike",
    "ot_kw_first_strike",
    "tp_kw_high_maneuver",
    "ot_kw_high_maneuver",
    "tp_kw_suppression",
    "ot_kw_suppression",
    "tp_units_activated",
    "tp_units_triggered",
    "ot_units_activated",
    "ot_units_triggered",
    "tp_burst_next",
    "tp_burst_any",
    "ot_burst_next",
    "ot_burst_any",
];

/// The keyword feature of `kw` (its index among the keyword pairs). Exhaustive, so a new `Kw`
/// variant cannot be forgotten (W-37).
#[must_use]
pub const fn kw_feature(kw: Kw) -> usize {
    match kw {
        Kw::Repair => 0,
        Kw::Breach => 1,
        Kw::Support => 2,
        Kw::Blocker => 3,
        Kw::FirstStrike => 4,
        Kw::HighManeuver => 5,
        Kw::Suppression => 6,
    }
}

/// The hand slot of a card type: one per type a card in hand can have; every other type is an
/// error, never a default slot (W-37: the reference files unknown types under Base).
///
/// # Errors
/// `EngineError::Assertion` for a Resource or a token, which never reach a hand.
pub fn hand_slot(t: CardType) -> Result<usize, EngineError> {
    match t {
        CardType::Unit => Ok(0),
        CardType::Pilot => Ok(1),
        CardType::Command => Ok(2),
        CardType::Base => Ok(3),
        CardType::Resource
        | CardType::UnitToken
        | CardType::ExBase
        | CardType::ExResource
        | CardType::BaseToken => Err(EngineError::Assertion(format!(
            "P6-G: a {t:?} card in hand has no hand slot"
        ))),
    }
}

#[expect(
    clippy::cast_precision_loss,
    reason = "board counts are tiny, so the conversion is exact"
)]
const fn count(n: usize) -> f64 {
    n as f64
}

/// The hand-slot block of player `x` (the level is `x`'s Resource Area count; the other player
/// is judged at its next turn, one Resource later).
fn hand_block(st: &GameState, x: u8, next_turn: bool) -> Result<[f64; 6], EngineError> {
    let level = st.zone(x, Zone::ResourceArea).len() + usize::from(next_turn);
    let level = i32::try_from(level).unwrap_or(i32::MAX);
    let mut slots = [0usize; 4];
    let (mut units, mut others) = (0usize, 0usize);
    for &u in st.zone(x, Zone::Hand) {
        let cd = st.def_of(u);
        let slot = hand_slot(cd.card_type)?;
        slots[slot] += 1;
        if cd.level <= level && cd.cost <= level {
            if slot == 0 {
                units += 1;
            } else {
                others += 1;
            }
        }
    }
    Ok([
        count(slots[0]),
        count(slots[1]),
        count(slots[2]),
        count(slots[3]),
        count(units),
        count(others),
    ])
}

/// The keyword amounts of player `x`'s battlefield: per `Kw`, the summed amounts, where a
/// keyword without an amount (or granted with 0) counts 1.
fn kw_block(st: &GameState, dv: &Derived, x: u8) -> [f64; Kw::ALL.len()] {
    let mut out = [0i64; Kw::ALL.len()];
    for &u in st.zone(x, Zone::Battle) {
        for kw in Kw::ALL {
            if view::has_kw(dv, u, kw) {
                out[kw_feature(kw)] += i64::from(view::kw_amount(dv, u, kw).max(1));
            }
        }
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "keyword sums are tiny, so the conversion is exact"
    )]
    out.map(|v| v as f64)
}

/// Player `x`'s Units with at least one activated and at least one triggered ability (the view's
/// active abilities, IR node kinds).
fn ability_block(st: &GameState, dv: &Derived, x: u8) -> [f64; 2] {
    let reg = st.registry();
    let (mut activated, mut triggered) = (0usize, 0usize);
    for &u in st.zone(x, Zone::Battle) {
        let (mut a, mut t) = (false, false);
        for &aid in view::abilities_of(dv, u) {
            let entry = reg.ability(aid);
            match reg.node(entry.ability.node()) {
                Node::Activated(_) => a = true,
                Node::Triggered(_) => t = true,
                _ => {}
            }
        }
        activated += usize::from(a);
        triggered += usize::from(t);
    }
    [count(activated), count(triggered)]
}

/// The Burst block of owner `x` with Burst density `d` (the probability that one Shield is a
/// Burst card): the next Shield's probability, and the probability that at least one Shield is
/// a Burst card, `1 - (1 - d)^shields` by repeated products.
fn burst_block(st: &GameState, x: u8, d: f64) -> [f64; 2] {
    let shields = st.zone(x, Zone::Shield).len();
    if shields == 0 {
        return [0.0, 0.0];
    }
    let mut none = 1.0;
    for _ in 0..shields {
        none *= 1.0 - d;
    }
    [d, 1.0 - none]
}

/// The superset vector of `st` from its turn player's point of view, with `density` per owner.
///
/// # Errors
/// The derived view's error, or a card in hand without a hand slot.
pub fn superset(st: &GameState, density: Density) -> Result<Vec<f64>, EngineError> {
    let mut out = role_features(st, density)?;
    debug_assert_eq!(out.len(), N_BASE);
    let dv = view::derived(st)?;
    let tp = st.active();
    let ot = 1 - tp;
    let (h_tp, h_ot) = (hand_block(st, tp, false)?, hand_block(st, ot, true)?);
    out.extend_from_slice(&h_tp[..4]);
    out.extend_from_slice(&h_ot[..4]);
    out.extend_from_slice(&h_tp[4..]);
    out.extend_from_slice(&h_ot[4..]);
    let (k_tp, k_ot) = (kw_block(st, &dv, tp), kw_block(st, &dv, ot));
    for i in 0..Kw::ALL.len() {
        out.push(k_tp[i]);
        out.push(k_ot[i]);
    }
    out.extend_from_slice(&ability_block(st, &dv, tp));
    out.extend_from_slice(&ability_block(st, &dv, ot));
    out.extend_from_slice(&burst_block(st, tp, density[usize::from(tp)]));
    out.extend_from_slice(&burst_block(st, ot, density[usize::from(ot)]));
    debug_assert_eq!(out.len(), N_FEATURES);
    Ok(out)
}
