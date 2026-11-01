//! Step hooks of `effects/bindings/wp_gd05_b.py` (#28, #29).

use gcg_effects::Ev;

use super::slice_stop;
use crate::core::{self as core_};
use crate::interp::to_uid;
use crate::{DamageOpts, EngineError, FrameKind, GameState, MoveOpts, Uid, Zone};

/// `wp_gd05_b_destroy_first_shield_area_cards` (`wp_gd05_b.py:70-87`): destroys the first
/// `count` cards of the opponent's shield area, the Base then the top Shields, simultaneously
/// (rulings GD05-107:Q405-Q407). Both lists are sliced before anything is destroyed.
pub(super) fn destroy_first_shield_area_cards(
    st: &mut GameState,
    fi: usize,
    count: i64,
) -> Result<bool, EngineError> {
    let (controller, host) = (st.frames[fi].controller, st.frames[fi].host);
    let p = 1 - controller;
    let base_list = st.zone(p, Zone::Base);
    let base: Vec<Uid> = base_list[..slice_stop(base_list.len(), count)].to_vec();
    let rest = count - i64::try_from(base.len()).unwrap_or(i64::MAX);
    let shield_list = st.zone(p, Zone::Shield);
    let shields: Vec<Uid> = shield_list[..slice_stop(shield_list.len(), rest)].to_vec();
    let mut did = false;
    let by = i32::from(controller);
    if !base.is_empty() && !core_::destroy(st, &base, false, by, host)?.is_empty() {
        did = true;
    }
    if !shields.is_empty() {
        core_::destroy_shields(
            st,
            p,
            &shields,
            DamageOpts {
                source: host,
                battle: false,
                by,
            },
        )?;
        did = true;
    }
    Ok(did)
}

/// `wp_gd05_b_resolving_command_to_trash` (`wp_gd05_b.py:90-103`): ends a played Command's
/// 【Main】 by placing it into the trash (rule 3-4-4), so that "after activating this card's
/// 【Main】, you may pair this card from your trash" can follow in the same resolution
/// (rulings GD05-112:Q409, GD05-113:Q410, GD05-121:Q414, GD05-122:Q415); a `command` frame
/// also announces COMMAND_RESOLVED. False when the card is not resolving.
pub(super) fn resolving_command_to_trash(
    st: &mut GameState,
    fi: usize,
) -> Result<bool, EngineError> {
    let f = &st.frames[fi];
    let (card_uid, kind, controller) = (f.card_uid, f.kind, f.controller);
    let uid = to_uid(st, card_uid)?;
    if st.cards[usize::from(uid)].zone != Zone::Resolving {
        return Ok(false);
    }
    core_::move_card(st, uid, Zone::Trash, MoveOpts::default())?;
    if kind == FrameKind::Command {
        core_::emit(
            st,
            Ev::CommandResolved,
            card_uid,
            i32::from(controller),
            None,
            &[],
        )?;
    }
    Ok(true)
}
