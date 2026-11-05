//! The fast heuristic playout policy (`ai/policy.py`; docs/port/inventory/08-ai-policy-eval.md
//! §2, §4).
//!
//! Every legal option of any decision is scored from the current board without look-ahead.
//! The search uses the scores as rollout policy (Gumbel-max sampling, [`pick`]) and as move
//! priors. Float operations follow the Python source one by one: each conditional binds
//! loosest (`(3.0 + 0.5 * v) if kills else 0.3`), and subtractions stay sequential
//! (`(2.5 - 0.3) - 0.4 == 1.8000000000000003`).
//!
//! `option_scores` reads the options only from its `options` argument, never from the
//! state's pending decision (Inv 08 §1): MCTS passes its pruned and merged moves. It computes
//! the derived view exactly where Python does (`_main`, `_block`, `_excess` and `_select`
//! after its mode check), so the view cache of `st` fills as Python's does (PLAN §4.6).

use gcg_core::rng::SplitMix64;
use gcg_data::CardType;
use gcg_effects::{Kw, RuleKind};
use gcg_engine::{
    Action, ActionKind, BATTLE_LIMIT, CtxKey, DecisionKind, DecisionView, Derived, EngineError,
    GameState, PLAYER_TARGET, SUPPORT_AID, Uid, Zone, view,
};

use crate::PolicyKnobs;
use crate::actions::{attack_can_matter, ex_used};
use crate::math::FloatMath;
use crate::targeting::{self, Polarity};
use crate::values::{
    card_worth, exposed, fight, hp_left, int_f64, life, pile_value, ready_blockers, unit_value,
};

/// `EX_PENALTY` (`ai/policy.py:42`): the score cost of each EX Resource an action spends.
pub const EX_PENALTY: f64 = 1.5;
/// `LETHAL` (`ai/policy.py:43`).
pub const LETHAL: f64 = 60.0;
/// `CLEAR_THE_WAY` (`ai/policy.py:44`).
pub const CLEAR_THE_WAY: f64 = 8.0;
/// `MUST_BLOCK` (`ai/policy.py:45`).
pub const MUST_BLOCK: f64 = 100.0;
/// `KEEPABLE_TOP` (`ai/policy.py:46`).
pub const KEEPABLE_TOP: f64 = 2.0;
/// `IDLE_ATTACK` (`ai/policy.py:47`).
pub const IDLE_ATTACK: f64 = -1.0;

/// `choose(st, rng, temperature, knobs=BASELINE_KNOBS)` (`ai/policy.py:72-83`): the policy's
/// move at `st`'s pending decision. A single option is returned with no scoring and no draw;
/// otherwise the best-scoring option, or a Gumbel-max sample when `rng` is given and
/// `temperature > 0`.
///
/// # Errors
/// `EngineError::Assertion` without a pending decision (`assert dec is not None`); any error
/// of the view the scorers compute; Python's `ValueError`/`IndexError` for a decision with no
/// options.
pub fn choose(
    st: &GameState,
    rng: Option<&mut SplitMix64>,
    temperature: f64,
    knobs: &PolicyKnobs,
) -> Result<Action, EngineError> {
    choose_with(st, rng, temperature, knobs, FloatMath::Platform)
}

/// [`choose`] with the Gumbel draws of [`pick_with`] at `math` (W-137): at
/// [`FloatMath::Platform`] it is [`choose`], bit for bit.
///
/// # Errors
/// As [`choose`].
pub fn choose_with(
    st: &GameState,
    rng: Option<&mut SplitMix64>,
    temperature: f64,
    knobs: &PolicyKnobs,
    math: FloatMath,
) -> Result<Action, EngineError> {
    let Some(dec) = st.pending() else {
        return Err(EngineError::Assertion(String::new()));
    };
    if dec.options.len() == 1 {
        return Ok(dec.options[0]);
    }
    let scores = option_scores(st, &dec, dec.options, knobs)?;
    if dec.options.is_empty() {
        return Err(if rng.is_none() || temperature <= 0.0 {
            EngineError::Value("max() iterable argument is empty".into())
        } else {
            EngineError::Index("tuple index out of range".into())
        });
    }
    Ok(dec.options[pick_with(dec.options, &scores, rng, temperature, math)])
}

