//! Target choices (SELECT outside a mode choice): the reference scorer (`_select`,
//! `ai/policy.py:349-385`, copied line by line from `gcg_ai::policy`) with the exhaustive
//! polarity of W-30 and the kill-aware damage targets of W-26.
//!
//! - `exhaustive_polarity`: the choice is signed by `polarity::choice_polarity`; a choice that
//!   stays UNKNOWN is scored by the look-ahead (each candidate resolved on a clone).
//! - `kill_aware_targeting`: when the choice is a harmful damage step with a literal amount,
//!   an enemy Unit the damage destroys scores its worth plus [`KILL_BONUS`], and one it does
//!   not destroy scores the share of its remaining HP the damage removes, times half its
//!   worth (the measured-and-removed Python setting re-implemented for the re-measurement).

use gcg_ai::targeting::{self, Polarity};
use gcg_ai::values::{hp_left, int_f64, pile_value};
use gcg_engine::{
    Action, ActionKind, CtxKey, DecisionView, EngineError, GameState, Uid, Zone, view,
};

use super::lookahead::Until;
use super::polarity;
use super::scorer::Scoring;

/// The extra score of a damage target that the damage destroys (W-26).
pub(crate) const KILL_BONUS: f64 = 1.0;

fn uid_of(st: &GameState, x: i32) -> Result<Uid, EngineError> {
    Uid::try_from(x)
        .ok()
        .filter(|&u| usize::from(u) < st.cards().len())
        .ok_or_else(|| EngineError::Index("list index out of range".into()))
}

/// The scores of a SELECT decision that is not a mode choice.
///
/// # Errors
/// The derived view's errors, an option that is no uid where one is needed, and the engine's
/// errors inside a look-ahead.
pub(crate) fn select_scores(
    sc: &Scoring<'_>,
    st: &GameState,
    dec: &DecisionView<'_>,
    options: &[Action],
) -> Result<Vec<f64>, EngineError> {
    let knobs = sc.knobs;
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
    let polarity = if sc.flags.exhaustive_polarity {
        polarity::choice_polarity(st)
    } else {
        targeting::choice_polarity(st, knobs.rule_polarity_table)
    };
    if sc.flags.exhaustive_polarity
        && polarity == Polarity::Unknown
        && st.frames().last().is_some_and(|f| f.waiting)
    {
        return sc.by_lookahead(st, dec, options, Until::Effect);
    }
    let damage = (sc.flags.kill_aware_targeting && polarity == Polarity::Harms)
        .then(|| polarity::choice_damage(st))
        .flatten();
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
        let score = if outside_battle || !wants {
            -worth
        } else if let Some(amount) = damage.filter(|_| !mine && st.card(uid).zone == Zone::Battle) {
            let left = hp_left(st, &dv, uid);
            if amount >= left {
                worth + KILL_BONUS
            } else {
                0.5 * worth * int_f64(amount) / int_f64(left)
            }
        } else {
            worth
        };
        scores.push(score);
    }
    Ok(scores)
}

/// `_done_score(st, dec, polarity)` (`ai/policy.py:381-385`).
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
