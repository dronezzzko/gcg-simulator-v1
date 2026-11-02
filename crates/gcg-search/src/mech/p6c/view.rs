//! `opponent_view_share` (W-139, PLAN §12.3 S4): opponent nodes stop conditioning on the
//! searcher's private cards.
//!
//! The reference determinises only the cards the searcher cannot identify
//! (`engine/observe.py:56-83`), so the searcher's hand is the same in every world, and the
//! opponent's tree nodes learn their statistics against it: the opponent never walks into a
//! trick the searcher really holds, and holding one looks worthless.
//!
//! A share `σ` of the worlds is re-determinised from the opponent's view: after the usual world
//! (the searcher's view), every card hidden from the opponent (the searcher's hand, deck,
//! Shields and resource deck, and the opponent's own deck and Shields) is redrawn by
//! `determinize(world, opponent, seed)`, from the opponent's pools. World k is such a world when
//! `floor((k + 1) σ) > floor(k σ)` for `k >= 1` (world 0, which sets the root priors, is never
//! one; with `σ = 1/4` they are worlds 3, 7, 11, ...). Its seed is
//! `derive_seed(W[k], "opponent-view")`.
//!
//! The iterations played in those worlds train the opponent's nodes only (`p6c::backup`): an
//! opponent child gains their values in its pooled statistics and in its opponent-view slice,
//! the searcher's own children do not learn from them (the root child gains its visit, which
//! indexes the worlds, at its current mean). An opponent node selects by its opponent-view slice
//! once that has [`MIN_VISITS`] visits, in every iteration, so the searcher's own lines face an
//! opponent that plays from its information set.
//!
//! Root moves name cards by uid. A redrawn world gives the searcher's hidden hand cards other
//! identities, so opponent-view worlds are used only at decisions where no option of the root
//! decision names a card hidden from the opponent ([`root_allows`]); elsewhere every world is
//! the usual one. Keeping the named cards' identities while redrawing the rest needs an engine
//! API to set hidden identities, which the engine does not offer (`p6c/mod.rs`).

use gcg_core::rng::{SeedPart, derive_seed};
use gcg_engine::observe::{determinize, is_hidden_from};
use gcg_engine::{DecisionView, EngineError, GameState, Uid};

/// The opponent-view visits an opponent node needs before its slice drives selection.
pub(crate) const MIN_VISITS: u32 = 8;

/// Whether world `k` is an opponent-view world at share `share`.
pub(crate) fn is_view_world(k: usize, share: f64) -> bool {
    if k == 0 || share <= 0.0 {
        return false;
    }
    let k = f64::from(u32::try_from(k).unwrap_or(u32::MAX));
    ((k + 1.0) * share).floor() > (k * share).floor()
}

/// Whether no option of the root decision `dec` names (as its first or second argument) a card
/// that the opponent of `player` cannot identify.
pub(crate) fn root_allows(st: &GameState, dec: &DecisionView<'_>, player: u8) -> bool {
    let opponent = 1 - player;
    let n = st.cards().len();
    let hidden = |x: i32| {
        Uid::try_from(x)
            .ok()
            .filter(|&u| usize::from(u) < n)
            .is_some_and(|u| is_hidden_from(st, u, opponent))
    };
    !dec.options.iter().any(|o| hidden(o.a) || hidden(o.b))
}

/// The world `out` (a world of `player`'s information set) re-determinised from the
/// opponent's view with the seed of `world_seed`.
///
/// # Errors
/// `determinize`'s error.
pub(crate) fn redraw(out: &mut GameState, player: u8, world_seed: u64) -> Result<(), EngineError> {
    let seed = derive_seed(world_seed, &[SeedPart::Str("opponent-view")]);
    *out = determinize(out, 1 - player, seed)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_worlds_follow_the_share() {
        let quarter: Vec<usize> = (0..16).filter(|&k| is_view_world(k, 0.25)).collect();
        assert_eq!(quarter, [3, 7, 11, 15]);
        let half: Vec<usize> = (0..8).filter(|&k| is_view_world(k, 0.5)).collect();
        assert_eq!(half, [1, 3, 5, 7]);
        assert!((0..64).all(|k| !is_view_world(k, 0.0)));
        assert!(!is_view_world(0, 1.0));
        assert!((1..64).all(|k| is_view_world(k, 1.0)));
        // The share of view worlds among the first n approaches the share.
        let n = (0..400).filter(|&k| is_view_world(k, 0.25)).count();
        assert_eq!(n, 100);
    }
}