/// `pick(options, scores, rng, temperature)` (`ai/policy.py:86-101`): the index of the chosen
/// option.
///
/// Without a generator or with `temperature <= 0` it is the first index of the maximal score
/// (`max(range(n), key=scores.__getitem__)`). Otherwise it is the Gumbel-max trick, which
/// samples exactly from `softmax(score / T)`: one `rng.random()` per score in option order
/// (`0.0` becomes `1e-300`), `v = s / T - ln(-ln(u))` with the platform libm, and the first
/// strictly greater `v` wins. Returns 0 for an empty list (Python fails on it; [`choose`]
/// never passes one).
#[must_use]
pub fn pick(
    options: &[Action],
    scores: &[f64],
    rng: Option<&mut SplitMix64>,
    temperature: f64,
) -> usize {
    pick_with(options, scores, rng, temperature, FloatMath::Platform)
}

/// [`pick`] with the `ln` of `math` in the Gumbel draws (W-137): at [`FloatMath::Platform`] it
/// is [`pick`], bit for bit.
#[must_use]
pub fn pick_with(
    options: &[Action],
    scores: &[f64],
    rng: Option<&mut SplitMix64>,
    temperature: f64,
    math: FloatMath,
) -> usize {
    debug_assert_eq!(options.len(), scores.len());
    // `rng is None or temperature <= 0.0`, with a NaN temperature sampling as in Python.
    let rng = match rng {
        Some(_) if temperature <= 0.0 => None,
        other => other,
    };
    let Some(rng) = rng else {
        let mut best = 0;
        for (i, &s) in scores.iter().enumerate().skip(1) {
            if s > scores[best] {
                best = i;
            }
        }
        return best;
    };
    let mut best_i = 0;
    let mut best_v = f64::NEG_INFINITY;
    for (i, &s) in scores.iter().enumerate() {
        let mut u = rng.random();
        if u == 0.0 {
            u = 1e-300;
        }
        let v = s / temperature - math.ln(-math.ln(u));
        if v > best_v {
            best_i = i;
            best_v = v;
        }
    }
    best_i
}

/// `option_scores(st, dec, knobs=BASELINE_KNOBS)` (`ai/policy.py:104-108`) over `options`
/// (Python's `dec.options`, which MCTS rebuilds with its reduced moves): one score per option,
/// in order, from the scorer of `dec`'s kind (`_SCORERS`, `:388-400`). ORDER_TRIGGER and
/// PAYMENT have no scorer and score all zeros.
///
/// # Errors
/// Any error of the derived view (`_main`, `_block`, `_excess`, `_select`) or of
/// `attack_can_matter`; `EngineError::Assertion` for a BLOCK decision outside a battle;
/// `EngineError::Index` for an option argument that is no uid where the scorer needs one.
pub fn option_scores(
    st: &GameState,
    dec: &DecisionView<'_>,
    options: &[Action],
    knobs: &PolicyKnobs,
) -> Result<Vec<f64>, EngineError> {
    let player = dec.hdr.player;
    match dec.hdr.kind {
        DecisionKind::Main => main_phase(st, player, options, knobs),
        DecisionKind::Block => block(st, player, options),
        DecisionKind::ActionStep => Ok(action_step(st, options)),
        DecisionKind::Burst => Ok(per_option(options, |o| {
            if o.kind == ActionKind::Yes {
                knobs.burst_yes_score
            } else {
                0.0
            }
        })),
        DecisionKind::YesNo => Ok(per_option(options, |o| flag(o.kind == ActionKind::Yes))),
        DecisionKind::ChooseFirst => Ok(per_option(options, |o| flag(o.a == i32::from(player)))),
        DecisionKind::Redraw => Ok(per_option(options, |o| flag(o.kind == ActionKind::Keep))),
        DecisionKind::Excess => excess(st, options),
        DecisionKind::Discard => discard(st, player, options),
        DecisionKind::Arrange => arrange(st, dec, options),
        DecisionKind::Select => select(st, dec, options, knobs),
        DecisionKind::OrderTrigger | DecisionKind::Payment => Ok(vec![0.0; options.len()]),
    }
}

fn per_option(options: &[Action], f: impl FnMut(&Action) -> f64) -> Vec<f64> {
    options.iter().map(f).collect()
}

