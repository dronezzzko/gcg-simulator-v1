//! `opponent_buckets{m}` (W-05, PLAN §12.3 S4): partial multiple-observer opponent nodes.
//!
//! The reference's single-observer tree pools an opponent node's statistics over worlds in
//! which the opponent holds different cards (`docs/python/AI.md:105-110`), so its reply to a move is
//! one reply for every hand it might hold (strategy fusion). Here each opponent node keeps, per
//! child, one slice of the statistics for each of at most four buckets of the opponent's
//! determinised hand in the iteration's world:
//!
//! - bit 1: the hand holds an action-step trick it can cast now (a Command with 【Action】 or
//!   【Main】/【Action】 timing whose level and cost the opponent's Resources cover);
//! - bit 0: the hand holds a <Blocker> Unit card it can deploy now (level and cost covered).
//!
//! Castability is judged from the printed level and cost and the command's timing (conditions
//! and targets are not checked), so the abstraction is cheap and depends on the world's hand and
//! the opponent's Resources only. A bucket's slice drives the node's selection once the bucket
//! has `m` visits at that node; below that the node's pooled statistics do (the reference's
//! selection), so no reply starves (`p6c::select`).

use gcg_data::{CardType, DefId};
use gcg_effects::{Kw, Node, Registry, Timing};
use gcg_engine::{GameState, Zone};

/// The number of buckets.
pub(crate) const BUCKETS: usize = 4;

/// The command timing of card `def`, if it is a card with a 【Main】/【Action】 command.
pub(crate) fn command_timing(reg: &Registry, def: DefId) -> Option<Timing> {
    let aid = reg.card(def).command_aid?;
    match reg.node(reg.ability(aid).ability.node()) {
        Node::Command(c) => Some(c.timing),
        _ => None,
    }
}

/// The level (Resources in the Resource Area) and the active Resources of `player`.
pub(crate) fn resources(s: &GameState, player: u8) -> (i32, i32) {
    let area = s.zone(player, Zone::ResourceArea);
    let open = area.iter().filter(|&&u| !s.card(u).rested).count();
    (
        i32::try_from(area.len()).unwrap_or(i32::MAX),
        i32::try_from(open).unwrap_or(i32::MAX),
    )
}

/// The bucket of `opponent`'s hand in the world `s`.
pub(crate) fn bucket(s: &GameState, opponent: u8) -> u8 {
    let (level, open) = resources(s, opponent);
    let reg = s.registry();
    let (mut trick, mut blocker) = (false, false);
    for &u in s.zone(opponent, Zone::Hand) {
        let cd = s.def_of(u);
        if cd.level > level || cd.cost > open {
            continue;
        }
        match cd.card_type {
            CardType::Command => {
                trick |= matches!(
                    command_timing(reg, cd.def_id),
                    Some(Timing::Action | Timing::MainOrAction)
                );
            }
            CardType::Unit => {
                blocker |= reg
                    .card(cd.def_id)
                    .printed_keywords
                    .get(Kw::Blocker)
                    .is_some();
            }
            _ => {}
        }
    }
    (u8::from(trick) << 1) | u8::from(blocker)
}
