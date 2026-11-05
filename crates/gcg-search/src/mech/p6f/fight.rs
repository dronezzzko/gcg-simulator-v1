//! Exact fights (`exact_fight`, W-31): the attack and block scorers of the reference policy
//! (`_attack`, `_block_risk`, `_block`, `ai/policy.py:209-285`, ported from `gcg_ai::policy`)
//! with every `fight()` (`ai/values.py:65-71`: AP against remaining HP and First Strike only)
//! replaced by the engine's own resolution of the battle on a clone. The clone plays the
//! attack (or the block) and the rest of the battle: the forced block or none, passes in the
//! action steps, the reference policy's best option elsewhere; the fight's result is which of
//! the two Units left the battle area. Damage prevention and reduction, First Strike, Breach,
//! redirects and battle triggers are therefore exact.
//!
//! Every other MAIN option keeps the reference score (`gcg_ai::option_scores`).

use std::cell::RefCell;

use gcg_ai::actions::attack_can_matter;
use gcg_ai::policy::{IDLE_ATTACK, LETHAL, MUST_BLOCK};
use gcg_ai::values::{Fight, exposed, hp_left, life, ready_blockers, unit_value};
use gcg_ai::{PolicyKnobs, apply_unchecked, choose, option_scores};
use gcg_effects::{Kw, RuleKind};
use gcg_engine::{
    Action, ActionKind, DecisionKind, DecisionView, Derived, EngineError, GameState, PLAYER_TARGET,
    Uid, Zone, view,
};

use super::lookahead::MAX_RESOLVE_APPLIES;
use super::scorer::Scoring;

fn uid_of(st: &GameState, x: i32) -> Result<Uid, EngineError> {
    Uid::try_from(x)
        .ok()
        .filter(|&u| usize::from(u) < st.cards().len())
        .ok_or_else(|| EngineError::Index("list index out of range".into()))
}

/// The outcome of one simulated battle: the fight between the attacker and the Unit it
/// battled, and whether the forced blocker blocked.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Battled {
    pub(crate) fight: Fight,
    pub(crate) blocked: bool,
}

/// Plays `start` (an ATTACK from MAIN, or an answer of the pending BLOCK) on a clone of `st`,
/// then the rest of the battle: BLOCK by `blocker` when offered (otherwise no block), PASS in
/// action steps, the reference policy's best option elsewhere. `None` once the decision's
/// auxiliary budget is spent.
///
/// # Errors
/// The engine's errors on the clone.
pub(crate) fn simulate(
    sc: &Scoring<'_>,
    st: &GameState,
    start: Action,
    attacker: Uid,
    defender: Option<Uid>,
    blocker: Option<Uid>,
) -> Result<Option<Battled>, EngineError> {
    if !sc.state.has_budget() {
        return Ok(None);
    }
    let mut t = st.clone();
    let mut blocked = start.kind == ActionKind::Block;
    let mut applies = 0;
    let mut next = Some(start);
    while let Some(a) = next.take() {
        apply_unchecked(&mut t, a)?;
        applies += 1;
        if applies > MAX_RESOLVE_APPLIES || t.winner().is_some() || t.battle().is_none() {
            break;
        }
        let Some(dec) = t.pending() else { break };
        let find = |k: ActionKind, uid: Option<Uid>| {
            dec.options
                .iter()
                .copied()
                .find(|o| o.kind == k && uid.is_none_or(|u| o.a == i32::from(u)))
        };
        next = Some(match dec.hdr.kind {
            DecisionKind::Main => break,
            DecisionKind::Block => match blocker.and_then(|b| find(ActionKind::Block, Some(b))) {
                Some(b) => {
                    blocked = true;
                    b
                }
                None => match find(ActionKind::NoBlock, None) {
                    Some(n) => n,
                    None => choose(&t, None, 0.0, &sc.knobs)?,
                },
            },
            DecisionKind::ActionStep => match find(ActionKind::Pass, None) {
                Some(p) => p,
                None => choose(&t, None, 0.0, &sc.knobs)?,
            },
            _ => choose(&t, None, 0.0, &sc.knobs)?,
        });
    }
    sc.state.spend(u64::from(applies));
    let gone = |u: Uid| t.card(u).zone != Zone::Battle;
    let foe = if blocked { blocker } else { defender };
    Ok(Some(Battled {
        fight: Fight {
            kills: foe.is_some_and(gone),
            dies: gone(attacker),
        },
        blocked,
    }))
}