const fn flag(b: bool) -> f64 {
    if b { 1.0 } else { 0.0 }
}

/// A uid argument of an option or battle record (`st.cards[x]` in Python, which the engine
/// guarantees to be a valid index at these sites).
fn uid_of(st: &GameState, x: i32) -> Result<Uid, EngineError> {
    Uid::try_from(x)
        .ok()
        .filter(|&u| usize::from(u) < st.cards().len())
        .ok_or_else(|| EngineError::Index("list index out of range".into()))
}

// ---------------------------------------------------------------------------------------------
// main phase (ai/policy.py:115-250)

/// `_Board` (`ai/policy.py:115-130`), computed once per MAIN call by `_board` (`:133-155`).
struct Board {
    player: u8,
    units: usize,
    enemy_blockers: Vec<Uid>,
    enemy_exposed: bool,
    enemy_has_base: bool,
    my_base_is_weak: bool,
    enemy_max_ap: i32,
    attackers: usize,
    knobs: PolicyKnobs,
}

impl Board {
    /// `blocked_from_lethal` (`ai/policy.py:127-130`): the enemy is open to a finishing hit,
    /// but has a Blocker for every attacker.
    const fn blocked_from_lethal(&self) -> bool {
        self.enemy_exposed && 0 < self.attackers && self.attackers <= self.enemy_blockers.len()
    }
}

/// `_board(st, dv, dec, knobs)` (`ai/policy.py:133-155`). `attackers` is the number of distinct
/// attacking uids among the ATTACK options; with `zero_ap_face` only attackers whose attack
/// can matter count (`attack_can_matter` is asked once per ATTACK option, in option order).
fn board(
    st: &GameState,
    dv: &Derived,
    player: u8,
    options: &[Action],
    knobs: &PolicyKnobs,
) -> Result<Board, EngineError> {
    let enemy = 1 - player;
    let my_base = st.zone(player, Zone::Base);
    let units = st.zone(player, Zone::Battle).len();
    let enemy_blockers = ready_blockers(st, dv, enemy, -1);
    let enemy_exposed = exposed(st, enemy);
    let enemy_has_base = !st.zone(enemy, Zone::Base).is_empty();
    let my_base_is_weak = my_base
        .first()
        .is_none_or(|&b| st.def_of(b).card_type == CardType::ExBase);
    let enemy_max_ap = st
        .zone(enemy, Zone::Battle)
        .iter()
        .map(|&u| view::ap_of(st, dv, u))
        .max()
        .unwrap_or(0);
    let mut attackers: Vec<i32> = Vec::new();
    for o in options {
        if o.kind != ActionKind::Attack {
            continue;
        }
        if knobs.zero_ap_face && !attack_can_matter(st, player, uid_of(st, o.a)?)? {
            continue;
        }
        if !attackers.contains(&o.a) {
            attackers.push(o.a);
        }
    }
    Ok(Board {
        player,
        units,
        enemy_blockers,
        enemy_exposed,
        enemy_has_base,
        my_base_is_weak,
        enemy_max_ap,
        attackers: attackers.len(),
        knobs: *knobs,
    })
}

/// `_main(st, dec, knobs)` (`ai/policy.py:158-161`).
fn main_phase(
    st: &GameState,
    player: u8,
    options: &[Action],
    knobs: &PolicyKnobs,
) -> Result<Vec<f64>, EngineError> {
    let dv = view::derived(st)?;
    let b = board(st, &dv, player, options, knobs)?;
    options
        .iter()
        .map(|o| main_option(st, &dv, &b, o))
        .collect()
}

/// `_main_option(st, dv, b, o)` (`ai/policy.py:164-184`).
fn main_option(st: &GameState, dv: &Derived, b: &Board, o: &Action) -> Result<f64, EngineError> {
    let k = o.kind;
    if k == ActionKind::EndMain {
        return Ok(0.0);
    }
    if k == ActionKind::Attack {
        return attack(st, dv, b, uid_of(st, o.a)?, o.b);
    }
    let cost = EX_PENALTY * f64::from(ex_used(*o));
    Ok(match k {
        ActionKind::PlayUnit => play_unit(st, b, uid_of(st, o.a)?) - cost,
        ActionKind::Pair => pair(st, dv, uid_of(st, o.a)?, uid_of(st, o.b)?) - cost,
        ActionKind::PlayBase => (if b.my_base_is_weak { 4.0 } else { -1.0 }) - cost,
        ActionKind::PlayCommand => {
            let uid = uid_of(st, o.a)?;
            let mut s = 2.5 + 0.3 * f64::from(st.def_of(uid).cost) - cost;
            if b.blocked_from_lethal()
                && targeting::command_harms_enemy_units(st.registry(), st.card(uid).def_id)
            {
                s += CLEAR_THE_WAY;
            }
            s
        }
        ActionKind::Activate => (if o.b == SUPPORT_AID { 0.5 } else { 1.5 }) - cost,
        _ => 0.0,
    })
}

/// `_play_unit(st, b, uid)` (`ai/policy.py:187-192`), from the printed `CardDef` stats.
fn play_unit(st: &GameState, b: &Board, uid: Uid) -> f64 {
    let cd = st.def_of(uid);
    let mut s = 4.0 + 0.5 * f64::from(cd.cost) + 0.2 * int_f64(i64::from(cd.ap) + i64::from(cd.hp));
    if b.units >= BATTLE_LIMIT {
        s -= 8.0;
    }
    s
}

/// `_pair(st, dv, pilot, unit)` (`ai/policy.py:195-206`). The link check is the overridable
/// `view::link_satisfied` (`policy.py:200` sees the `force_link` monkeypatch).
fn pair(st: &GameState, dv: &Derived, pilot: Uid, unit: Uid) -> f64 {
    let pcd = st.def_of(pilot);
    let ucd = st.def_of(unit);
    let uc = st.card(unit);
    let mut s =
        4.5 + 0.2 * int_f64(i64::from(pcd.ap) + i64::from(pcd.hp)) + 0.1 * unit_value(st, dv, unit);
    if view::link_satisfied(st, ucd, pcd) {
        s += 3.0;
        if uc.entered_turn == st.turn() && !uc.rested {
            s += 1.0;
        }
    }
    if uc.rested {
        s -= 1.5;
    }
    s
}

/// `_unblockable(dv, attacker)` (`ai/policy.py:209-212`): 【High-Maneuver】 or a
/// CANT_BE_BLOCKED rule.
fn unblockable(dv: &Derived, attacker: Uid) -> bool {
    view::has_kw(dv, attacker, Kw::HighManeuver)
        || view::rules_of(dv, attacker, RuleKind::CantBeBlocked)
            .next()
            .is_some()
}

/// `_block_risk(st, dv, b, attacker, target)` (`ai/policy.py:215-227`): the worst loss an
/// enemy Blocker other than `target` can inflict on the attacker.
fn block_risk(st: &GameState, dv: &Derived, b: &Board, attacker: Uid, target: i32) -> f64 {
    if unblockable(dv, attacker) {
        return 0.0;
    }
    let mut worst = 0.0;
    let a_val = unit_value(st, dv, attacker);
    for &bl in &b.enemy_blockers {
        if i32::from(bl) == target {
            continue;
        }
        let f = fight(st, dv, attacker, bl);
        if f.dies {
            let gain = if f.kills {
                0.5 * unit_value(st, dv, bl)
            } else {
                0.0
            };
            let loss = 1.0 + 0.5 * a_val - gain;
            if loss > worst {
                worst = loss;
            }
        }
    }
    worst
}