/// `_Board` (`ai/policy.py:115-130`) for the attack scorer.
struct Board {
    player: u8,
    enemy_blockers: Vec<Uid>,
    enemy_exposed: bool,
    enemy_has_base: bool,
    enemy_max_ap: i32,
    attackers: usize,
    knobs: PolicyKnobs,
}

impl Board {
    const fn blocked_from_lethal(&self) -> bool {
        self.enemy_exposed && 0 < self.attackers && self.attackers <= self.enemy_blockers.len()
    }
}

fn board(
    st: &GameState,
    dv: &Derived,
    player: u8,
    options: &[Action],
    knobs: PolicyKnobs,
) -> Result<Board, EngineError> {
    let enemy = 1 - player;
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
        enemy_blockers: ready_blockers(st, dv, enemy, -1),
        enemy_exposed: exposed(st, enemy),
        enemy_has_base: !st.zone(enemy, Zone::Base).is_empty(),
        enemy_max_ap: st
            .zone(enemy, Zone::Battle)
            .iter()
            .map(|&u| view::ap_of(st, dv, u))
            .max()
            .unwrap_or(0),
        attackers: attackers.len(),
        knobs,
    })
}

/// `_unblockable` (`ai/policy.py:209-212`).
fn unblockable(dv: &Derived, attacker: Uid) -> bool {
    view::has_kw(dv, attacker, Kw::HighManeuver)
        || view::rules_of(dv, attacker, RuleKind::CantBeBlocked)
            .next()
            .is_some()
}

/// The MAIN scores with exact fights: the reference's, with every ATTACK rescored.
///
/// # Errors
/// The reference scorer's errors and the engine's errors on the clones.
pub(crate) fn main_scores(
    sc: &Scoring<'_>,
    st: &GameState,
    dec: &DecisionView<'_>,
    options: &[Action],
) -> Result<Vec<f64>, EngineError> {
    let mut out = option_scores(st, dec, options, &sc.knobs)?;
    if !options.iter().any(|o| o.kind == ActionKind::Attack) {
        return Ok(out);
    }
    let dv = view::derived(st)?;
    let b = board(st, &dv, dec.hdr.player, options, sc.knobs)?;
    let cache = RefCell::new(Vec::new());
    for (i, o) in options.iter().enumerate() {
        if o.kind != ActionKind::Attack {
            continue;
        }
        if let Some(s) = attack(sc, st, &dv, &b, *o, &cache)? {
            out[i] = s;
        }
    }
    Ok(out)
}

type Cache = RefCell<Vec<((Action, Option<Uid>), Option<Battled>)>>;

/// [`simulate`] memoised per `(attack, blocker)` within one scoring.
fn battle_of(
    sc: &Scoring<'_>,
    st: &GameState,
    attack: Action,
    blocker: Option<Uid>,
    cache: &Cache,
) -> Result<Option<Battled>, EngineError> {
    if let Some(&(_, r)) = cache.borrow().iter().find(|(k, _)| *k == (attack, blocker)) {
        return Ok(r);
    }
    let attacker = uid_of(st, attack.a)?;
    let defender = (attack.b != PLAYER_TARGET)
        .then(|| uid_of(st, attack.b))
        .transpose()?;
    let r = simulate(sc, st, attack, attacker, defender, blocker)?;
    cache.borrow_mut().push(((attack, blocker), r));
    Ok(r)
}