/// `_attack(st, dv, b, attacker, target)` (`ai/policy.py:230-250`). The block risk is computed
/// first, always.
fn attack(
    st: &GameState,
    dv: &Derived,
    b: &Board,
    attacker: Uid,
    target: i32,
) -> Result<f64, EngineError> {
    let risk = block_risk(st, dv, b, attacker, target);
    if target == PLAYER_TARGET {
        if b.knobs.zero_ap_face && !attack_can_matter(st, b.player, attacker)? {
            return Ok(IDLE_ATTACK);
        }
        if b.enemy_exposed {
            if unblockable(dv, attacker) || b.enemy_blockers.is_empty() {
                return Ok(LETHAL);
            }
            return Ok(if b.blocked_from_lethal() {
                2.0
            } else {
                LETHAL / 2.0 - risk
            });
        }
        let mut s = 2.5 - if b.enemy_has_base { 0.3 } else { 0.0 };
        if i64::from(b.enemy_max_ap) >= hp_left(st, dv, attacker) {
            s -= 0.4;
        }
        return Ok(s - risk);
    }
    let target = uid_of(st, target)?;
    let f = fight(st, dv, attacker, target);
    let mut s = if f.kills {
        3.0 + 0.5 * unit_value(st, dv, target)
    } else {
        0.3
    };
    if f.dies {
        s -= 1.0 + 0.5 * unit_value(st, dv, attacker);
    }
    let breach = view::kw_amount(dv, attacker, Kw::Breach);
    if f.kills && breach != 0 && !exposed(st, 1 - b.player) {
        s += 0.5 * f64::from(breach);
    }
    Ok(s - risk)
}

// ---------------------------------------------------------------------------------------------
// battle decisions (ai/policy.py:257-298)

/// `_block(st, dec, knobs)` (`ai/policy.py:257-285`). The battle is checked before the view is
/// computed (`assert battle is not None`).
fn block(st: &GameState, player: u8, options: &[Action]) -> Result<Vec<f64>, EngineError> {
    let Some(battle) = st.battle() else {
        return Err(EngineError::Assertion(String::new()));
    };
    let dv = view::derived(st)?;
    let attacker = uid_of(st, battle.attacker)?;
    let target = battle.target;
    let at_player = target == PLAYER_TARGET || st.card(uid_of(st, target)?).zone == Zone::Base;
    let saved = if at_player {
        if target == PLAYER_TARGET && exposed(st, player) {
            MUST_BLOCK
        } else {
            1.2 + if life(st, &dv, player) <= 2.0 {
                2.0
            } else {
                0.0
            }
        }
    } else {
        let target = uid_of(st, target)?;
        if fight(st, &dv, attacker, target).kills {
            1.0 + 0.5 * unit_value(st, &dv, target)
        } else {
            0.0
        }
    };
    options
        .iter()
        .map(|o| {
            if o.kind != ActionKind::Block {
                return Ok(0.0);
            }
            let blocker = uid_of(st, o.a)?;
            let f = fight(st, &dv, attacker, blocker);
            let mut s = saved;
            if f.kills {
                s -= 1.0 + 0.5 * unit_value(st, &dv, blocker);
            }
            if f.dies {
                s += 1.0 + 0.5 * unit_value(st, &dv, attacker);
            }
            Ok(s)
        })
        .collect()
}

/// `_action_step(st, dec, knobs)` (`ai/policy.py:288-298`).
fn action_step(st: &GameState, options: &[Action]) -> Vec<f64> {
    let in_battle = !st.battles().is_empty();
    per_option(options, |o| match o.kind {
        ActionKind::Pass => 0.0,
        ActionKind::PlayCommand => {
            (if in_battle { 0.3 } else { -1.0 }) - EX_PENALTY * f64::from(ex_used(*o))
        }
        _ => (if in_battle { 0.2 } else { -0.5 }) - EX_PENALTY * f64::from(ex_used(*o)),
    })
}

// ---------------------------------------------------------------------------------------------
// choices inside effects (ai/policy.py:305-385)

/// `_excess(st, dec, knobs)` (`ai/policy.py:321-323`): trash the least valuable card.
fn excess(st: &GameState, options: &[Action]) -> Result<Vec<f64>, EngineError> {
    let dv = view::derived(st)?;
    options
        .iter()
        .map(|o| Ok(-pile_value(st, &dv, uid_of(st, o.a)?)))
        .collect()
}

/// `_discard(st, dec, knobs)` (`ai/policy.py:326-331`).
fn discard(st: &GameState, player: u8, options: &[Action]) -> Result<Vec<f64>, EngineError> {
    options
        .iter()
        .map(|o| {
            let uid = uid_of(st, o.a)?;
            let worth = card_worth(st.def_of(uid));
            Ok(if st.card(uid).owner == player {
                -worth
            } else {
                worth
            })
        })
        .collect()
}

/// `_arrange(st, dec, knobs)` (`ai/policy.py:334-341`): the "next card (top first)" prompt
/// ranks cards by worth; the per-card prompt keeps a card worth at least
/// [`KEEPABLE_TOP`] on top; the all-cards prompt slightly prefers the top.
fn arrange(
    st: &GameState,
    dec: &DecisionView<'_>,
    options: &[Action],
) -> Result<Vec<f64>, EngineError> {
    let player = dec.hdr.player;
    if dec.hdr.ctx(CtxKey::Order) == 1 {
        return options
            .iter()
            .map(|o| Ok(deck_worth(st, uid_of(st, o.a)?, player)))
            .collect();
    }
    let card = dec.hdr.ctx(CtxKey::Card);
    if card < 0 {
        return Ok(per_option(options, |o| if o.a == 0 { 0.1 } else { 0.0 }));
    }
    let keep_on_top = deck_worth(st, uid_of(st, card)?, player) >= KEEPABLE_TOP;
    Ok(per_option(options, |o| flag((o.a == 0) == keep_on_top)))
}

/// `_deck_worth(st, uid, chooser)` (`ai/policy.py:344-346`).
fn deck_worth(st: &GameState, uid: Uid, chooser: u8) -> f64 {
    let worth = card_worth(st.def_of(uid));
    if st.card(uid).owner == chooser {
        worth
    } else {
        -worth
    }
}

/// `_select(st, dec, knobs)` (`ai/policy.py:349-378`).
fn select(
    st: &GameState,
    dec: &DecisionView<'_>,
    options: &[Action],
    knobs: &PolicyKnobs,
) -> Result<Vec<f64>, EngineError> {
    if dec.hdr.ctx(CtxKey::Mode) == 1 {
        return Ok(vec![0.0; options.len()]);
    }
    let dv = view::derived(st)?;
    let player = dec.hdr.player;
    let protected = dec.hdr.ctx(CtxKey::RestSub);
    if protected >= 0 {
        let keep = pile_value(st, &dv, uid_of(st, protected)?);
        return options
            .iter()
            .map(|o| {
                if o.kind == ActionKind::Done {
                    return Ok(0.0);
                }
                Ok(keep - pile_value(st, &dv, uid_of(st, o.a)?) - 0.5)
            })
            .collect();
    }
    let polarity = targeting::choice_polarity(st, knobs.rule_polarity_table);
    let battling: Option<Option<(i32, i32)>> = (knobs.rule_polarity_table
        && targeting::choice_rule_lasts_this_battle(st))
    .then(|| st.battle().map(|b| (b.attacker, b.target)));
    let n_cards = st.cards().len();
    let is_card = |o: &Action| {
        o.kind == ActionKind::Select && usize::try_from(o.a).is_ok_and(|a| a < n_cards)
    };
    let mut scores = Vec::with_capacity(options.len());
    for o in options {
        if !is_card(o) {
            scores.push(done_score(st, player, options, polarity));
            continue;
        }
        let uid = uid_of(st, o.a)?;
        let worth = pile_value(st, &dv, uid);
        let mine = st.card(uid).owner == player;
        let mut wants = if mine {
            polarity == Polarity::Helps
        } else {
            polarity != Polarity::Helps
        };
        if mine && polarity == Polarity::Unknown {
            wants = true;
        }
        let outside_battle = battling.is_some_and(|pair| {
            mine && pair.is_none_or(|(attacker, target)| o.a != attacker && o.a != target)
        });
        scores.push(if outside_battle || !wants {
            -worth
        } else {
            worth
        });
    }
    Ok(scores)
}

/// `_done_score(st, dec, polarity)` (`ai/policy.py:381-385`): the score of a non-card option
/// (DONE). Only the first card candidate decides whose cards are offered.
fn done_score(st: &GameState, player: u8, options: &[Action], polarity: Polarity) -> f64 {
    let n_cards = st.cards().len();
    let first = options.iter().find_map(|o| {
        (o.kind == ActionKind::Select)
            .then(|| usize::try_from(o.a).ok().filter(|&a| a < n_cards))
            .flatten()
    });
    let mine = first.is_some_and(|a| st.cards()[a].owner == player);
    let unwanted = (mine && polarity == Polarity::Harms) || (!mine && polarity == Polarity::Helps);
    if unwanted { 1.0 } else { -0.25 }
}