/// `_attack` (`ai/policy.py:230-250`) with exact fights; `None` once the budget is spent.
fn attack(
    sc: &Scoring<'_>,
    st: &GameState,
    dv: &Derived,
    b: &Board,
    o: Action,
    cache: &Cache,
) -> Result<Option<f64>, EngineError> {
    let attacker = uid_of(st, o.a)?;
    let target = o.b;
    let Some(risk) = block_risk(sc, st, dv, b, o, cache)? else {
        return Ok(None);
    };
    if target == PLAYER_TARGET {
        if b.knobs.zero_ap_face && !attack_can_matter(st, b.player, attacker)? {
            return Ok(Some(IDLE_ATTACK));
        }
        if b.enemy_exposed {
            if unblockable(dv, attacker) || b.enemy_blockers.is_empty() {
                return Ok(Some(LETHAL));
            }
            return Ok(Some(if b.blocked_from_lethal() {
                2.0
            } else {
                LETHAL / 2.0 - risk
            }));
        }
        let mut s = 2.5 - if b.enemy_has_base { 0.3 } else { 0.0 };
        if i64::from(b.enemy_max_ap) >= hp_left(st, dv, attacker) {
            s -= 0.4;
        }
        return Ok(Some(s - risk));
    }
    let defender = uid_of(st, target)?;
    let Some(battled) = battle_of(sc, st, o, None, cache)? else {
        return Ok(None);
    };
    let f = battled.fight;
    let mut s = if f.kills {
        3.0 + 0.5 * unit_value(st, dv, defender)
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
    Ok(Some(s - risk))
}

/// `_block_risk` (`ai/policy.py:215-227`) with exact fights: the worst loss a Blocker other
/// than the target inflicts by blocking (a Blocker the engine does not let block adds none).
fn block_risk(
    sc: &Scoring<'_>,
    st: &GameState,
    dv: &Derived,
    b: &Board,
    o: Action,
    cache: &Cache,
) -> Result<Option<f64>, EngineError> {
    let attacker = uid_of(st, o.a)?;
    if unblockable(dv, attacker) {
        return Ok(Some(0.0));
    }
    let mut worst = 0.0;
    let a_val = unit_value(st, dv, attacker);
    for &bl in &b.enemy_blockers {
        if i32::from(bl) == o.b {
            continue;
        }
        let Some(battled) = battle_of(sc, st, o, Some(bl), cache)? else {
            return Ok(None);
        };
        let f = battled.fight;
        if battled.blocked && f.dies {
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
    Ok(Some(worst))
}

/// `_block` (`ai/policy.py:257-285`) with exact fights; the reference scores once the budget
/// is spent.
///
/// # Errors
/// `EngineError::Assertion` outside a battle, the derived view's errors and the engine's
/// errors on the clones.
pub(crate) fn block_scores(
    sc: &Scoring<'_>,
    st: &GameState,
    dec: &DecisionView<'_>,
    options: &[Action],
) -> Result<Vec<f64>, EngineError> {
    let Some(battle) = st.battle() else {
        return Err(EngineError::Assertion(String::new()));
    };
    let player = dec.hdr.player;
    let dv = view::derived(st)?;
    let attacker = uid_of(st, battle.attacker)?;
    let target = battle.target;
    let at_player = target == PLAYER_TARGET || st.card(uid_of(st, target)?).zone == Zone::Base;
    let reference = || option_scores(st, dec, options, &sc.knobs);
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
        let defender = uid_of(st, target)?;
        let Some(no_block) = options
            .iter()
            .copied()
            .find(|o| o.kind == ActionKind::NoBlock)
        else {
            return reference();
        };
        let Some(r) = simulate(sc, st, no_block, attacker, Some(defender), None)? else {
            return reference();
        };
        if r.fight.kills {
            1.0 + 0.5 * unit_value(st, &dv, defender)
        } else {
            0.0
        }
    };
    let mut out = Vec::with_capacity(options.len());
    for &o in options {
        if o.kind != ActionKind::Block {
            out.push(0.0);
            continue;
        }
        let blocker = uid_of(st, o.a)?;
        let Some(r) = simulate(sc, st, o, attacker, None, Some(blocker))? else {
            return reference();
        };
        let mut s = saved;
        if r.fight.kills {
            s -= 1.0 + 0.5 * unit_value(st, &dv, blocker);
        }
        if r.fight.dies {
            s += 1.0 + 0.5 * unit_value(st, &dv, attacker);
        }
        out.push(s);
    }
    Ok(out)
}
